import { createHash, randomUUID } from "node:crypto";
import { z } from "zod";
import type { DatabaseSync } from "node:sqlite";
import type { AccountContext, AccountContextService } from "../../identity/account-context.ts";
import { parseStoredJson } from "../../contracts/stored-json.ts";
import { stringifyApplicationJson } from "../../contracts/application-json.ts";
import { decryptVaultEnvelope } from "../../storage/vault-reader.ts";
import { OwnerPlatformVaultStore, assertOwnedService, encryptOwnerVaultSecret, type OwnerPlatformReadScope, type OwnerPlatformWriteScope } from "./owner-vault-store.ts";
import { youtubeHealthOutcome, type YoutubeHealthFailure } from "./youtube-health.ts";
import { validateYoutubeCredentialKey, YoutubeCredentialProbeError, youtubeCredentialConnectInput, youtubeCredentialRenameInput, youtubeCredentialRotateInput } from "./youtube-credentials-validation.ts";
import { beginYoutubeCredentialDelete, completeYoutubeCredentialDelete, markYoutubeCredentialDeleteVerified, pendingYoutubeCredentialDeletes, youtubeCredentialDeleteInput } from "./youtube-credentials-delete.ts";
import type { YoutubeRequest } from "./youtube-request-budget.ts";

const rowSchema = z.object({ id: z.string(), provider: z.literal("youtube"), label: z.string(), metadata_json: z.string(), encrypted_secret: z.instanceof(Uint8Array), disabled: z.number().int(), cooldown_until: z.union([z.number().int().safe(), z.bigint()]).nullable().transform(value => value !== null && BigInt(value) < 0n ? BigInt.asUintN(64, BigInt(value)) : value), last_error: z.string().nullable(), created_at: z.string(), updated_at: z.string() });
const metadataSchema = z.object({ api: z.literal("youtube_data_api_v3") });
function project(raw: string) { const metadata = z.record(z.string(), z.unknown()).parse(parseStoredJson(raw)); return typeof metadata.project === "string" ? metadata.project : null; }
type Row = z.infer<typeof rowSchema>;
function readRow(vault: DatabaseSync, id: string): Row { const statement = vault.prepare("SELECT * FROM service_credentials WHERE id=?"); statement.setReadBigInts(true); const value = statement.get(id); if (!value) throw new Error("YouTube credential was not found"); return rowSchema.parse({ ...value, disabled: value.disabled === 0n ? 0 : 1 }); }
const required = (value: string, message: string) => { const result = value.trim(); if (!result) throw new Error(message); return result; };
function nextRevision(previous?: string) { if (previous !== undefined && (!/^\+?\d+$/u.test(previous) || BigInt(previous) > 18446744073709551615n)) throw new Error("credential revision is invalid"); const now = BigInt(Math.floor(Date.now() / 1000)), next = previous === undefined ? now : BigInt(previous) + 1n; if (next > 18446744073709551615n) throw new Error("credential revision is exhausted"); return (next > now ? next : now).toString(); }
function row(scope: OwnerPlatformReadScope, id: string): Row {
  const entry = assertOwnedService(scope, id), value = readRow(scope.vault, entry.credentialId);
  const reference = scope.creator.prepare("SELECT label FROM account_refs WHERE id=?").get(id);
  if (reference?.label !== value.label) throw new Error("credential_id does not identify a YouTube credential"); return value;
}
function secret(scope: OwnerPlatformReadScope, value: Row) { const plaintext = decryptVaultEnvelope(scope.key, value.encrypted_secret); try { return z.object({ value: z.string().min(1) }).parse(parseStoredJson(plaintext.toString("utf8"))).value; } finally { plaintext.fill(0); } }
function fingerprint(value: Row, key: string) { return createHash("sha256").update(stringifyApplicationJson([value.id, value.label, value.metadata_json, value.disabled, value.cooldown_until, value.last_error, value.created_at, value.updated_at, key])).digest("hex"); }
function available(value: Row) { return metadataSchema.safeParse(parseStoredJson(value.metadata_json)).success && value.disabled === 0 && value.last_error === null && (value.cooldown_until === null || BigInt(value.cooldown_until) <= BigInt(Math.floor(Date.now() / 1000))); }
function reconcile(scope: OwnerPlatformWriteScope, id: string, value: Row) {
  const now = Math.floor(Date.now() / 1000), previous = scope.creator.prepare("SELECT platform,kind FROM account_refs WHERE id=?").get(id);
  if (previous && (previous.platform !== "youtube" || previous.kind !== "service_credential")) throw new Error("credential_id does not identify a YouTube credential");
  scope.creator.prepare("INSERT INTO account_refs(id,platform,kind,label,active,availability,created_at,updated_at) VALUES(?,'youtube','service_credential',?,1,?,?,?) ON CONFLICT(id) DO UPDATE SET label=excluded.label,availability=excluded.availability,updated_at=excluded.updated_at").run(id, value.label, available(value) ? "available" : "unavailable", now, now);
}
function projection(scope: OwnerPlatformReadScope, id: string, value: Row) { return { account_ref_id: id, provider: "youtube", label: value.label, active: scope.creator.prepare("SELECT active FROM account_refs WHERE id=?").get(id)?.active === 1, available: available(value), disabled: value.disabled !== 0, cooldown_until: value.cooldown_until, last_error: value.last_error === null ? null : "YouTube credential has a recorded failure" }; }
function connected(value: Row, health: Awaited<ReturnType<typeof validateYoutubeCredentialKey>>) { return { credential: { credential_id: value.id, provider: "youtube", label: value.label, youtube: { project: project(value.metadata_json) }, mailbox: null, available: value.disabled === 0 }, health: { checked_at: health.checked_at, message: health.message, provider_resource_id: health.channel_id } }; }
function encrypted(scope: OwnerPlatformWriteScope, key: string) { const bytes = Buffer.from(stringifyApplicationJson({ value: key })); try { return encryptOwnerVaultSecret(scope.key, bytes); } finally { bytes.fill(0); } }
function duplicate(scope: OwnerPlatformReadScope, label: string, except?: string) { const existing = scope.vault.prepare("SELECT id FROM service_credentials WHERE provider='youtube' AND label=?").get(label); if (existing && existing.id !== except) throw new Error("a YouTube credential with this label already exists"); }

