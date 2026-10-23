import { TelegramClient, MemoryStorage, proxyTransportFromUrl, tl } from "@mtcute/node";
import { telegramSearchInput } from "../../discovery/native-search-inputs.ts";
import { importGrammersSession } from "../candidates/telegram-read.ts";
import { nativeProxy } from "../candidates/social-read.ts";
import type { PlatformProfileSecret } from "./platform-vault.ts";

export function projectNativeTelegramChats(chats: readonly tl.TypeChat[]) {
  const result: { id: string; title: string; username?: string; kind: string; participant_count?: number; verified: boolean; scam: boolean }[] = [];
  for (const chat of chats) {
    if (chat._ !== "channel" && chat._ !== "chat") continue;
    if (!Number.isSafeInteger(chat.id)) throw new Error("telegram.chat_id_precision_invalid");
    if (chat._ === "channel") result.push({ id: String(chat.id), title: chat.title, ...(chat.username === undefined ? {} : { username: chat.username }), kind: chat.broadcast ? "channel" : chat.megagroup ? "supergroup" : "group", ...(chat.participantsCount === undefined ? {} : { participant_count: chat.participantsCount }), verified: chat.verified ?? false, scam: (chat.scam ?? false) || (chat.fake ?? false) });
    else result.push({ id: String(chat.id), title: chat.title, kind: "group", participant_count: chat.participantsCount, verified: false, scam: false });
  }
  return result;
}
export async function readNativeTelegramCommunities(secret: PlatformProfileSecret, providerAccountId: string, arguments_: unknown, signal: AbortSignal, assertCurrent: () => Promise<void>) {
  const input = telegramSearchInput.parse(arguments_), proxy = nativeProxy(secret.proxy);
  if (!proxy?.startsWith("socks5://") || !secret.telegram_api_id || !secret.telegram_api_hash || !secret.telegram_session) throw new Error("telegram.session_or_socks5_required");
  const converted = importGrammersSession(secret.telegram_session), storage = new MemoryStorage();
  const client = new TelegramClient({ apiId: secret.telegram_api_id, apiHash: secret.telegram_api_hash, storage, transport: proxyTransportFromUrl(proxy), disableUpdates: true, logLevel: 0, fallbackDcs: [] });
  const scoped = client.withParams({ abortSignal: signal, timeout: 30_000, maxRetryCount: 0, floodSleepThreshold: 0 });
  try {
    signal.throwIfAborted(); await assertCurrent();
    await client.importSession(converted.session); for (const key of converted.keys) storage.authKeys.set(key.id, key.key);
    const me = await scoped.getMe(); if (String(me.id) !== providerAccountId) throw new Error("telegram.operator_identity_mismatch");
    await assertCurrent(); signal.throwIfAborted();
    const found = await scoped.call({ _: "contacts.search", q: input.query, limit: input.count, bots: false, broadcasts: false });
    await assertCurrent(); signal.throwIfAborted(); return projectNativeTelegramChats(found.chats);
  } finally { try { await client.destroy(); } finally { converted.session.authKey.fill(0); for (const key of converted.keys) key.key.fill(0); } }
}
