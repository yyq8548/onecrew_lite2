import { randomUUID, createHash } from "node:crypto";
import { lstat } from "node:fs/promises";
import { join } from "node:path";
import { z } from "zod";
import type { AccountContext, AccountContextService } from "../../identity/account-context.ts";
import type { AccountDatabases } from "../../identity/account-databases.ts";
import { createPrivateDirectory, verifyPrivateDirectory } from "../../storage/private-backup.ts";
import { decryptVaultEnvelope } from "../../storage/vault-reader.ts";
import { parseApplicationJson, stringifyApplicationJson } from "../../contracts/application-json.ts";
import { readChromeCookies } from "../../native-capabilities/chrome-cookies.ts";
import { OwnerPlatformVaultStore, assertOwnedProfile, encryptOwnerVaultSecret } from "./owner-vault-store.ts";
import { platformProfileSecretSchema, type PlatformProfileSecret } from "./platform-vault.ts";
import { createBrowserProxy, probeProxy, proxyConfigSchema, type ProxyConfig } from "./proxy.ts";
import { chromeStatus, launchOwnedChrome, type OwnedChromeProcess } from "./chrome-host.ts";
import { verifyBrowserLoginIdentity } from "./browser-login-identity.ts";
import { verifyOwnedChromeDirectory } from "./binding.ts";
import { copyOwnedBrowserProfile } from "./browser-profile-copy.ts";

type Phase = "awaiting_secret_in_app" | "ready_to_begin" | "starting" | "awaiting_user_in_app" | "finalizing" | "completed" | "canceled" | "failed";
type Platform = "x" | "instagram";
interface Flow { id: string; context: AccountContext; platform: Platform; intent: "connect" | "reconnect"; phase: Phase; expires: number; controller: AbortController; label: string; target?: { accountRefId: string; profileId: string; providerAccountId: string; revision: string; secret: PlatformProfileSecret }; proxy?: ProxyConfig; directory?: string; browser?: OwnedChromeProcess; forwarder?: Awaited<ReturnType<typeof createBrowserProxy>>; geo?: Awaited<ReturnType<typeof probeProxy>>; beginKey?: string; finalizeKey?: string; pending?: Promise<unknown>; account?: { operator_account_id: string; platform: Platform; provider_account_id: string; username: string; label: string; configured: boolean; credential_availability: "available"; updated_at: string }; error?: string }
const prepareInput = z.strictObject({ platform: z.enum(["x", "instagram"]), intent: z.enum(["connect", "reconnect"]), method: z.literal("browser"), operator_account_id: z.uuid().nullish(), label: z.string().max(120).nullish() });
const transitionInput = z.strictObject({ flow_id: z.uuid(), confirmed: z.boolean(), idempotency_key: z.string().min(1).max(128) });
const terminal = (flow: Flow) => ["completed", "canceled", "failed"].includes(flow.phase);
const completed = (flow: Flow): boolean => flow.phase === "completed";