export function renameYoutubeCredentialInTransaction(vault: DatabaseSync, creator: DatabaseSync, credentialId: string, accountRefId: string, label: string) {
  if (!vault.isTransaction || !creator.isTransaction) throw new Error("youtube_credential_rename_requires_transaction");
  const value = readRow(vault, credentialId);
  const reference = creator.prepare("SELECT platform,kind,label FROM account_refs WHERE id=?").get(accountRefId);
  if (!reference || reference.platform !== "youtube" || reference.kind !== "service_credential" || reference.label !== value.label) throw new Error("credential_id does not identify a YouTube credential");
  const nextLabel = required(label, "请输入 Key 名称"), conflict = vault.prepare("SELECT id FROM service_credentials WHERE provider='youtube' AND label=?").get(nextLabel);
  if (conflict && conflict.id !== credentialId) throw new Error("a YouTube credential with this label already exists");
  vault.prepare("UPDATE service_credentials SET label=?,updated_at=? WHERE id=?").run(nextLabel, nextRevision(value.updated_at), credentialId);
  const updated = { ...value, label: nextLabel };
  creator.prepare("UPDATE account_refs SET label=?,availability=?,updated_at=? WHERE id=?").run(nextLabel, available(updated) ? "available" : "unavailable", Math.floor(Date.now() / 1000), accountRefId);
  return readRow(vault, credentialId);
}
export function recordYoutubeCredentialProbeInTransaction(vault: DatabaseSync, id: string, failure: YoutubeHealthFailure | null, now: number) {
  if (!vault.isTransaction) throw new Error("youtube_credential_health_requires_transaction");
  const value = readRow(vault, id);
  if (failure === null) vault.prepare("UPDATE service_credentials SET cooldown_until=CASE WHEN cooldown_until IS NOT NULL AND cooldown_until>? THEN cooldown_until ELSE NULL END,last_error=CASE WHEN disabled OR (cooldown_until IS NOT NULL AND cooldown_until>?) THEN last_error ELSE NULL END,last_used_at=?,updated_at=? WHERE id=?").run(now, now, now, nextRevision(value.updated_at), id);
  else { const health = youtubeHealthOutcome(failure, now); vault.prepare("UPDATE service_credentials SET disabled=?,cooldown_until=?,last_error=?,updated_at=? WHERE id=?").run(health.disabled ? 1 : 0, health.cooldownUntil, health.message, nextRevision(value.updated_at), id); }
  return readRow(vault, id);
}

