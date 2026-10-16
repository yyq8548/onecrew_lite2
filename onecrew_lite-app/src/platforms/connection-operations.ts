import { z } from "zod";
import { tl, MtTimeoutError, TransportError, HttpProxyConnectionError, SocksProxyConnectionError } from "@mtcute/node";
import type { AccountContext, AccountContextService } from "../identity/account-context.ts";
import type { OperationRegistry } from "../application/operation-registry.ts";
import { mcpConnection } from "../service/mcp-contracts.ts";
import { creatorU64 } from "@onecrew/runtime-contracts/creators";
import { OwnerPlatformVaultStore } from "./browser/owner-vault-store.ts";
import { readOwnedOperatorProfile } from "./browser/operator-profile-operations.ts";
import { proxyConfigSchema, probeProxy } from "./browser/proxy.ts";
import { verifyBrowserLoginIdentity, InstagramLoginResponseError, InstagramLoginSessionError } from "./browser/browser-login-identity.ts";
import { NativeReadTransportError } from "../native-capabilities/network.ts";
import { XReadFailure } from "./browser/native-x-search.ts";
import { CreatorProviderReadError } from "./candidates/projection.ts";
import { YoutubeCredentialProbeError } from "./browser/youtube-credentials-validation.ts";
import type { YoutubeCredentials } from "./browser/youtube-credentials.ts";
import { parseStoredJson } from "../contracts/stored-json.ts";