export class OperatorBrowserConnections {
  readonly #accounts: AccountContextService; readonly #directories: AccountDatabases; readonly #store: OwnerPlatformVaultStore; readonly #changed: (context: AccountContext) => void;
  readonly #flows = new Map<string, Flow>(); readonly #portals = new Set<{ browser: OwnedChromeProcess; forwarder: Awaited<ReturnType<typeof createBrowserProxy>> }>(); readonly #cleanupFailures: unknown[] = []; #closed = false;
  readonly #direct = new WeakMap<AccountContext, Map<string, { fingerprint: string; work: Promise<string> }>>();
  constructor(accounts: AccountContextService, directories: AccountDatabases, store: OwnerPlatformVaultStore, changed: (context: AccountContext) => void = () => {}) { this.#accounts = accounts; this.#directories = directories; this.#store = store; this.#changed = changed; }
  owns(id: string): boolean { return this.#flows.has(id); }
  async startDirect(context: AccountContext, raw: unknown) {
    const input = z.strictObject({ platform: z.enum(["x", "instagram"]), profile_name: z.string().trim().min(1).max(120), proxy: proxyConfigSchema, idempotency_key: z.string().min(1).max(128) }).parse(raw); this.#accounts.assertCurrent(context);
    const fingerprint = createHash("sha256").update(stringifyApplicationJson([input.platform, input.profile_name, input.proxy])).digest("hex"), requests = this.#direct.get(context) ?? new Map(), existing = requests.get(input.idempotency_key);
    if (existing) { if (existing.fingerprint !== fingerprint) throw new Error("browser.request_idempotency_conflict"); return this.directStatus(context, await existing.work); }
    let createdId: string | undefined;
    const work = (async () => {
      if ((await chromeStatus()).state !== "ready") throw new Error("browser_not_installed"); await probeProxy(input.proxy, context.signal);
      const prepared = await this.prepare(context, { platform: input.platform, intent: "connect", method: "browser", label: input.profile_name }); createdId = prepared.flow_id;
      const flow = this.#get(context, prepared.flow_id); flow.proxy = input.proxy; flow.phase = "ready_to_begin";
      await this.begin(context, { flow_id: prepared.flow_id, confirmed: true, idempotency_key: input.idempotency_key }); return prepared.flow_id;
    })(); requests.set(input.idempotency_key, { fingerprint, work }); this.#direct.set(context, requests);
    try { return this.directStatus(context, await work); } catch (error) { if (createdId) requests.set(input.idempotency_key, { fingerprint, work: Promise.resolve(createdId) }); else requests.delete(input.idempotency_key); throw error; }
  }
  async directStatus(context: AccountContext, id: string) { const flow = this.#get(context, id); return { operation_id: flow.id, platform: flow.platform, phase: flow.phase === "awaiting_user_in_app" ? "awaiting_browser_login" : flow.phase === "awaiting_secret_in_app" || flow.phase === "ready_to_begin" || flow.phase === "finalizing" ? "starting" : flow.phase, expires_at: flow.expires, browser_capability: await chromeStatus(), ...(flow.account ? { operator_account: flow.account } : {}), ...(flow.error ? { error_code: flow.error } : {}) }; }
  #get(context: AccountContext, id: string): Flow { this.#accounts.assertCurrent(context); if (this.#closed) throw new Error("browser.connections_closed"); const flow = this.#flows.get(id); if (!flow || flow.context !== context) throw new Error("operator account connection flow not found"); if (Date.now() >= flow.expires && !terminal(flow)) { flow.controller.abort(); throw new Error("operator account connection flow expired"); } return flow; }
  async #view(flow: Flow) { return { flow_id: flow.id, platform: flow.platform, intent: flow.intent, method: "browser", kind: flow.intent === "connect" ? "browser_connect" : "browser_reconnect", phase: flow.phase, expires_at: flow.expires, ...(flow.account ? { operator_account: flow.account } : {}), allowed_actions: terminal(flow) ? ["status"] : flow.phase === "awaiting_secret_in_app" ? ["open_desktop", "cancel"] : flow.phase === "ready_to_begin" ? ["begin", "cancel"] : flow.phase === "awaiting_user_in_app" ? ["open_desktop", "status", "finalize", "cancel"] : ["status", "cancel"], browser_capability: await chromeStatus(), ...(flow.error ? { error_code: flow.error } : {}), ...(!terminal(flow) ? { handoff: { kind: flow.phase === "awaiting_secret_in_app" ? "configure_connection_in_desktop" : "complete_browser_login_in_desktop", message: "Complete the connection in the application." } } : {}) }; }
  async prepare(context: AccountContext, raw: unknown) {
    const input = prepareInput.parse(raw); this.#accounts.assertCurrent(context); if (this.#closed) throw new Error("browser.connections_closed");
    for (const [id, previous] of this.#flows) if (previous.expires < Date.now() && terminal(previous)) this.#flows.delete(id);
    if (this.#flows.size >= 128) throw new Error("browser.connection_limit");
    if (input.intent === "connect" && (input.operator_account_id || input.method !== "browser")) throw new Error("browser.connect_input_invalid");
    const target = await this.#store.read(context, scope => {
      if (input.intent === "connect") return undefined;
      if (!input.operator_account_id) throw new Error("browser.reconnect_target_required");
      const entry = assertOwnedProfile(scope, input.operator_account_id), row = scope.vault.prepare("SELECT platform,provider_account_id,encrypted_secret FROM profiles WHERE id=?").get(entry.profileId);
      if (!row || row.platform !== input.platform || row.provider_account_id !== entry.providerAccountId || !(row.encrypted_secret instanceof Uint8Array)) throw new Error("browser.reconnect_identity_mismatch");
      const clear = decryptVaultEnvelope(scope.key, row.encrypted_secret); try { return { accountRefId: entry.accountRefId, profileId: entry.profileId, providerAccountId: entry.providerAccountId, revision: entry.vaultRevision, secret: platformProfileSecretSchema.parse(parseApplicationJson(clear)) }; } finally { clear.fill(0); }
    });
    const flow: Flow = { id: randomUUID(), context, platform: input.platform, intent: input.intent, phase: target ? "ready_to_begin" : "awaiting_secret_in_app", expires: Date.now() + 1800000, controller: new AbortController(), label: input.label?.trim() || target?.secret.profile_name || "" , ...(target ? { target, proxy: proxyConfigSchema.parse(target.secret.proxy) } : {}) };
    this.#flows.set(flow.id, flow); context.signal.addEventListener("abort", () => { flow.controller.abort(); void this.#cleanup(flow).catch(error => this.#cleanupFailures.push(error)); }, { once: true }); return this.#view(flow);
  }
  async configure(context: AccountContext, raw: unknown) { const input = z.strictObject({ flowId: z.uuid(), profileName: z.string().trim().min(1).max(120), proxy: proxyConfigSchema }).parse(raw), flow = this.#get(context, input.flowId); if (!["awaiting_secret_in_app", "ready_to_begin"].includes(flow.phase) || flow.target) throw new Error("browser.configuration_phase_invalid"); await probeProxy(input.proxy, AbortSignal.any([context.signal, flow.controller.signal])); this.#get(context, flow.id); flow.label = input.profileName; flow.proxy = input.proxy; flow.phase = "ready_to_begin"; return this.#view(flow); }
  async begin(context: AccountContext, raw: unknown) {
    const input = transitionInput.parse(raw), flow = this.#get(context, input.flow_id); if (!input.confirmed) throw new Error("confirmed=true is required");
    if (flow.beginKey) { if (flow.beginKey !== input.idempotency_key) throw new Error("browser.begin_idempotency_conflict"); if (flow.pending) await flow.pending; return this.#view(flow); }
    if (flow.phase !== "ready_to_begin" || !flow.proxy) throw new Error("browser.begin_phase_invalid"); flow.beginKey = input.idempotency_key; flow.phase = "starting";
    const work = (async () => { try {
      const signal = AbortSignal.any([context.signal, flow.controller.signal]), geo = await probeProxy(flow.proxy!, signal), owner = await this.#directories.accountDirectory(context), profiles = join(owner.directory, "profiles");
      const stat = await lstat(profiles).then(value => value, error => { if (error.code === "ENOENT") return null; throw error; }); if (stat) await verifyPrivateDirectory(profiles); else await createPrivateDirectory(profiles);
      const directory = join(profiles, `login-${flow.id}`); if (flow.target) { const source = await verifyOwnedChromeDirectory(profiles, flow.target.secret.browser_profile_dir); await copyOwnedBrowserProfile(source.path, directory, signal); } else await createPrivateDirectory(directory);
      signal.throwIfAborted(); owner.assertOwned(); this.#accounts.assertCurrent(context); flow.directory = directory; flow.geo = geo; flow.forwarder = await createBrowserProxy(flow.proxy!);
      flow.browser = await launchOwnedChrome({ profileDirectory: directory, url: new URL(flow.platform === "x" ? "https://x.com/login" : "https://www.instagram.com/accounts/login/"), proxy: flow.forwarder, ...(geo.timezone ? { timezone: geo.timezone } : {}), ...(geo.accept_language ? { acceptLanguage: geo.accept_language } : {}), signal });
      signal.throwIfAborted(); this.#accounts.assertCurrent(context); flow.phase = "awaiting_user_in_app";
    } catch (error) { flow.phase = flow.controller.signal.aborted || context.signal.aborted ? "canceled" : "failed"; flow.error = "browser_connection_begin_failed"; await this.#cleanup(flow); throw error; } })(); flow.pending = work; try { await work; return this.#view(flow); } finally { delete flow.pending; }
  }
  async finalize(context: AccountContext, raw: unknown) {
    const input = transitionInput.parse(raw), flow = this.#get(context, input.flow_id); if (!input.confirmed) throw new Error("confirmed=true is required");
    if (flow.finalizeKey) { if (flow.finalizeKey !== input.idempotency_key) throw new Error("browser.finalize_idempotency_conflict"); if (flow.pending) await flow.pending; return this.#view(flow); }
    if (flow.phase !== "awaiting_user_in_app" || !flow.browser || !flow.directory || !flow.geo || !flow.proxy) throw new Error("browser.finalize_phase_invalid"); flow.finalizeKey = input.idempotency_key; flow.phase = "finalizing";
    const work = (async () => { try {
      await flow.browser!.close(); delete flow.browser; if (flow.forwarder) { await flow.forwarder.close(); delete flow.forwarder; }
      const cookies = await readChromeCookies(flow.directory!, flow.platform === "x" ? ["x.com", "twitter.com"] : ["instagram.com"]); if (!cookies.length) throw new Error("browser.login_cookies_missing");
      const base: PlatformProfileSecret = { provider_account_id: "", cookies, x_chat_passcode: null, instagram_session_settings: null, telegram_api_id: null, telegram_api_hash: null, telegram_session: null, profile_name: flow.label, browser_profile_dir: flow.directory!, fingerprint_seed: stringifyApplicationJson({ timezone: flow.geo!.timezone, accept_language: flow.geo!.accept_language }), proxy: flow.proxy, proxy_egress: flow.geo!.ip };
      const signal = AbortSignal.any([context.signal, flow.controller.signal]), assertCurrent = async () => { signal.throwIfAborted(); this.#accounts.assertCurrent(context); }, verified = await verifyBrowserLoginIdentity(flow.platform, base, flow.geo!, signal, assertCurrent); await assertCurrent();
      if (flow.target && verified.providerAccountId !== flow.target.providerAccountId) throw new Error("browser.reconnect_identity_mismatch");
      flow.account = await this.#store.mutate(context, scope => {
        signal.throwIfAborted(); this.#accounts.assertCurrent(context); const id = flow.target?.profileId ?? randomUUID(), accountRefId = flow.target?.accountRefId ?? id, now = String(Math.floor(Date.now() / 1000));
        if (flow.target) { const entry = assertOwnedProfile(scope, accountRefId); if (entry.profileId !== id || entry.providerAccountId !== verified.providerAccountId || entry.vaultRevision !== flow.target.revision) throw new Error("browser.reconnect_target_changed"); }
        else if (scope.vault.prepare("SELECT id FROM profiles WHERE platform=? AND provider_account_id=?").get(flow.platform, verified.providerAccountId)) throw new Error("browser.account_already_exists_use_reconnect");
        const clear = Buffer.from(stringifyApplicationJson(verified.secret)); try { scope.vault.prepare("INSERT INTO profiles(id,platform,provider_account_id,username,encrypted_secret,created_at,updated_at) VALUES(?,?,?,?,?,?,?) ON CONFLICT(id) DO UPDATE SET username=excluded.username,encrypted_secret=excluded.encrypted_secret,updated_at=excluded.updated_at").run(id, flow.platform, verified.providerAccountId, verified.username, encryptOwnerVaultSecret(scope.key, clear), now, now); } finally { clear.fill(0); }
        scope.creator.prepare("INSERT INTO account_refs(id,platform,kind,label,active,availability,created_at,updated_at) VALUES(?,?,'profile',?,1,'available',?,?) ON CONFLICT(id) DO UPDATE SET label=excluded.label,active=1,availability='available',updated_at=excluded.updated_at").run(accountRefId, flow.platform, verified.username, Number(now), Number(now));
        if (!flow.target) scope.manifest.profiles.push({ accountRefId, profileId: id, platform: flow.platform, providerAccountId: verified.providerAccountId, vaultRevision: "0".repeat(64) });
        return { operator_account_id: accountRefId, platform: flow.platform, provider_account_id: verified.providerAccountId, username: verified.username, label: flow.label || verified.username, configured: true, credential_availability: "available" as const, updated_at: now };
      }); flow.phase = "completed"; delete flow.proxy; delete flow.target; if (!context.signal.aborted) this.#changed(context);
    } catch (error) { if (!flow.account) { flow.phase = flow.controller.signal.aborted || context.signal.aborted ? "canceled" : "failed"; flow.error = "browser_connection_finalize_failed"; } await this.#cleanup(flow); throw error; } })(); flow.pending = work; try { await work; return this.#view(flow); } finally { delete flow.pending; }
  }
  async status(context: AccountContext, raw: unknown) { return this.#view(this.#get(context, z.strictObject({ flow_id: z.uuid() }).parse(raw).flow_id)); }
  async handoff(context: AccountContext, raw: unknown) { const flow = this.#get(context, z.strictObject({ flowId: z.uuid() }).parse(raw).flowId); return { flow_id: flow.id }; }
  async cancel(context: AccountContext, raw: unknown) { const input = z.strictObject({ flow_id: z.uuid(), confirmed: z.boolean() }).parse(raw), flow = this.#get(context, input.flow_id); if (!input.confirmed) throw new Error("confirmed=true is required"); if (completed(flow)) return this.#view(flow); flow.controller.abort(); if (flow.pending) await flow.pending.then(() => undefined, () => undefined); if (!completed(flow)) { await this.#cleanup(flow); flow.phase = "canceled"; delete flow.proxy; delete flow.target; } return this.#view(flow); }
  async #cleanup(flow: Flow) { const results = await Promise.allSettled([...(flow.browser ? [flow.browser.close()] : []), ...(flow.forwarder ? [flow.forwarder.close()] : [])]); delete flow.browser; delete flow.forwarder; const errors = results.filter(value => value.status === "rejected").map(value => value.reason); if (errors.length) throw new AggregateError(errors, "browser.connection_cleanup_failed"); }
  async openTelegramCredentials(context: AccountContext, proxy: ProxyConfig) {
    this.#accounts.assertCurrent(context); if (this.#closed || this.#portals.size >= 8) throw new Error("browser.credentials_portal_unavailable");
    const owner = await this.#directories.accountDirectory(context), directory = await createPrivateDirectory(join(owner.directory, `telegram-credentials-${randomUUID()}`)), forwarder = await createBrowserProxy(proxy); let browser: OwnedChromeProcess | undefined;
    try {
      browser = await launchOwnedChrome({ profileDirectory: directory, url: new URL("https://my.telegram.org/apps"), proxy: forwarder, signal: context.signal }); owner.assertOwned(); this.#accounts.assertCurrent(context);
      const portal = { browser, forwarder }; this.#portals.add(portal);
      context.signal.addEventListener("abort", () => { void (async () => { try { await portal.browser.close(); } finally { await portal.forwarder.close(); this.#portals.delete(portal); } })().catch(error => this.#cleanupFailures.push(error)); }, { once: true });
      return { opened: true };
    } catch (error) { try { if (browser) await browser.close(); } finally { await forwarder.close(); } throw error; }
  }
  stopAcceptingWork(): void { this.#closed = true; for (const flow of this.#flows.values()) if (!terminal(flow)) flow.controller.abort(); }
  async close(): Promise<void> { this.stopAcceptingWork(); const results = await Promise.allSettled([...this.#flows.values()].map(async flow => { if (flow.pending) await flow.pending.then(() => undefined, () => undefined); await this.#cleanup(flow); })); const portals = await Promise.allSettled([...this.#portals].map(async value => { try { await value.browser.close(); } finally { await value.forwarder.close(); } })); const errors = [...this.#cleanupFailures, ...results.filter(value => value.status === "rejected").map(value => value.reason), ...portals.filter(value => value.status === "rejected").map(value => value.reason)]; if (errors.length) throw new AggregateError(errors, "browser.connection_close_failed"); }
}