export class YoutubeCredentials {
  readonly #accounts: AccountContextService; readonly #store: OwnerPlatformVaultStore; readonly #changed: (context: AccountContext) => void; readonly #request: (id: string) => YoutubeRequest;
  #observation = 0n; readonly #committed = new Map<string, { observation: bigint; material: string; revision: string }>();
  constructor(accounts: AccountContextService, store: OwnerPlatformVaultStore, changed: ((context: AccountContext) => void) | undefined, requestForCredential: (id: string) => YoutubeRequest) { this.#accounts = accounts; this.#store = store; this.#changed = changed ?? (() => {}); this.#request = requestForCredential; }
  async connect(context: AccountContext, raw: unknown) {
    const input = youtubeCredentialConnectInput.parse(raw), label = required(input.label, "请输入 Key 名称"), key = required(input.api_key, "请输入 YouTube API Key"), project = input.project?.trim() || null;
    await this.#store.read(context, scope => duplicate(scope, label));
    const health = await validateYoutubeCredentialKey(key, context.signal, async () => this.#accounts.assertCurrent(context));
    const output = await this.#store.mutate(context, scope => {
      duplicate(scope, label); const id = randomUUID(), now = nextRevision();
      scope.vault.prepare("INSERT INTO service_credentials(id,provider,label,metadata_json,encrypted_secret,disabled,cooldown_until,last_used_at,last_error,created_at,updated_at) VALUES(?,'youtube',?,?,?,0,NULL,NULL,NULL,?,?)").run(id, label, stringifyApplicationJson({ api: "youtube_data_api_v3", project }), encrypted(scope, key), now, now);
      const value = readRow(scope.vault, id); reconcile(scope, id, value);
      scope.manifest.services.push({ accountRefId: id, credentialId: id, provider: "youtube", vaultRevision: "0".repeat(64) }); return connected(value, health);
    }); this.#changed(context); return output;
  }
  async rotate(context: AccountContext, raw: unknown) {
    const input = youtubeCredentialRotateInput.parse(raw); if (!input.confirmed) throw new Error("credential rotation requires explicit confirmation");
    const id = z.uuid().parse(input.credential_id.trim()), key = required(input.api_key, "请输入 YouTube API Key");
    const initial = await this.#store.read(context, scope => { const value = row(scope, id); return { fingerprint: fingerprint(value, secret(scope, value)), project: project(value.metadata_json) }; });
    const health = await validateYoutubeCredentialKey(key, context.signal, async () => this.#accounts.assertCurrent(context));
    const output = await this.#store.mutate(context, scope => {
      const existing = row(scope, id); if (fingerprint(existing, secret(scope, existing)) !== initial.fingerprint) throw new Error("youtube_credential_changed");
      scope.vault.prepare("UPDATE service_credentials SET metadata_json=?,encrypted_secret=?,disabled=0,cooldown_until=NULL,last_error=NULL,updated_at=? WHERE id=?").run(stringifyApplicationJson({ api: "youtube_data_api_v3", project: initial.project }), encrypted(scope, key), nextRevision(existing.updated_at), existing.id);
      const value = row(scope, id); reconcile(scope, id, value); return connected(value, health);
    }); this.#changed(context); return output;
  }
  async rename(context: AccountContext, raw: unknown) {
    const input = youtubeCredentialRenameInput.parse(raw); if (!input.confirmed) throw new Error("credential rename requires explicit confirmation");
    const id = z.uuid().parse(input.credential_id.trim()), label = required(input.label, "请输入 Key 名称");
    const output = await this.#store.mutate(context, scope => { const existing = row(scope, id); const value = renameYoutubeCredentialInTransaction(scope.vault, scope.creator, existing.id, id, label); return projection(scope, id, value); });
    this.#changed(context); return output;
  }
  async test(context: AccountContext, rawId: string) {
    const id = z.uuid().parse(rawId), observation = ++this.#observation, observationKey = JSON.stringify([context.principal.user.id, context.principal.tenantId, context.principal.baseId, id]);
    const opened = await this.#store.read(context, scope => { const value = row(scope, id), key = secret(scope, value); const revision = fingerprint(value, key), previous = this.#committed.get(observationKey); return { key, credentialId: value.id, fingerprint: revision, material: previous?.revision === revision ? previous.material : revision }; });
    const probe = await validateYoutubeCredentialKey(opened.key, context.signal, async () => this.#accounts.assertCurrent(context), this.#request(opened.credentialId)).then(health => ({ ok: true as const, health }), error => { if (!(error instanceof YoutubeCredentialProbeError)) throw error; return { ok: false as const, error }; });
    const revision = await this.#store.mutate(context, scope => {
      const value = row(scope, id), current = fingerprint(value, secret(scope, value)), previous = this.#committed.get(observationKey);
      if ((previous?.observation ?? 0n) >= observation || current !== opened.fingerprint && !(previous?.material === opened.material && previous.revision === current)) throw new Error("youtube_credential_health_superseded");
      const now = Math.floor(Date.now() / 1000);
      recordYoutubeCredentialProbeInTransaction(scope.vault, value.id, probe.ok ? null : probe.error.health, now);
      const updated = row(scope, id); reconcile(scope, id, updated); return fingerprint(updated, secret(scope, updated));
    }); this.#committed.set(observationKey, { observation, material: opened.material, revision }); this.#changed(context);
    if (probe.ok) return { credential_id: id, ok: true, checked_at: probe.health.checked_at, channel_id: probe.health.channel_id, issue: null, message: probe.health.message };
    if (!["api_key_invalid", "access_not_configured"].includes(probe.error.issue)) throw probe.error;
    return { credential_id: id, ok: false, checked_at: Math.floor(Date.now() / 1000), channel_id: null, issue: probe.error.issue, message: probe.error.message };
  }
  async delete(context: AccountContext, raw: unknown) {
    const input = youtubeCredentialDeleteInput.parse(raw); if (!input.confirmed) throw new Error("confirmed=true is required"); const id = z.uuid().parse(input.credential_id), key = input.idempotency_key;
    const claim = await this.#store.mutate(context, scope => {
      const reference = scope.creator.prepare("SELECT platform,kind,label FROM account_refs WHERE id=?").get(id);
      if (!reference || reference.platform !== "youtube" || reference.kind !== "service_credential") throw new Error("credential_id does not identify a YouTube credential");
      const entry = scope.manifest.services.find(value => value.accountRefId === id); if (entry) row(scope, id);
      else if (scope.creator.prepare("SELECT key FROM idempotency_records WHERE action='service_credential_delete' AND request_fingerprint=?").get(id) === undefined) throw new Error("platform_store.owner_mapping_required");
      return beginYoutubeCredentialDelete(scope.creator, key, id);
    });
    if (claim.state === "completed") return { credential_id: id, replayed: true };
    const completed = await Promise.resolve().then(async () => {
      if (claim.state === "acquired") await this.#store.mutate(context, scope => { const entry = scope.manifest.services.find(value => value.accountRefId === id); if (entry) { assertOwnedService(scope, id); scope.vault.prepare("DELETE FROM service_credentials WHERE id=?").run(entry.credentialId); scope.manifest.services = scope.manifest.services.filter(value => value.accountRefId !== id); } markYoutubeCredentialDeleteVerified(scope.creator, key, id); });
      return this.#store.mutate(context, scope => completeYoutubeCredentialDelete(scope.creator, key, id));
    }).then(output => ({ ok: true as const, output }), error => ({ ok: false as const, error }));
    this.#changed(context);
    if (!completed.ok) throw new Error("YouTube deletion is incomplete; retry the same intent with the same idempotency_key", { cause: completed.error });
    return { credential_id: id, replayed: completed.output.replayed };
  }
  async recover(context: AccountContext): Promise<void> { const pending = await this.#store.read(context, scope => pendingYoutubeCredentialDeletes(scope.creator)); for (const item of pending) await this.delete(context, { credential_id: item.account_ref_id, idempotency_key: item.key, confirmed: true }); }
}
