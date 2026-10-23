import { createHash, randomUUID } from "node:crypto";
import { TelegramClient, MemoryStorage, proxyTransportFromUrl, SentCode, tl, User } from "@mtcute/node";
import QRCode from "qrcode";
import { z } from "zod";
import type { AccountContext, AccountContextService } from "../../identity/account-context.ts";
import { assertOwnedProfile, type OwnerPlatformVaultStore } from "./owner-vault-store.ts";
import { decryptVaultEnvelope } from "../../storage/vault-reader.ts";
import { parseApplicationJson } from "../../contracts/application-json.ts";
import { importGrammersSession } from "../candidates/telegram-read.ts";
import { nativeProxy } from "../candidates/social-read.ts";
import { persistTelegramConnection } from "./telegram-connection-persistence.ts";
import { assertTelegramSecretStep, telegramConnectionFingerprint, telegramConnectionProxy, validateTelegramConnectionStart, type TelegramConnectionStart, type TelegramCodeDelivery, type TelegramConnectionStatus } from "./telegram-connection-contracts.ts";

type Phase = "awaiting_secret_in_app" | "ready_to_begin" | "starting" | "awaiting_user_in_app" | "completed" | "canceled" | "failed";
interface Flow {
  id: string; context: AccountContext; method: "telegram_qr" | "telegram_phone" | "existing_account"; intent: "connect" | "reconnect"; phase: Phase; expires: number; input?: TelegramConnectionStart; requestKey?: string; fingerprint?: string; beginKey?: string; finalizeKey?: string; expected?: { accountRefId: string; providerAccountId: string; revision: string; proxyEgress: string | null }; importedSession?: string; label?: string;
  client?: TelegramClient; cancel: AbortController; task?: Promise<void>; code?: SentCode; password?: { resolve(value: string): void; reject(error: unknown): void }; qr?: { image_data_url: string; terminal_qr: string; expires_at: number }; qrRevision?: number; delivery?: TelegramCodeDelivery; account?: ReturnType<typeof persistTelegramConnection>; error?: string;
}
const operation = z.strictObject({ flow_id: z.string() });
const terminal = (flow: Flow) => ["completed", "canceled", "failed"].includes(flow.phase);
const codeMethods: Record<string, string> = { app: "telegram_app", sms: "sms", call: "voice_call", flash_call: "flash_call", missed_call: "missed_call", email: "email", email_required: "email_setup_required", fragment: "fragment", firebase: "firebase_sms", sms_word: "sms_word", sms_phrase: "sms_phrase" };
function delivery(code: SentCode): TelegramCodeDelivery {
  const type = code.raw.type;
  const hint = code.type === "app" ? "Telegram service chat 777000 on another signed-in device" : "emailPattern" in type ? type.emailPattern : "pattern" in type ? type.pattern : "prefix" in type ? type.prefix : "beginning" in type ? type.beginning ?? null : null, url = "url" in type ? type.url : null;
  return { method: codeMethods[code.type] ?? (() => { throw new Error("telegram.code_delivery_invalid"); })(), code_length: "length" in type ? z.number().int().parse(type.length) : null, destination_hint: hint === null ? null : z.string().parse(hint), timeout_seconds: code.raw.timeout ?? null, next_method: codeMethods[code.nextType] ?? null, action_url: url === null ? null : z.string().parse(url), can_submit_code: code.type !== "email_required" };
}

