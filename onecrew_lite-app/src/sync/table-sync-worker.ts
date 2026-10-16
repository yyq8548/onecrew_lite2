import { createHash, randomUUID } from "node:crypto";
import { z } from "zod";
import { creatorU64 } from "@onecrew/runtime-contracts/creators";
import type { AccountContext, AccountContextService } from "../identity/account-context.ts";
import type { DiscoveryAdmissionGate } from "../discovery/admission-gate.ts";
import { TableSyncClient, TableSyncError, type TableSyncRequest, type TableSyncResponse } from "./table-sync-client.ts";
import { TableSyncStore, registerTableSyncResult, resumeTableSyncConsent, tableSyncKey, tableSyncQueued, type TableSyncState, type TableSyncTable } from "./table-sync-store.ts";
import { tableSyncPartBytes, validateTableSyncArchive, validateTableSyncManifest, validateTableSyncPending } from "./table-sync-integrity.ts";

export interface TableSyncCaptureSource {
  capture(context: AccountContext, campaign: string, metadata: { version_id: string; source_instance_id: string; consent_id: string; capture_started_at: string }): Promise<{ archive: string; content_hash: string; record_count: number; table_id: string; table_name: string }>;
}
const object = z.record(z.string(), z.unknown()), uuid = z.uuid();
const now = () => BigInt(Math.floor(Date.now() / 1000));
const owner = (context: AccountContext) => ({ accountId: context.principal.user.id, tenantId: context.principal.tenantId, baseId: context.principal.baseId });
const success = (reply: TableSyncResponse) => { if (reply.status < 200 || reply.status >= 300) throw new TableSyncError(`table_sync_http_${reply.status}`); return object.parse(reply.body); };
function identity(capabilities: Record<string, unknown>, account: AccountContext) {
  const value = z.object({ access_user_id: z.string().min(1), source_instance_id: uuid }).passthrough().parse(capabilities.identity);
  if (value.access_user_id !== account.principal.user.id) throw new TableSyncError("table_sync_account_changed");
  return value;
}
function consent(capabilities: Record<string, unknown>): string {
  if (capabilities.upload_enabled !== true || capabilities.consent_current !== true) throw new TableSyncError("table_sync_consent_required");
  const value = z.object({ status: z.literal("granted"), consent_id: uuid, scopes: z.array(z.string()) }).passthrough().parse(capabilities.consent);
  if (!value.scopes.includes("dynamic_table_schema") || !value.scopes.includes("dynamic_table_records")) throw new TableSyncError("table_sync_consent_required");
  return value.consent_id;
}

