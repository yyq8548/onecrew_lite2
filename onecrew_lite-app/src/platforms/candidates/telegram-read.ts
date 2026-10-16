import { isIP } from "node:net";
import { TelegramClient, MemoryStorage, proxyTransportFromUrl, Chat, MtcuteError, tl } from "@mtcute/node";
import type { InputStringSessionData } from "@mtcute/core/utils.js";
import { readStringSession } from "@mtcute/core/utils.js";
import { z } from "zod";
import { parseStoredJson } from "../../contracts/stored-json.ts";
import type { CreatorSaveCandidateInput, ReadCreatorProfile } from "../../creators/candidate-input.ts";
import type { CandidateEvidence } from "../../creators/candidate-contracts.ts";
import type { PlatformProfileSecret } from "../browser/platform-vault.ts";
import { nativeProxy } from "./social-read.ts";
import { CreatorProviderReadError } from "./projection.ts";

const snapshotSchema = z.strictObject({ home_dc: z.number().int().min(1).max(5), dc_options: z.array(z.strictObject({ id: z.number().int().min(1).max(5), ipv4: z.string(), ipv6: z.string(), auth_key: z.string().regex(/^[a-fA-F\d]{512}$/u).nullable() })).min(1).max(5) }).refine(value => new Set(value.dc_options.map(option => option.id)).size === value.dc_options.length);
export function importGrammersSession(json: string): { session: InputStringSessionData; keys: readonly { id: number; key: Uint8Array }[] } {
  const raw = parseStoredJson(json, true);
  if (raw !== null && typeof raw === "object" && "format" in raw && raw.format === "onecrew-mtcute-v1") {
    const saved = z.strictObject({ format: z.literal("onecrew-mtcute-v1"), session: z.string().min(1) }).parse(raw), session = readStringSession(saved.session);
    return { session, keys: [] };
  }
  const data = snapshotSchema.parse(raw); const home = data.dc_options.find(option => option.id === data.home_dc);
  if (!home?.auth_key) throw new CreatorProviderReadError("provider_read", "telegram_home_auth_key_unavailable");
  const ipv4 = URL.parse(`tcp://${home.ipv4}`);
  if (!ipv4 || isIP(ipv4.hostname) !== 4 || !ipv4.port || ipv4.username || ipv4.password || ipv4.pathname || ipv4.search || ipv4.hash) throw new CreatorProviderReadError("provider_read", "telegram_dc_invalid");
  const dc = { id: home.id, ipAddress: ipv4.hostname, port: Number(ipv4.port) };
  return { session: { testMode: false, primaryDcs: { main: dc, media: dc }, authKey: Buffer.from(home.auth_key, "hex") }, keys: data.dc_options.filter(option => option.auth_key !== null).map(option => ({ id: option.id, key: Buffer.from(option.auth_key!, "hex") })) };
}
type AdministratorSource = NonNullable<ReadCreatorProfile["administrator"]>["source"];
export interface SavedTelegramCommunityResolver {
  resolve(creatorId: string, routeId: string, accountRefId: string): Promise<Readonly<{ externalId: string; kind: "channel" | "supergroup"; source: AdministratorSource }>>;
}
export async function readTelegramAdministrator(secret: PlatformProfileSecret, operatorProviderId: string, candidate: CreatorSaveCandidateInput, signal: AbortSignal, assertCurrent: () => Promise<void>, savedRoutes?: SavedTelegramCommunityResolver): Promise<ReadCreatorProfile> {
  if (candidate.target.platform !== "telegram_administrator" || candidate.connection.platform !== "telegram" || candidate.provenance.kind !== "administrator_selection") throw new CreatorProviderReadError("source_mismatch", "telegram_administrator_source_invalid");
  const target = candidate.target; const provenance = candidate.provenance; const accountRef = candidate.connection.account_ref_id;
  const proxy = nativeProxy(secret.proxy);
  if (!proxy?.startsWith("socks5://") || !secret.telegram_api_id || !secret.telegram_api_hash || !secret.telegram_session) throw new CreatorProviderReadError("provider_read", "telegram_session_or_socks5_unavailable");
  const converted = importGrammersSession(secret.telegram_session); const storage = new MemoryStorage();
  const client = new TelegramClient({ apiId: secret.telegram_api_id, apiHash: secret.telegram_api_hash, storage, transport: proxyTransportFromUrl(proxy), disableUpdates: true, logLevel: 0, fallbackDcs: [] });
  const scoped = client.withParams({ abortSignal: signal, timeout: 30000, maxRetryCount: 0, floodSleepThreshold: 0 });
  const read = async <T>(operation: () => Promise<T>) => { signal.throwIfAborted(); await assertCurrent(); const result = await operation().then(value => ({ value }), error => ({ error })); await assertCurrent(); signal.throwIfAborted(); if ("error" in result) { if (result.error instanceof MtcuteError || result.error instanceof tl.RpcError) throw new CreatorProviderReadError("provider_read", "telegram_administrators_read"); throw result.error; } return result.value; };
  try {
    await client.importSession(converted.session); for (const key of converted.keys) storage.authKeys.set(key.id, key.key);
    const me = await read(() => scoped.getMe());
    if (String(me.id) !== operatorProviderId) throw new CreatorProviderReadError("identity_mismatch", "telegram_operator_identity_mismatch");
    let source: AdministratorSource = { kind: "operator_account", operator_account_ref_id: accountRef };
    let expectedId = provenance.community_id; let expectedKind = provenance.community_kind; let chat: Chat | undefined;
    if (target.community.kind === "saved_community_route") {
      if (!savedRoutes) throw new Error("Saved Telegram community resolver is unavailable");
      const saved = await savedRoutes.resolve(target.community.creator_id, target.community.route_id, accountRef);
      if (saved.externalId !== expectedId || saved.kind !== expectedKind) throw new CreatorProviderReadError("source_mismatch", "telegram_saved_community_source_mismatch");
      source = saved.source;
    }
    if (target.community.kind === "public_username") { const username = target.community.username; chat = await read(() => scoped.getChat(username)); }
    else {
      if (target.community.kind === "stable_community") { expectedId = target.community.external_id; expectedKind = target.community.community_kind; }
      const iterator = scoped.iterDialogs({ limit: 2000, chunkSize: 100, archived: "keep" });
      for (;;) {
        const step = await read(() => iterator.next()); if (step.done) break;
        const peer = step.value.peer;
        if (peer instanceof Chat && peer.raw._ === "channel" && String(peer.raw.id) === expectedId) { chat = peer; break; }
      }
    }
    if (!chat || chat.raw._ !== "channel" || !["channel", "supergroup"].includes(chat.chatType) || String(chat.raw.id) !== provenance.community_id || chat.chatType !== provenance.community_kind || String(chat.raw.id) !== expectedId || chat.chatType !== expectedKind) throw new CreatorProviderReadError("identity_mismatch", "telegram_community_identity_mismatch");
    const page = await read(() => scoped.getChatMembers(chat.inputPeer, { type: "admins", offset: 0, limit: 200 }));
    if (page.length > 200) throw new CreatorProviderReadError("provider_read", "telegram_administrator_page_invalid");
    const member = page.find(member => String(member.user.id) === target.user_id && ["creator", "admin"].includes(member.status));
    if (!member) throw new CreatorProviderReadError("provider_read", "telegram_administrator_unavailable");
    const user = member.user; const handle = user.username; const evidence: CandidateEvidence[] = [];
    return { observed_at: Math.floor(Date.now() / 1000), account: { platform: "telegram", external_id: String(user.id), target_kind: user.isBot ? "bot" : "user", handle, display_name: [user.firstName, user.lastName].filter(value => value !== null).join(" "), profile_url: handle ? `https://t.me/${handle}` : null }, profile: { avatar_url: null, description: null, location: null, website_urls: [], follower_count: null }, evidence, warnings: [], administrator: { role: member.status === "creator" ? "owner" : "administrator", community: { external_id: String(chat.raw.id), title: chat.title, handle: chat.username, target_kind: chat.chatType as "channel" | "supergroup" }, source } };
  } finally { try { await client.destroy(); } finally { converted.session.authKey.fill(0); for (const key of converted.keys) key.key.fill(0); } }
}