// 只持有当前账号的临时MTProto连接；OTP/密码/原始二维码均不进入公开状态或日志。
export class TelegramConnectionRuntime {
  readonly #accounts: AccountContextService; readonly #store: OwnerPlatformVaultStore; readonly #changed: (context: AccountContext) => void;
  readonly #flows = new Map<string, Flow>(); readonly #pending = new Set<Promise<unknown>>(); readonly #errors: unknown[] = []; #closing = false;
  readonly #shutdown = new AbortController();
  readonly #requests = new WeakMap<AccountContext, Map<string, { fingerprint: string; task: Promise<TelegramConnectionStatus>; flowId?: string }>>();
  readonly #capabilities: { openTelegramCredentials?(context: AccountContext, proxy: unknown): Promise<unknown> };
  constructor(accounts: AccountContextService, store: OwnerPlatformVaultStore, changed: (context: AccountContext) => void = () => {}, capabilities: { openTelegramCredentials?(context: AccountContext, proxy: unknown): Promise<unknown> } = {}) { this.#accounts = accounts; this.#store = store; this.#changed = changed; this.#capabilities = capabilities; }
  owns(id: string): boolean { return this.#flows.has(id); }
  async #tracked<T>(work: () => Promise<T>): Promise<T> { const task = work(); this.#pending.add(task); try { return await task; } finally { this.#pending.delete(task); } }
  #current(context: AccountContext): void { this.#accounts.assertCurrent(context); if (this.#closing) throw new Error("telegram.connection_stopping"); }
  #flow(context: AccountContext, id: string): Flow {
    this.#current(context); const flow = this.#flows.get(id);
    if (!flow || flow.context !== context) throw new Error("operator account connection flow not found");
    if (performance.now() >= flow.expires) { flow.cancel.abort(new Error("telegram.connection_expired")); flow.password?.reject(flow.cancel.signal.reason); const cleanup = this.#tracked(async () => { if (flow.client) await flow.client.destroy(); if (flow.task) await flow.task; }); void cleanup.then(() => undefined, error => { this.#errors.push(error); }); this.#flows.delete(id); delete flow.input; delete flow.importedSession; delete flow.qr; throw new Error("operator account connection flow expired"); }
    return flow;
  }
  async prepare(context: AccountContext, raw: unknown) {
    const input = z.strictObject({ platform: z.literal("telegram"), intent: z.enum(["connect", "reconnect"]), method: z.enum(["telegram_qr", "telegram_phone", "existing_account"]), operator_account_id: z.string().nullish(), label: z.string().nullish() }).parse(raw);
    if (input.intent === "reconnect" !== (input.method === "existing_account") || input.intent === "reconnect" !== !!input.operator_account_id) throw new Error("Telegram connection method does not match its intent");
    this.#current(context); await this.#store.read(context, scope => scope.manifest.owner); this.#current(context);
    for (const [id, flow] of this.#flows) if (performance.now() >= flow.expires) { flow.cancel.abort(new Error("telegram.connection_expired")); if (flow.client) await flow.client.destroy(); this.#flows.delete(id); }
    if (this.#flows.size >= 128) throw new Error("too many pending operator account connections");
    const flow: Flow = { id: randomUUID(), context, method: input.method, intent: input.intent, phase: "awaiting_secret_in_app", expires: performance.now() + 1800000, cancel: new AbortController() };
    if (input.intent === "reconnect") {
      const saved = await this.#store.read(context, scope => {
        const mapping = assertOwnedProfile(scope, z.uuid().parse(input.operator_account_id)); if (mapping.platform !== "telegram") throw new Error("Telegram reconnect target is not a Telegram account");
        const row = scope.vault.prepare("SELECT username,encrypted_secret FROM profiles WHERE id=?").get(mapping.profileId); if (!row || !(row.encrypted_secret instanceof Uint8Array)) throw new Error("telegram.session_unavailable");
        const clear = decryptVaultEnvelope(scope.key, row.encrypted_secret);
        try { const parsed = z.object({ provider_account_id: z.string().trim().min(1), telegram_api_id: z.number().int().positive().max(2147483647), telegram_api_hash: z.string().refine(value => !!value.trim()), telegram_session: z.string().refine(value => !!value.trim()), profile_name: z.string(), proxy: telegramConnectionProxy.refine(value => value.kind === "socks5"), proxy_egress: z.string().nullish() }).safeParse(parseApplicationJson(clear)); if (!parsed.success || parsed.data.provider_account_id !== mapping.providerAccountId) return null; const secret = parsed.data; return { secret, expected: { accountRefId: mapping.accountRefId, providerAccountId: mapping.providerAccountId, revision: createHash("sha256").update(row.encrypted_secret).digest("hex"), proxyEgress: secret.proxy_egress ?? null } }; } finally { clear.fill(0); }
      });
      if (saved === null) { flow.phase = "failed"; flow.error = "telegram_reconnect_unavailable_start_new_connection"; flow.expires = performance.now() + 300000; }
      else { flow.input = validateTelegramConnectionStart({ api_id: saved.secret.telegram_api_id, api_hash: saved.secret.telegram_api_hash, proxy: saved.secret.proxy, idempotency_key: flow.id }); flow.expected = saved.expected; flow.importedSession = saved.secret.telegram_session; flow.label = saved.secret.profile_name; flow.phase = "ready_to_begin"; }
    }
    this.#current(context); this.#flows.set(flow.id, flow);
    context.signal.addEventListener("abort", () => {
      if (terminal(flow)) return;
      flow.cancel.abort(context.signal.reason); flow.password?.reject(context.signal.reason);
      const cleanup = this.#tracked(async () => { if (flow.client) await flow.client.destroy(); flow.phase = "canceled"; flow.expires = performance.now() + 300000; delete flow.input; delete flow.importedSession; delete flow.qr; });
      void cleanup.then(() => undefined, error => { this.#errors.push(error); });
    }, { once: true });
    return this.#snapshot(flow);
  }
  async configure(context: AccountContext, raw: unknown) {
    const input = z.strictObject({ flowId: z.string(), apiId: z.number(), apiHash: z.string(), phone: z.string().nullish(), proxy: z.unknown() }).parse(raw), flow = this.#flow(context, input.flowId);
    if (flow.intent !== "connect" || !["awaiting_secret_in_app", "ready_to_begin"].includes(flow.phase) || flow.client) throw new Error("operator account connection cannot be configured in its current phase");
    flow.input = validateTelegramConnectionStart({ api_id: input.apiId, api_hash: input.apiHash, proxy: input.proxy, phone: input.phone, method: flow.method === "telegram_qr" ? "qr" : "phone", idempotency_key: flow.id }); flow.phase = "ready_to_begin"; return this.#snapshot(flow);
  }
  begin(context: AccountContext, raw: unknown) { return this.#tracked(async () => {
    const input = z.strictObject({ flow_id: z.string(), confirmed: z.boolean(), idempotency_key: z.string() }).parse(raw);
    const result = await this.#begin(context, input).then(value => ({ ok: true as const, value }), error => ({ ok: false as const, error: error as unknown }));
    if (result.ok) return result.value;
    const flow = this.#flows.get(input.flow_id);
    if (flow?.context === context && flow.phase === "starting") {
      if (flow.cancel.signal.aborted || context.signal.aborted || this.#closing) { flow.phase = "canceled"; flow.expires = performance.now() + 300000; if (flow.client) await flow.client.destroy(); delete flow.input; delete flow.importedSession; }
      else await this.#fail(flow, "connection_begin_failed");
    }
    throw result.error;
  }); }
  async #begin(context: AccountContext, raw: unknown) {
    const input = z.strictObject({ flow_id: z.string(), confirmed: z.boolean(), idempotency_key: z.string() }).parse(raw), flow = this.#flow(context, input.flow_id);
    if (!input.confirmed) throw new Error("operator account connection requires explicit confirmation");
    if (!input.idempotency_key.trim() || Buffer.byteLength(input.idempotency_key.trim()) > 128) throw new Error("idempotency_key must contain 1 to 128 characters");
    if (flow.beginKey !== undefined) { if (flow.beginKey !== input.idempotency_key) throw new Error("idempotency_key does not match this connection operation"); return this.#snapshot(flow); }
    if (flow.phase !== "ready_to_begin" || !flow.input) throw new Error("operator account connection is not ready to begin");
    const configured = flow.input, proxy = nativeProxy(configured.proxy); if (!proxy?.startsWith("socks5://")) throw new Error("Telegram requires a SOCKS5 proxy");
    flow.beginKey = input.idempotency_key; flow.phase = "starting";
    const storage = new MemoryStorage(); flow.client = new TelegramClient({ apiId: configured.api_id, apiHash: configured.api_hash, storage, transport: proxyTransportFromUrl(proxy), disableUpdates: flow.method !== "telegram_qr", updates: false, logLevel: 0, fallbackDcs: [] });
    const signal = AbortSignal.any([context.signal, flow.cancel.signal]), scoped = flow.client.withParams({ abortSignal: signal, timeout: 30000, maxRetryCount: 0, floodSleepThreshold: 0 });
    if (flow.method === "existing_account") {
      const converted = importGrammersSession(flow.importedSession!);
      try { await flow.client.importSession(converted.session); for (const key of converted.keys) storage.authKeys.set(key.id, key.key); const me = await scoped.getMe(); if (String(me.id) !== flow.expected?.providerAccountId) throw new Error("telegram.operator_identity_mismatch"); await this.#complete(flow, me); }
      finally { converted.session.authKey.fill(0); for (const key of converted.keys) key.key.fill(0); }
      return this.#snapshot(flow);
    }
    if (flow.method === "telegram_phone") {
      const result = await scoped.sendCode({ phone: configured.phone!, abortSignal: signal }).then(value => ({ ok: true as const, value }), error => ({ ok: false as const, error: error as unknown }));
      this.#current(context); signal.throwIfAborted();
      if (!result.ok) { await this.#fail(flow, "telegram_login_failed"); throw new Error("Telegram login start failed", { cause: result.error }); }
      if (result.value instanceof SentCode) { flow.code = result.value; flow.delivery = delivery(result.value); if (!flow.delivery.can_submit_code) { await this.#fail(flow, "telegram_email_setup_required"); throw new Error("Telegram requires email setup in an official client before this login can continue"); } flow.phase = "awaiting_secret_in_app"; }
      else { await this.#fail(flow, "telegram_authorized_instead_of_code"); throw new Error("Telegram returned an already-authorized login instead of a code"); }
      return this.#snapshot(flow);
    }
    let ready!: () => void; const updated = new Promise<void>(resolve => { ready = resolve; });
    flow.task = scoped.signInQr({ abortSignal: signal, onUrlUpdated: (url, expires) => {
      this.#current(context); signal.throwIfAborted();
      const revision = (flow.qrRevision ?? 0) + 1; flow.qrRevision = revision;
      const rendered = this.#tracked(async () => { const image_data_url = await QRCode.toDataURL(url, { errorCorrectionLevel: "M" }), terminal_qr = await QRCode.toString(url, { type: "terminal", small: true }); this.#current(context); signal.throwIfAborted(); if (flow.qrRevision !== revision) return; flow.qr = { image_data_url, terminal_qr, expires_at: Math.floor(expires.getTime() / 1000) }; flow.phase = "awaiting_user_in_app"; ready(); });
      // 渲染属于同一连接工作，失败直接终止MTProto流程，绝不输出原始token。
      void rendered.then(() => undefined, error => { if (!signal.aborted) { this.#errors.push(error); flow.error = "telegram_qr_render_failed"; flow.cancel.abort(error); } ready(); });
    }, password: () => { delete flow.qr; flow.phase = "awaiting_secret_in_app"; ready(); return new Promise<string>((resolve, reject) => { flow.password = { resolve, reject }; signal.addEventListener("abort", () => reject(signal.reason), { once: true }); }); }, invalidPasswordCallback: () => { flow.phase = "awaiting_secret_in_app"; flow.error = "telegram_two_factor_invalid"; } }).then(user => this.#complete(flow, user)).then(() => undefined, async error => { if (!flow.cancel.signal.aborted && !context.signal.aborted) { await this.#fail(flow, "telegram_login_failed"); if (!(error instanceof tl.RpcError)) this.#errors.push(error); } else if (!terminal(flow)) { flow.phase = flow.error === "telegram_qr_render_failed" ? "failed" : "canceled"; flow.expires = performance.now() + 300000; } if (flow.client) await flow.client.destroy(); ready(); });
    void flow.task.then(() => undefined, error => { this.#errors.push(error); });
    await Promise.race([updated, flow.task]); this.#current(context); signal.throwIfAborted(); return this.#snapshot(flow);
  }
  async #complete(flow: Flow, user: User): Promise<void> {
    const client = flow.client; if (!client || !flow.input) throw new Error("telegram.connection_state_invalid");
    this.#current(flow.context); flow.cancel.signal.throwIfAborted(); const me = await client.withParams({ timeout: 30000, maxRetryCount: 0, floodSleepThreshold: 0, abortSignal: AbortSignal.any([flow.context.signal, flow.cancel.signal]) }).getMe();
    this.#current(flow.context); flow.cancel.signal.throwIfAborted(); if (String(me.id) !== String(user.id)) throw new Error("telegram.operator_identity_mismatch");
    const session = await client.exportSession(); await client.destroy();
    const input = flow.input, label = flow.label ?? (flow.method === "telegram_qr" ? "Telegram · QR 扫码" : `Telegram · ${input.phone}`);
    const account = await this.#store.mutate(flow.context, scope => {
      if (flow.expected) { const mapping = assertOwnedProfile(scope, flow.expected.accountRefId), row = scope.vault.prepare("SELECT encrypted_secret FROM profiles WHERE id=?").get(mapping.profileId); if (!row || !(row.encrypted_secret instanceof Uint8Array) || createHash("sha256").update(row.encrypted_secret).digest("hex") !== flow.expected.revision) throw new Error("telegram.session_changed"); }
      return persistTelegramConnection(scope, input, { id: String(me.id), username: me.username, first_name: me.firstName, last_name: me.lastName, is_bot: me.isBot, verified: me.isVerified }, session, label, flow.expected);
    });
    flow.account = account; delete flow.input; delete flow.importedSession; delete flow.qr; delete flow.code; delete flow.delivery; delete flow.password; delete flow.client; flow.phase = "completed"; flow.expires = performance.now() + 300000; this.#changed(flow.context);
  }
  async #fail(flow: Flow, code: string): Promise<void> { flow.phase = "failed"; flow.error = code; flow.expires = performance.now() + 300000; delete flow.qr; delete flow.input; delete flow.importedSession; if (flow.client) await flow.client.destroy(); delete flow.client; }
  submit(context: AccountContext, raw: unknown, password = false) { return this.#tracked(() => this.#submit(context, raw, password)); }
  async #submit(context: AccountContext, raw: unknown, password: boolean) {
    const input = z.strictObject({ flowId: z.string(), value: z.string() }).parse(raw), flow = this.#flow(context, input.flowId);
    assertTelegramSecretStep({ phase: flow.phase, method: flow.method, hasClient: flow.client !== undefined, hasInput: flow.input !== undefined, hasCode: flow.code !== undefined, hasPasswordWaiter: flow.password !== undefined }, password);
    if (flow.phase !== "awaiting_secret_in_app" || !flow.client || !flow.input) throw new Error("operator account connection operation is in progress");
    if (password && !input.value || !password && !input.value.trim()) throw new Error(password ? "Telegram two-factor password is required" : "Telegram OTP is required");
    if (password && flow.method === "telegram_qr") {
      if (!flow.password) throw new Error("connection is not awaiting Telegram 2FA in the app");
      const pending = flow.password; delete flow.password; flow.phase = "starting"; pending.resolve(input.value); return this.#snapshot(flow);
    }
    if (!password && !flow.code) throw new Error("connection is not awaiting a Telegram OTP in the app");
    flow.phase = "starting"; const signal = AbortSignal.any([context.signal, flow.cancel.signal]);
    const client = flow.client.withParams({ timeout: 30000, maxRetryCount: 0, floodSleepThreshold: 0, abortSignal: signal });
    const authorization = async () => {
      if (password) return client.checkPassword({ password: input.value, abortSignal: signal });
      if (flow.code!.type !== "email") return client.signIn({ phone: flow.input!.phone!, phoneCodeHash: flow.code!.phoneCodeHash, phoneCode: input.value.trim(), abortSignal: signal });
      const result = await client.call({ _: "auth.signIn", phoneNumber: flow.input!.phone!, phoneCodeHash: flow.code!.phoneCodeHash, emailVerification: { _: "emailVerificationCode", code: input.value.trim() } });
      if (result._ !== "auth.authorization") throw new Error("sign up with an official Telegram client is required");
      return new User(await flow.client!._client.notifyLoggedIn(result));
    };
    const outcome = await authorization().then(value => ({ ok: true as const, value }), error => ({ ok: false as const, error: error as unknown }));
    this.#current(context); signal.throwIfAborted();
    if (outcome.ok) await this.#complete(flow, outcome.value);
    else if (tl.RpcError.is(outcome.error, "SESSION_PASSWORD_NEEDED")) { delete flow.code; flow.phase = "awaiting_secret_in_app"; }
    else { flow.phase = "awaiting_secret_in_app"; if (outcome.error instanceof tl.RpcError && /^(PHONE_CODE_|EMAIL_VERIFY_)/u.test(outcome.error.text)) throw new Error("Telegram OTP is invalid or expired"); if (tl.RpcError.is(outcome.error, "PASSWORD_HASH_INVALID")) throw new Error("Telegram two-factor password is invalid"); throw new Error(password ? "Telegram two-factor submission failed" : "Telegram OTP submission failed", { cause: outcome.error }); }
    return this.#snapshot(flow);
  }
  async status(context: AccountContext, raw: unknown) { return this.#snapshot(this.#flow(context, operation.parse(raw).flow_id)); }
  async handoff(context: AccountContext, raw: unknown) { const flow = this.#flow(context, operation.parse(raw).flow_id); return { flow_id: flow.id, ...(flow.qr ? { telegram_qr_challenge: { image_data_url: flow.qr.image_data_url, expires_at: flow.qr.expires_at } } : {}), ...(flow.delivery ? { telegram_code_delivery: flow.delivery } : {}) }; }
  async openCredentials(context: AccountContext, raw: unknown): Promise<void> { const input = z.strictObject({ flowId: z.string(), proxy: z.unknown() }).parse(raw), flow = this.#flow(context, input.flowId); if (flow.intent !== "connect" || !["awaiting_secret_in_app", "ready_to_begin"].includes(flow.phase) || flow.client) throw new Error("Telegram credential browser is unavailable for this flow"); if (!this.#capabilities.openTelegramCredentials) throw new Error("telegram.credential_browser_unavailable"); await this.#capabilities.openTelegramCredentials(context, input.proxy); this.#current(context); }
  async finalize(context: AccountContext, raw: unknown) { const input = z.strictObject({ flow_id: z.string(), confirmed: z.boolean(), idempotency_key: z.string() }).parse(raw); if (!input.confirmed) throw new Error("operator account connection requires explicit confirmation"); if (!input.idempotency_key.trim() || Buffer.byteLength(input.idempotency_key.trim()) > 128) throw new Error("idempotency_key must contain 1 to 128 characters"); const flow = this.#flow(context, input.flow_id); if (flow.finalizeKey !== undefined && flow.finalizeKey !== input.idempotency_key) throw new Error("connection finalize already used a different idempotency_key"); if (flow.phase === "completed") flow.finalizeKey = input.idempotency_key; return this.#snapshot(flow); }
  async refreshProfile(context: AccountContext, profileId: string) { const prepared = await this.prepare(context, { platform: "telegram", intent: "reconnect", method: "existing_account", operator_account_id: profileId }); const result = await this.begin(context, { flow_id: prepared.flow_id, confirmed: true, idempotency_key: `refresh:${prepared.flow_id}` }); if (result.phase !== "completed" || !result.operator_account) throw new Error("telegram.refresh_incomplete"); const account = result.operator_account; return { id: account.operator_account_id, platform: "telegram", provider_account_id: account.provider_account_id, username: account.username, created_at: await this.#store.read(context, scope => { const mapping = assertOwnedProfile(scope, account.operator_account_id); return z.string().parse(scope.vault.prepare("SELECT created_at FROM profiles WHERE id=?").get(mapping.profileId)?.created_at); }), updated_at: account.updated_at }; }
  checkProfile(context: AccountContext, profileId: string) { return this.#tracked(async () => {
    this.#current(context);
    const original = await this.#store.read(context, scope => {
      const mapping = assertOwnedProfile(scope, z.uuid().parse(profileId)); if (mapping.platform !== "telegram") throw new Error("telegram.profile_kind_invalid");
      const row = scope.vault.prepare("SELECT encrypted_secret FROM profiles WHERE id=?").get(mapping.profileId); if (!row || !(row.encrypted_secret instanceof Uint8Array)) throw new Error("telegram.session_unavailable");
      const clear = decryptVaultEnvelope(scope.key, row.encrypted_secret);
      try { const secret = z.object({ provider_account_id: z.string(), telegram_api_id: z.number().int().positive(), telegram_api_hash: z.string().min(1), telegram_session: z.string().min(1), proxy: telegramConnectionProxy }).parse(parseApplicationJson(clear)); if (secret.provider_account_id !== mapping.providerAccountId) throw new Error("telegram.operator_identity_mismatch"); return { secret, revision: createHash("sha256").update(row.encrypted_secret).digest("hex") }; } finally { clear.fill(0); }
    });
    this.#current(context); const proxy = nativeProxy(original.secret.proxy); if (!proxy?.startsWith("socks5://")) throw new Error("Telegram requires a SOCKS5 proxy");
    const converted = importGrammersSession(original.secret.telegram_session), storage = new MemoryStorage(), client = new TelegramClient({ apiId: original.secret.telegram_api_id, apiHash: original.secret.telegram_api_hash, storage, transport: proxyTransportFromUrl(proxy), disableUpdates: true, logLevel: 0, fallbackDcs: [] });
    try {
      await client.importSession(converted.session); for (const key of converted.keys) storage.authKeys.set(key.id, key.key);
      this.#current(context); const me = await client.withParams({ abortSignal: AbortSignal.any([context.signal, this.#shutdown.signal]), timeout: 30000, maxRetryCount: 0, floodSleepThreshold: 0 }).getMe(); this.#current(context);
      if (String(me.id) !== original.secret.provider_account_id) throw new Error("telegram.operator_identity_mismatch");
      await this.#store.read(context, scope => { const mapping = assertOwnedProfile(scope, profileId), row = scope.vault.prepare("SELECT encrypted_secret FROM profiles WHERE id=?").get(mapping.profileId); if (!row || !(row.encrypted_secret instanceof Uint8Array) || createHash("sha256").update(row.encrypted_secret).digest("hex") !== original.revision) throw new Error("telegram.session_changed"); });
      return { id: String(me.id), username: me.username };
    } finally { try { await client.destroy(); } finally { converted.session.authKey.fill(0); for (const key of converted.keys) key.key.fill(0); } }
  }); }
  async cancel(context: AccountContext, raw: unknown) {
    const input = z.strictObject({ flow_id: z.string(), confirmed: z.boolean() }).parse(raw), flow = this.#flow(context, input.flow_id); if (!input.confirmed) throw new Error("operator account connection cancellation requires confirmation");
    if (flow.phase === "completed") throw new Error("completed operator account connections cannot be canceled");
    if (flow.phase !== "canceled") { flow.phase = "canceled"; flow.cancel.abort(new Error("telegram.connection_canceled")); flow.password?.reject(flow.cancel.signal.reason); if (flow.client) await flow.client.destroy(); if (flow.task) await flow.task; flow.expires = performance.now() + 300000; delete flow.error; delete flow.input; delete flow.importedSession; delete flow.client; delete flow.qr; delete flow.code; delete flow.password; delete flow.delivery; } return this.#snapshot(flow);
  }
  #snapshot(flow: Flow) {
    const kind = flow.phase === "awaiting_user_in_app" ? "scan_telegram_qr_in_desktop" : flow.phase === "awaiting_secret_in_app" ? flow.client ? flow.code ? "enter_telegram_otp_in_desktop" : "enter_telegram_two_factor_in_desktop" : "configure_connection_in_desktop" : null;
    return { flow_id: flow.id, platform: "telegram" as const, intent: flow.intent, method: flow.method, kind: flow.method === "existing_account" ? "telegram_reconnect" as const : flow.method === "telegram_qr" ? "telegram_qr_connect" as const : "telegram_phone_connect" as const, phase: flow.phase, expires_at: Date.now() + Math.max(0, Math.floor(flow.expires - performance.now())), ...(flow.account ? { operator_account: flow.account } : {}), ...(kind ? { handoff: { kind, message: "Complete this Telegram connection in the desktop Accounts flow." } } : {}), allowed_actions: flow.phase === "ready_to_begin" ? ["begin", "status", "cancel"] : terminal(flow) ? flow.phase === "failed" ? ["status", "cancel"] : ["status"] : flow.phase === "starting" ? ["status", "cancel"] : flow.phase === "awaiting_user_in_app" ? ["open_desktop", "status", "finalize", "cancel"] : ["open_desktop", "status", "cancel"], ...(flow.error ? { error_code: flow.error } : {}) };
  }
  start(context: AccountContext, raw: unknown): Promise<TelegramConnectionStatus> {
    const input = validateTelegramConnectionStart(raw), fingerprint = telegramConnectionFingerprint(input); this.#current(context);
    let requests = this.#requests.get(context); if (!requests) { requests = new Map(); this.#requests.set(context, requests); }
    let existing = requests.get(input.idempotency_key);
    if (existing?.flowId) { const flow = this.#flows.get(existing.flowId); if (!flow || performance.now() >= flow.expires) { requests.delete(input.idempotency_key); existing = undefined; } }
    if (existing) { if (existing.fingerprint !== fingerprint) return Promise.reject(new Error("idempotency_key is already used by another connection request")); return existing.task.then(result => this.directStatus(context, result.operation_id)); }
    const task = this.#tracked(() => this.#start(context, input)), reservation: { fingerprint: string; task: Promise<TelegramConnectionStatus>; flowId?: string } = { fingerprint, task }; requests.set(input.idempotency_key, reservation);
    void task.then(result => { reservation.flowId = result.operation_id; }, () => undefined);
    void task.then(() => undefined, () => { requests!.delete(input.idempotency_key); }); return task;
  }
  async #start(context: AccountContext, raw: unknown): Promise<TelegramConnectionStatus> {
    const input = validateTelegramConnectionStart(raw), fingerprint = telegramConnectionFingerprint(input); this.#current(context);
    const existing = [...this.#flows.values()].find(flow => flow.context === context && flow.requestKey === input.idempotency_key && performance.now() < flow.expires);
    if (existing) { if (existing.fingerprint !== fingerprint) throw new Error("idempotency_key is already used by another connection request"); return this.directStatus(context, existing.id); }
    const prepared = await this.prepare(context, { platform: "telegram", intent: "connect", method: input.method === "qr" ? "telegram_qr" : "telegram_phone" }); const flow = this.#flow(context, prepared.flow_id);
    flow.requestKey = input.idempotency_key; flow.fingerprint = fingerprint; flow.input = input; flow.phase = "ready_to_begin";
    await this.begin(context, { flow_id: flow.id, confirmed: true, idempotency_key: input.idempotency_key }); return this.directStatus(context, flow.id);
  }
  directStatus(context: AccountContext, id: string): TelegramConnectionStatus { const flow = this.#flow(context, id); return { operation_id: id, phase: flow.phase === "awaiting_user_in_app" ? "awaiting_telegram_qr_scan" : flow.phase === "awaiting_secret_in_app" && flow.client ? flow.code ? "awaiting_telegram_otp" : "awaiting_telegram_two_factor" : terminal(flow) ? flow.phase as "completed" | "canceled" | "failed" : "starting", expires_at: Date.now() + Math.max(0, Math.floor(flow.expires - performance.now())), ...(flow.qr ? { qr_code: { expires_at: flow.qr.expires_at } } : {}), ...(flow.delivery ? { code_delivery: flow.delivery } : {}), ...(flow.account ? { operator_account: flow.account } : {}), ...(flow.error ? { error_code: flow.error } : {}) }; }
  stopAcceptingWork(): void { this.#closing = true; this.#shutdown.abort(new Error("telegram.connection_stopping")); for (const flow of this.#flows.values()) if (!terminal(flow)) { flow.cancel.abort(new Error("telegram.connection_stopping")); flow.password?.reject(flow.cancel.signal.reason); } }
  async close(): Promise<void> { this.stopAcceptingWork(); const results = await Promise.allSettled([...this.#pending, ...[...this.#flows.values()].map(async flow => { if (flow.client) await flow.client.destroy(); if (flow.task) await flow.task; })]); const errors = [...this.#errors.splice(0), ...results.filter((result): result is PromiseRejectedResult => result.status === "rejected").map(result => result.reason as unknown)].filter(error => error !== this.#shutdown.signal.reason && !(error instanceof Error && error.message === "telegram.connection_stopping") && ![...this.#flows.values()].some(flow => error === flow.context.signal.reason || error === flow.cancel.signal.reason)); this.#flows.clear(); if (errors.length) throw new AggregateError(errors, "telegram.connection_close_failed"); }
}