export const connectionOperationIds = ["x.connection.get", "instagram.connection.get", "telegram.connection.get", "youtube.connection.get", "x.connection.check", "instagram.connection.check", "telegram.connection.check", "youtube.connection.check", "email.connection.check"] as const;
type Operation = typeof connectionOperationIds[number]; type Platform = "x" | "instagram" | "telegram";
export const connectionCheckSchema = z.strictObject({ status: z.enum(["ok", "proxy_unreachable", "network_unreachable", "credential_rejected", "provider_unavailable"]), proxy_configured: z.boolean(), egress: z.strictObject({ ip: z.string(), country: z.string() }).nullable(), account_handle: z.string().nullable(), checked_at: creatorU64 });
type Status = z.infer<typeof connectionCheckSchema>["status"];
const integer = z.union([z.bigint(), z.number().int().safe()]);
const profileMetadata = z.object({ accountRefId: z.string(), platform: z.enum(["x", "instagram", "telegram"]), provider_account_id: z.string(), username: z.string(), updated_at: z.string(), credentialReadable: z.boolean(), profileName: z.string().nullable() });
const youtubeMetadata = z.object({ accountRefId: z.string(), provider: z.literal("youtube"), label: z.string(), metadata_json: z.string(), disabled: integer, cooldown_until: integer.nullable(), last_used_at: integer.nullable(), last_error: z.string().nullable(), created_at: z.string(), updated_at: z.string(), credentialReadable: z.boolean() });
const phone = (value: string) => (value.match(/[0-9]/gu)?.length ?? 0) >= 7 && /^[0-9+() -]*$/u.test(value);
const unsigned = (value: number | bigint) => BigInt(value) < 0n ? BigInt.asUintN(64, BigInt(value)) : value;
export function projectOperatorConnectionGet(platform: Platform, id: string, rows: readonly Record<string, unknown>[]) {
  const raw = rows.find(value => value.accountRefId === id && value.platform === platform);
  if (!raw) return { status: "not_found" as const, operator_account_id: id };
  const row = profileMetadata.parse(raw), publicName = platform === "telegram" && phone(row.username) ? "" : row.username;
  const handle = row.username.trim().replace(/^@+/u, ""), label = platform === "telegram" ? handle && !phone(handle) ? `@${handle}` : `Telegram account ${id.slice(0, 8)}` : row.profileName?.trim() || row.username;
  return { status: "found" as const, detail: { operator_account_id: id, provider_account_id: row.provider_account_id, username: publicName, label, configured: row.credentialReadable, credential_availability: row.credentialReadable ? "available" as const : "unavailable" as const, updated_at: row.updated_at } };
}
export function projectYoutubeConnectionGet(id: string, rows: readonly Record<string, unknown>[], now = Math.floor(Date.now() / 1000)) {
  const raw = rows.find(value => value.accountRefId === id && value.provider === "youtube");
  if (!raw) return { status: "not_found" as const, credential_id: id };
  const row = youtubeMetadata.parse(raw), decoded = parseStoredJson(row.metadata_json), metadata = decoded !== null && typeof decoded === "object" && !Array.isArray(decoded) ? decoded as Record<string, unknown> : {}, issues: string[] = [];
  const project = typeof metadata.project === "string" ? metadata.project.trim() || null : null;
  if (metadata.api !== "youtube_data_api_v3" || metadata.project != null && typeof metadata.project !== "string") issues.push("invalid_metadata");
  if (!row.credentialReadable) issues.push("credential_unreadable");
  if (BigInt(row.disabled) !== 0n) issues.push("disabled");
  if (row.cooldown_until !== null && BigInt(unsigned(row.cooldown_until)) > BigInt(now)) issues.push("cooling_down");
  if (row.last_error !== null) issues.push("recorded_failure");
  return { status: "found" as const, detail: { credential_id: id, label: row.label, ...(project === null ? {} : { project }), availability: issues.length ? "unavailable" as const : "available" as const, issues, disabled: BigInt(row.disabled) !== 0n, ...(row.cooldown_until === null ? {} : { cooldown_until: unsigned(row.cooldown_until) }), ...(row.last_used_at === null ? {} : { last_checked_at: unsigned(row.last_used_at) }), created_at: row.created_at, updated_at: row.updated_at } };
}
export function connectionOperationScope(operation: Operation) {
  return z.strictObject({ operation_id: z.literal(operation), schema_revision: z.string(), connection: mcpConnection, arguments: z.strictObject({}) }).superRefine((input, issue) => {
    const platform = operation.split(".")[0], connection = input.connection;
    if (platform === "youtube" ? connection.kind !== "service_credential" || connection.toolkit !== "youtube" : platform === "email" ? connection.kind !== "mailbox" || connection.toolkit !== "email" : connection.kind !== "operator_account" || connection.toolkit !== platform) issue.addIssue({ code: "custom", message: "connection_selection_invalid" });
  });
}
export function connectionProviderFailure(platform: Platform, error: unknown): Status {
  if (error instanceof InstagramLoginResponseError) return "provider_unavailable";
  if (error instanceof InstagramLoginSessionError) return "credential_rejected";
  if (error instanceof XReadFailure || error instanceof NativeReadTransportError) return error.status === null ? "network_unreachable" : platform === "x" && [401, 403].includes(error.status) ? "credential_rejected" : "provider_unavailable";
  if (error instanceof CreatorProviderReadError && ["x_session_missing", "instagram_session_missing"].includes(error.message)) return "credential_rejected";
  if (error instanceof tl.RpcError) return error.code === 401 ? "credential_rejected" : "provider_unavailable";
  if (platform === "telegram" && (error instanceof MtTimeoutError || error instanceof TransportError || error instanceof HttpProxyConnectionError || error instanceof SocksProxyConnectionError)) return "network_unreachable";
  if (platform === "telegram" && error instanceof CreatorProviderReadError && ["telegram_home_auth_key_unavailable", "telegram_dc_invalid"].includes(error.message)) return "credential_rejected";
  if (error instanceof z.ZodError) return "provider_unavailable";
  throw error;
}
function check(status: Status, proxy = false, egress: { ip: string; country: string } | null = null, handle: string | null = null) { return connectionCheckSchema.parse({ status, proxy_configured: proxy, egress, account_handle: handle, checked_at: Math.floor(Date.now() / 1000) }); }
export interface ConnectionOperationsDependencies {
  store: OwnerPlatformVaultStore;
  telegram: { checkProfile(context: AccountContext, id: string): Promise<{ id: string; username: string | null }> };
  youtube: Pick<YoutubeCredentials, "test">;
  gmailProfile(context: AccountContext, mailboxRefId: string): Promise<string>;
}
export class ConnectionOperations {
  readonly #accounts: AccountContextService; readonly #dependencies: ConnectionOperationsDependencies;
  constructor(accounts: AccountContextService, dependencies: ConnectionOperationsDependencies) { this.#accounts = accounts; this.#dependencies = dependencies; }
  async get(context: AccountContext, platform: Platform | "youtube", id: string) {
    this.#accounts.assertCurrent(context);
    return this.#dependencies.store.inspect(context, scope => platform === "youtube" ? projectYoutubeConnectionGet(id, scope.services) : projectOperatorConnectionGet(platform, id, scope.profiles));
  }
  async operatorCheck(context: AccountContext, platform: Platform, id: string) {
    await this.#dependencies.store.inspect(context, scope => {
      const found = scope.profiles.find(value => value.accountRefId === id && value.platform === platform);
      if (!found || !profileMetadata.parse(found).credentialReadable) throw new Error("connection_unavailable");
    });
    const current = await this.#dependencies.store.read(context, scope => readOwnedOperatorProfile(scope, id)); this.#accounts.assertCurrent(context);
    if (current.row.platform !== platform) throw new Error("connection_unavailable");
    const assertCurrent = async () => {
      this.#accounts.assertCurrent(context); context.signal.throwIfAborted();
      await this.#dependencies.store.inspect(context, scope => { const entry = scope.manifest.profiles.find(value => value.accountRefId === id); if (!entry || entry.platform !== platform || entry.providerAccountId !== current.entry.providerAccountId || entry.vaultRevision !== current.entry.vaultRevision) throw new Error("connection_changed"); });
      this.#accounts.assertCurrent(context);
    };
    let egress: { ip: string; country: string } | null = null;
    const configured = current.secret.proxy != null;
    if (configured) {
      const parsed = proxyConfigSchema.safeParse(current.secret.proxy); if (!parsed.success) return check("proxy_unreachable", true);
      const observed = await probeProxy(parsed.data, context.signal).then(value => ({ value }), error => { context.signal.throwIfAborted(); if (error instanceof Error && error.message.startsWith("proxy.probe_failed:")) return { failed: true as const }; throw error; });
      await assertCurrent(); if (!("value" in observed)) return check("proxy_unreachable", true);
      egress = { ip: observed.value.ip, country: observed.value.country };
    }
    if (platform === "telegram" && (!configured || !proxyConfigSchema.safeParse(current.secret.proxy).success || proxyConfigSchema.parse(current.secret.proxy).kind !== "socks5")) return check("proxy_unreachable", configured, egress);
    if (platform === "telegram" && (!current.secret.telegram_api_id || current.secret.telegram_api_id < 0 || !current.secret.telegram_api_hash?.trim() || !current.secret.telegram_session?.trim())) return check("credential_rejected", configured, egress);
    const outcome = await Promise.resolve().then(async () => {
      if (platform === "telegram") { const identity = await this.#dependencies.telegram.checkProfile(context, id); return identity.username ?? identity.id; }
      const identity = await verifyBrowserLoginIdentity(platform, current.secret, { timezone: null, accept_language: null }, context.signal, assertCurrent, true);
      if (identity.providerAccountId !== current.entry.providerAccountId) throw new Error("connection_identity_mismatch"); return identity.username;
    }).then(handle => ({ handle }), error => { context.signal.throwIfAborted(); return { status: connectionProviderFailure(platform, error) }; });
    await assertCurrent();
    return "handle" in outcome ? check("ok", configured, egress, outcome.handle) : check(outcome.status, configured, egress);
  }
  async youtubeCheck(context: AccountContext, id: string) {
    const outcome = await this.#dependencies.youtube.test(context, id).then(value => ({ value }), error => { context.signal.throwIfAborted(); if (!(error instanceof YoutubeCredentialProbeError)) throw error; if (error.issue === "infrastructure") throw new Error("connection_check_internal", { cause: error }); return { error }; });
    this.#accounts.assertCurrent(context); if ("value" in outcome) return check(outcome.value.ok ? "ok" : "credential_rejected");
    return check(outcome.error.issue === "network_unreachable" ? "network_unreachable" : "provider_unavailable");
  }
  async gmailCheck(context: AccountContext, id: string) {
    const result = await this.#dependencies.gmailProfile(context, id).then(email => ({ email }), error => {
      context.signal.throwIfAborted(); if (!(error instanceof Error)) throw error;
      const code = error.message;
      if (["gmail_read_timeout", "gmail_read_transport_failed"].includes(code)) return { status: "network_unreachable" as const };
      if (code === "gmail_authorization_required" || code.startsWith("gmail_read_http_401")) return { status: "credential_rejected" as const };
      if (code.startsWith("gmail_read_") && code !== "gmail_read_cancelled") return { status: "provider_unavailable" as const };
      throw error;
    }); this.#accounts.assertCurrent(context); return "email" in result ? check("ok", false, null, result.email) : check(result.status);
  }
}
export function registerConnectionOperations(registry: OperationRegistry, accounts: AccountContextService, dependencies: ConnectionOperationsDependencies): ConnectionOperations {
  const operations = new ConnectionOperations(accounts, dependencies);
  for (const operation of connectionOperationIds) registry.register(operation, connectionOperationScope(operation), async (input, context) => {
    const connection = input.connection;
    if (connection.kind === "operator_account") return operation.endsWith(".get") ? operations.get(context, connection.toolkit, connection.operator_account_id) : operations.operatorCheck(context, connection.toolkit, connection.operator_account_id);
    if (connection.kind === "service_credential") return operation.endsWith(".get") ? operations.get(context, "youtube", connection.credential_id) : operations.youtubeCheck(context, connection.credential_id);
    if (connection.kind === "mailbox") return operations.gmailCheck(context, connection.mailbox_ref_id);
    throw new Error("connection_selection_invalid");
  });
  return operations;
}