// 每个远端写入前持久化原始捕获；未知结果先查询原 version，不能创建替代上传。
export class TableSyncWorker {
  readonly #accounts: AccountContextService;
  readonly #store: TableSyncStore;
  readonly #client: TableSyncClient;
  readonly #gate: DiscoveryAdmissionGate;
  readonly #capture: TableSyncCaptureSource;
  readonly #blocksBackendSync: () => boolean;
  readonly #startupReconciled = new Set<string>();
  #retryAfter = 0n;
  #closed = false;
  constructor(accounts: AccountContextService, store: TableSyncStore, client: TableSyncClient, gate: DiscoveryAdmissionGate, capture: TableSyncCaptureSource, blocksBackendSync: () => boolean) {
    this.#accounts = accounts; this.#store = store; this.#client = client; this.#gate = gate; this.#capture = capture; this.#blocksBackendSync = blocksBackendSync;
  }
  #assert(context: AccountContext): void { if (this.#closed) throw new TableSyncError("table_sync_stopping"); this.#accounts.assertCurrent(context); }
  async #send(context: AccountContext, input: TableSyncRequest) {
    this.#assert(context);
    if (!["consent", "set_consent"].includes(input.operation) && now() < this.#retryAfter) throw new TableSyncError("table_sync_backoff");
    const reply = await this.#client.send(owner(context), input, context.signal);
    this.#assert(context);
    if (reply.status === 429 || reply.status === 503) {
      const value = reply.retry_after && /^\d+$/u.test(reply.retry_after) ? creatorU64.safeParse(BigInt(reply.retry_after)) : null;
      const delay = value?.success ? BigInt(value.data) : 300n, deadline = now() + delay;
      this.#retryAfter = this.#retryAfter > deadline ? this.#retryAfter : deadline;
    }
    return reply;
  }
  async #capabilities(context: AccountContext) {
    const cap = success(await this.#send(context, { operation: "capabilities" })), limits = object.parse(cap.limits);
    if (cap.protocol !== "onecrew.dynamic_table_sync" || cap.protocol_version !== 1 || limits.part_bytes !== tableSyncPartBytes || limits.collection_interval_seconds !== 300) throw new TableSyncError("table_sync_protocol_mismatch");
    identity(cap, context); return cap;
  }
  async registerResult(context: AccountContext, campaign: string): Promise<void> {
    this.#assert(context);
    await this.#store.exclusive(context, async (state, checkpoint) => { if (registerTableSyncResult(state, context.principal.user.id, campaign)) await checkpoint(); });
  }
  async searchSettled(context: AccountContext, campaign: string): Promise<void> {
    this.#assert(context);
    await this.#store.exclusive(context, async (state, checkpoint) => { const value = state.tables[tableSyncKey(context.principal.user.id, campaign)]; if (value?.enabled) { value.next_check = 0n; value.status = "waiting"; await checkpoint(); } });
  }
  async status(context: AccountContext, campaign: string) {
    const capabilities = await this.#capabilities(context);
    return this.#store.exclusive(context, async state => { const value = state.tables[tableSyncKey(context.principal.user.id, campaign)]; return { capabilities, enabled: value?.enabled ?? false, status: value?.status ?? "disabled", error: value?.error ?? null, version_id: value?.previous_version ?? null, next_check: value?.next_check ?? null }; });
  }
  async configure(context: AccountContext, campaign: string, enabled: boolean) {
    this.#assert(context);
    const held = await this.#gate.acquire(context, context.signal);
    using release = { [Symbol.dispose]: () => held.release() };
    if (enabled) consent(await this.#capabilities(context));
    return this.#store.exclusive(context, async (state, checkpoint) => {
      const key = tableSyncKey(context.principal.user.id, campaign);
      if (enabled && !state.tables[key]) registerTableSyncResult(state, context.principal.user.id, campaign);
      const value = state.tables[key];
      if (value) { value.enabled = enabled; value.failures = 0; value.next_check = 0n; value.status = enabled ? "waiting" : "disabled"; }
      await checkpoint(); return { enabled };
    });
  }
  async changeConsent(context: AccountContext, input: { action: "grant" | "revoke"; operation_id: string; policy_fingerprint: string | null }) {
    this.#assert(context); uuid.parse(input.operation_id);
    const held = await this.#gate.acquire(context, context.signal);
    using release = { [Symbol.dispose]: () => held.release() };
    return this.#store.exclusive(context, async (state, checkpoint) => {
      if (input.action === "revoke") { this.#disable(state); await checkpoint(); }
      let body: Record<string, unknown>;
      if (input.action === "grant") {
        const cap = await this.#capabilities(context);
        if (cap.consent_management_enabled !== true || cap.policy_fingerprint !== input.policy_fingerprint) throw new TableSyncError("table_sync_policy_changed");
        body = { action: "grant", operation_id: input.operation_id, policy_fingerprint: input.policy_fingerprint, scopes: ["dynamic_table_records", "dynamic_table_schema"] };
      } else body = { action: "revoke", operation_id: input.operation_id };
      const current = success(await this.#send(context, { operation: "consent" }));
      const existing = current.consent === null ? null : object.parse(current.consent), user = context.principal.user.id;
      if (existing?.identity !== undefined && object.parse(existing.identity).access_user_id !== user) throw new TableSyncError("table_sync_account_changed");
      const saved = state.consent_operation;
      if (state.consent_user === user && saved && saved.action === body.action && (saved.policy_fingerprint ?? null) === (body.policy_fingerprint ?? null)) {
        body = saved;
        if (existing?.operation_id === body.operation_id) { state.consent_operation = null; state.consent_user = null; if (input.action === "revoke") this.#disable(state); else resumeTableSyncConsent(state, user); await checkpoint(); return { consent: current.consent }; }
      }
      state.consent_operation = body; state.consent_user = user; await checkpoint();
      const reply = await this.#send(context, { operation: "set_consent", input: body, etag: typeof current.consent_etag === "string" ? current.consent_etag : null });
      if (reply.status === 412) { state.consent_operation = null; state.consent_user = null; await checkpoint(); throw new TableSyncError("table_sync_consent_changed"); }
      const result = success(reply), receipt = object.parse(result.consent);
      if (receipt.status !== (input.action === "grant" ? "granted" : "revoked") || receipt.operation_id !== body.operation_id) throw new TableSyncError("table_sync_consent_unconfirmed");
      state.consent_operation = null; state.consent_user = null;
      if (input.action === "revoke") this.#disable(state); else resumeTableSyncConsent(state, user);
      await checkpoint(); return result;
    });
  }
  #disable(state: TableSyncState): void { for (const value of Object.values(state.tables)) { value.enabled = false; value.status = "consent_revoked"; } }
  async tick(context: AccountContext): Promise<void> {
    this.#assert(context);
    const held = await this.#gate.acquire(context, context.signal);
    using release = { [Symbol.dispose]: () => held.release() };
    if (this.#blocksBackendSync()) return;
    await this.#store.exclusive(context, async (state, checkpoint) => {
      const user = context.principal.user.id, startupKey = JSON.stringify(owner(context));
      if (!this.#startupReconciled.has(startupKey)) {
        let changed = false;
        for (const value of Object.values(state.tables)) if (value.enabled && ["synced", "up_to_date"].includes(value.status)) { value.status = "waiting"; value.next_check = 0n; changed = true; }
        if (changed) await checkpoint(); this.#startupReconciled.add(startupKey);
      }
      if (!Object.values(state.tables).some(value => tableSyncQueued(value, now()))) return;
      const admission = await this.#capabilities(context).then(cap => ({ cap, consent: consent(cap) })).then(value => ({ ok: true as const, value }), error => ({ ok: false as const, error }));
      if (!admission.ok) {
        this.#assert(context);
        const deadline = now() + 300n;
        for (const value of Object.values(state.tables)) if (tableSyncQueued(value, now())) { value.next_check = deadline > this.#retryAfter ? deadline : this.#retryAfter; value.error = admission.error instanceof TableSyncError ? admission.error.code : "table_sync_internal_failure"; value.status = "error"; }
        await checkpoint(); throw admission.error;
      }
      const admitted = admission.value;
      const source = identity(admitted.cap, context).source_instance_id;
      state.local_source_id ??= randomUUID(); await checkpoint();
      const failures: unknown[] = [];
      for (const key of Object.keys(state.tables).sort()) {
        const value = state.tables[key]!;
        if (value.user_id !== user || !tableSyncQueued(value, now())) continue;
        value.next_check = now() + 300n;
        const recovering = value.pending !== null;
        const outcome = await this.#runTable(context, value, state.local_source_id, source, admitted.consent, checkpoint).then(() => ({ ok: true as const }), error => ({ ok: false as const, error }));
        this.#assert(context);
        if (outcome.ok) { value.next_check = 0n; if (recovering) value.status = "waiting"; }
        else {
          value.error = outcome.error instanceof TableSyncError ? outcome.error.code : "table_sync_internal_failure";
          value.failures = Math.min(255, value.failures + 1);
          const deadline = now() + 300n * (1n << BigInt(Math.min(value.failures, 3)));
          value.next_check = deadline > this.#retryAfter ? deadline : this.#retryAfter; value.status = "error";
        }
        await checkpoint();
        if (!outcome.ok) failures.push(outcome.error);
      }
      if (failures.length) throw new AggregateError(failures, "table_sync_queued_attempts_failed");
    });
  }
  async #runTable(context: AccountContext, value: TableSyncTable, local: string, source: string, currentConsent: string, checkpoint: () => Promise<void>): Promise<void> {
    if (!value.pending) {
      const version = randomUUID(), started = new Date().toISOString();
      const capture = await this.#capture.capture(context, value.campaign_id, { version_id: version, source_instance_id: source, consent_id: currentConsent, capture_started_at: started });
      this.#assert(context);
      if (value.committed_hash === capture.content_hash && value.committed_consent === currentConsent && value.committed_source === source) { value.failures = 0; value.status = "up_to_date"; value.error = null; return; }
      await validateTableSyncArchive(capture.archive, BigInt(capture.record_count));
      const sequence = BigInt(value.sequence);
      if (sequence >= 18446744073709551615n) throw new TableSyncError("table_sync_sequence_exhausted");
      value.sequence = sequence + 1n;
      value.pending = { input: { version_id: version, consent_id: currentConsent, local_source_id: local, table_id: capture.table_id, table_name: capture.table_name, capture_seq: String(value.sequence), capture_kind: "full_sample", capture_started_at: started, previous_version_id: value.previous_version, scopes: ["dynamic_table_records", "dynamic_table_schema"], format: "onecrew.dynamic_table_capture", format_version: 1 }, archive: capture.archive, archive_sha256: createHash("sha256").update(capture.archive).digest("hex"), content_hash: capture.content_hash, record_count: capture.record_count, finished: new Date().toISOString(), parts: [], attempt: null };
    }
    await checkpoint(); await this.#upload(context, value, currentConsent, checkpoint);
  }
  async #upload(context: AccountContext, value: TableSyncTable, currentConsent: string, checkpoint: () => Promise<void>): Promise<void> {
    const pending = value.pending;
    if (!pending) throw new TableSyncError("table_sync_pending_missing");
    await validateTableSyncPending(pending);
    const version = uuid.parse(pending.input.version_id), status = await this.#send(context, { operation: "status", version_id: version });
    let envelope: Record<string, unknown>;
    if (status.status === 404) { if (pending.input.consent_id !== currentConsent) throw new TableSyncError("table_sync_pending_consent_changed"); envelope = success(await this.#send(context, { operation: "begin", input: pending.input })); }
    else envelope = success(status);
    let body = object.parse(envelope.upload);
    if (body.version_id !== version) throw new TableSyncError("table_sync_version_mismatch");
    if (body.status === "committed") { await this.#commit(value, body, checkpoint); return; }
    if (body.status === "aborted") { value.pending = null; await checkpoint(); throw new TableSyncError("table_sync_aborted"); }
    if (body.status === "initializing") {
      if (pending.input.consent_id !== currentConsent) throw new TableSyncError("table_sync_pending_consent_changed");
      body = object.parse(success(await this.#send(context, { operation: "begin", input: pending.input })).upload);
      if (body.status === "initializing") throw new TableSyncError("table_sync_initializing");
      if (body.status === "aborted") { value.pending = null; await checkpoint(); throw new TableSyncError("table_sync_aborted"); }
      if (body.status === "committed") { await this.#commit(value, body, checkpoint); return; }
    }
    if (body.version_id !== version) throw new TableSyncError("table_sync_version_mismatch");
    if (body.status !== "uploading" && body.status !== "completing") throw new TableSyncError("table_sync_upload_response_invalid");
    const attempt = uuid.parse(body.attempt_id);
    if (pending.input.consent_id !== currentConsent) { const ended = object.parse(success(await this.#send(context, { operation: "abort", version_id: version, attempt_id: attempt })).upload); if (ended.status !== "aborted") throw new TableSyncError("table_sync_abort_unconfirmed"); value.pending = null; await checkpoint(); throw new TableSyncError("table_sync_pending_consent_changed"); }
    if (pending.attempt !== attempt) { pending.parts = []; pending.attempt = attempt; }
    await checkpoint();
    const archive = Buffer.from(pending.archive), total = Math.ceil(archive.length / tableSyncPartBytes);
    if (total > 2048) throw new TableSyncError("table_sync_archive_too_large");
    for (let index = 0; index < total; index++) {
      if (pending.parts.length > index) continue;
      await this.#liveConsent(context, currentConsent);
      const bytes = archive.subarray(index * tableSyncPartBytes, (index + 1) * tableSyncPartBytes), md5 = createHash("md5").update(bytes).digest();
      const part = object.parse(success(await this.#send(context, { operation: "part", version_id: version, attempt_id: attempt, number: index + 1, md5_base64: md5.toString("base64"), bytes })).part);
      if (part.part_number !== index + 1 || BigInt(creatorU64.parse(part.bytes)) !== BigInt(bytes.length) || part.md5 !== md5.toString("hex") || typeof part.etag !== "string" || !part.etag) throw new TableSyncError("table_sync_part_unconfirmed");
      pending.parts.push(part); await checkpoint();
    }
    await this.#liveConsent(context, currentConsent);
    const result = object.parse(success(await this.#send(context, { operation: "complete", version_id: version, input: { attempt_id: attempt, capture_finished_at: pending.finished, record_count: pending.record_count, schema_count: 1, parts: pending.parts } })).upload);
    if (result.status !== "committed") throw new TableSyncError("table_sync_completion_unconfirmed");
    await this.#commit(value, result, checkpoint);
  }
  async #liveConsent(context: AccountContext, expected: string) { const cap = await this.#capabilities(context); if (identity(cap, context).access_user_id !== context.principal.user.id || consent(cap) !== expected) throw new TableSyncError("table_sync_consent_changed"); }
  async #commit(value: TableSyncTable, body: Record<string, unknown>, checkpoint: () => Promise<void>) {
    if (!value.pending) throw new TableSyncError("table_sync_pending_missing");
    await validateTableSyncManifest(value.pending, body);
    const manifest = object.parse(body.manifest);
    value.previous_version = uuid.parse(manifest.version_id); value.committed_hash = value.pending.content_hash; value.committed_consent = uuid.parse(value.pending.input.consent_id); value.committed_source = uuid.parse(manifest.source_instance_id); value.failures = 0; value.pending = null; value.status = "synced"; value.error = null;
    await checkpoint();
  }
  stopAcceptingWork(): void { this.#closed = true; }
}
