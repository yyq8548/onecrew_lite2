import { Chat, Long, MemoryStorage, TelegramClient, proxyTransportFromUrl, tl } from "@mtcute/node";
import { z } from "zod";
import type { OperationRegistry } from "../../application/operation-registry.ts";
import type { AccountContextService } from "../../identity/account-context.ts";
import { mcpConnection } from "../../service/mcp-contracts.ts";
import type { OwnedPlatformAccountResolver } from "./owned-accounts.ts";
import { importGrammersSession } from "../candidates/telegram-read.ts";
import { nativeProxy } from "../candidates/social-read.ts";

export const telegramPublicReadOperationIds = ["telegram.community.get", "telegram.community.administrators.list"] as const;
const username = (value: string) => /^[A-Za-z0-9_]+$/u.test(value) && /[A-Za-z_]/u.test(value);
function parsePublicUsername(input: string): string | null {
  const value = input.trim(), bare = value.startsWith("@") ? value.slice(1) : value;
  if (username(bare) && !/[/:?#]/u.test(value)) return bare;
  if (/^tg:/iu.test(value)) return null;
  const url = URL.parse(value.includes("://") ? value : `https://${value}`), parts = url?.pathname.split("/").filter(Boolean) ?? [];
  if (!url || !["t.me", "www.t.me", "telegram.me", "www.telegram.me"].includes(url.hostname.toLowerCase()) || parts.length !== 1 || !username(parts[0]!) || ["s", "c", "joinchat", "share", "proxy", "socks", "login", "iv", "addstickers", "addemoji", "setlanguage"].includes(parts[0]!.toLowerCase()) || [...url.searchParams.keys()].some(key => ["start", "startgroup", "game", "appname"].includes(key))) return null;
  return parts[0]!;
}
export function telegramPublicUsername(input: string): string { const parsed = parsePublicUsername(input); if (parsed === null) throw new Error("invalid_community_locator"); return parsed; }
export const telegramCommunityLocator = z.discriminatedUnion("type", [z.strictObject({ type: z.literal("public_username"), username: z.string().refine(value => parsePublicUsername(value) !== null) }), z.strictObject({ type: z.literal("stable_community"), external_id: z.string().refine(value => /^[1-9][0-9]*$/u.test(value) && BigInt(value) <= 9223372036854775807n), kind: z.enum(["channel", "supergroup"]) })]);
export function projectTelegramCommunity(chat: Chat, expected?: Extract<z.infer<typeof telegramCommunityLocator>, { type: "stable_community" }>) {
  if (chat.raw._ !== "channel" || !["channel", "supergroup"].includes(chat.chatType)) throw new Error("unsupported_community_kind");
  const id = z.number().int().safe().positive().parse(chat.raw.id).toString(), kind = chat.chatType as "channel" | "supergroup";
  if (expected && (expected.external_id !== id || expected.kind !== kind)) throw new Error("community_identity_mismatch");
  const title = chat.title.trim(); if (!title) throw new Error("invalid_community_metadata"); const handle = chat.username?.trim();
  return { external_id: id, title, ...(handle ? { username: handle } : {}), kind };
}
export function projectTelegramAdministrators(response: tl.channels.RawChannelParticipants) {
  const roles = new Map<number, "owner" | "administrator">();
  for (const participant of response.participants) if (participant._ === "channelParticipantCreator" || participant._ === "channelParticipantAdmin") roles.set(participant.userId, participant._ === "channelParticipantCreator" ? "owner" : "administrator");
  return response.users.flatMap(user => {
    if (user._ !== "user") return []; const role = roles.get(user.id); if (!role) return [];
    return [{ role, user: { id: z.number().int().safe().parse(user.id).toString(), username: user.username ?? null, first_name: user.firstName ?? null, last_name: user.lastName ?? null, is_bot: user.bot ?? false, verified: user.verified ?? false } }];
  });
}
export function registerTelegramPublicReadOperations(registry: OperationRegistry, accounts: AccountContextService, platforms: OwnedPlatformAccountResolver): void {
  for (const id of telegramPublicReadOperationIds) registry.register(id, z.strictObject({ operation_id: z.literal(id), schema_revision: z.string(), connection: mcpConnection, arguments: z.strictObject({ locator: telegramCommunityLocator }) }), async (input, context) => {
    if (input.connection.kind !== "operator_account" || input.connection.toolkit !== "telegram") throw new Error("public_read_connection_invalid");
    const locator = input.arguments.locator;
    return platforms.withProfile(context, input.connection.operator_account_id, "telegram", async (profile, secret, assertOwned) => {
      const current = async () => { accounts.assertCurrent(context); await assertOwned(); context.signal.throwIfAborted(); };
      const proxy = nativeProxy(secret.proxy); if (!proxy?.startsWith("socks5://") || !secret.telegram_api_id || !secret.telegram_api_hash || !secret.telegram_session) throw new Error("telegram.session_or_socks5_required");
      await current(); const converted = importGrammersSession(secret.telegram_session), storage = new MemoryStorage(), client = new TelegramClient({ apiId: secret.telegram_api_id, apiHash: secret.telegram_api_hash, storage, transport: proxyTransportFromUrl(proxy), disableUpdates: true, logLevel: 0, fallbackDcs: [] }), scoped = client.withParams({ abortSignal: context.signal, timeout: 30000, maxRetryCount: 0, floodSleepThreshold: 0 });
      const read = async <T>(operation: () => Promise<T>): Promise<T> => { await current(); const result = await operation().then(value => ({ ok: true as const, value }), error => ({ ok: false as const, error: error as unknown })); await current(); if (!result.ok) { const code = result.error instanceof tl.RpcError && /^[A-Z][A-Z0-9_]{0,100}$/u.test(result.error.text) ? result.error.text : "telegram_provider_read_failed"; throw new Error(code, { cause: result.error }); } return result.value; };
      try {
        await client.importSession(converted.session); for (const key of converted.keys) storage.authKeys.set(key.id, key.key);
        const me = await read(() => scoped.getMe()); if (String(me.id) !== profile.provider_account_id) throw new Error("telegram.operator_identity_mismatch");
        let chat: Chat | undefined;
        if (locator.type === "public_username") chat = await read(() => scoped.getChat(telegramPublicUsername(locator.username)));
        else {
          const dialogs = scoped.iterDialogs({ limit: 2000, chunkSize: 100, archived: "keep" });
          for (;;) { const item = await read(() => dialogs.next()); if (item.done) break; const peer = item.value.peer; if (peer instanceof Chat && peer.raw._ === "channel" && String(peer.raw.id) === locator.external_id) { chat = peer; break; } }
        }
        if (!chat) throw new Error("community_peer_unavailable"); const community = projectTelegramCommunity(chat, locator.type === "stable_community" ? locator : undefined);
        if (id === "telegram.community.get") return community;
        const channel = await read(() => client.resolveChannel(chat!.inputPeer));
        const response = await read(() => scoped.call({ _: "channels.getParticipants", channel, filter: { _: "channelParticipantsAdmins" }, offset: 0, limit: 200, hash: Long.ZERO }));
        if (response._ !== "channels.channelParticipants") throw new Error("telegram.administrators_invalid_response");
        return { community, administrators: projectTelegramAdministrators(response), page_limit: 200 };
      } finally { try { await client.destroy(); } finally { converted.session.authKey.fill(0); for (const key of converted.keys) key.key.fill(0); } }
    });
  });
}
