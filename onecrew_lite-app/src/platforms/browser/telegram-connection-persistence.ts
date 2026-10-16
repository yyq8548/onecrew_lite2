import { createHash, randomUUID } from "node:crypto";
import { z } from "zod";
import { stringifyApplicationJson } from "../../contracts/application-json.ts";
import { encryptOwnerVaultSecret, type OwnerPlatformWriteScope } from "./owner-vault-store.ts";
import type { TelegramConnectionStart } from "./telegram-connection-contracts.ts";

export const telegramConnectedIdentity = z.strictObject({ id: z.string().refine(value => !!value.trim()), username: z.string().nullable(), first_name: z.string().nullable(), last_name: z.string().nullable(), is_bot: z.boolean(), verified: z.boolean() });
export type TelegramConnectedIdentity = z.infer<typeof telegramConnectedIdentity>;
// 已通过MTProto getMe验证的结果在共同Vault/Creator事务内提交，不认领其他账号的旧全局记录。
export function persistTelegramConnection(scope: OwnerPlatformWriteScope, input: TelegramConnectionStart, account: TelegramConnectedIdentity, session: string, label: string, expected?: { accountRefId: string; providerAccountId: string; proxyEgress?: string | null }) {
  if (!scope.creator.isTransaction || !scope.vault.isTransaction) throw new Error("telegram.persistence_requires_shared_transaction");
  telegramConnectedIdentity.parse(account); if (!session) throw new Error("telegram.session_unavailable");
  if (expected && expected.providerAccountId !== account.id) throw new Error("telegram.operator_identity_mismatch");
  const mapping = scope.manifest.profiles.find(record => record.platform === "telegram" && record.providerAccountId === account.id);
  if (expected && mapping?.accountRefId !== expected.accountRefId) throw new Error("telegram.owner_mapping_changed");
  const previous = mapping ? scope.vault.prepare("SELECT id,platform,provider_account_id,created_at,updated_at FROM profiles WHERE id=?").get(mapping.profileId) : undefined;
  if (mapping && (!previous || previous.platform !== "telegram" || previous.provider_account_id !== account.id)) throw new Error("telegram.profile_identity_mismatch");
  const id = mapping?.profileId ?? randomUUID(), username = account.username ?? account.id, now = Math.floor(Date.now() / 1000), updated = String(now);
  const secret = { provider_account_id: account.id, cookies: [], x_chat_passcode: null, instagram_session_settings: null, telegram_api_id: input.api_id, telegram_api_hash: input.api_hash, telegram_session: stringifyApplicationJson({ format: "onecrew-mtcute-v1", session }), profile_name: label, browser_profile_dir: "", fingerprint_seed: "", proxy: input.proxy, proxy_egress: expected?.proxyEgress ?? null };
  const clear = Buffer.from(stringifyApplicationJson(secret)); let encrypted: Buffer;
  try { encrypted = encryptOwnerVaultSecret(scope.key, clear); } finally { clear.fill(0); }
  scope.vault.prepare("INSERT INTO profiles(id,platform,provider_account_id,username,encrypted_secret,created_at,updated_at) VALUES(?,'telegram',?,?,?,?,?) ON CONFLICT(id) DO UPDATE SET username=excluded.username,encrypted_secret=excluded.encrypted_secret,updated_at=excluded.updated_at").run(id, account.id, username, encrypted, String(previous?.created_at ?? now), updated);
  const accountRefId = mapping?.accountRefId ?? id;
  const reference = scope.creator.prepare("SELECT platform,kind FROM account_refs WHERE id=?").get(accountRefId);
  if (reference && (reference.platform !== "telegram" || reference.kind !== "profile")) throw new Error("telegram.creator_identity_mismatch");
  scope.creator.prepare("INSERT INTO account_refs(id,platform,kind,label,active,availability,created_at,updated_at) VALUES(?,'telegram','profile',?,1,'available',?,?) ON CONFLICT(id) DO UPDATE SET label=excluded.label,availability=excluded.availability,updated_at=excluded.updated_at").run(accountRefId, username.trim(), now, now);
  if (!mapping) scope.manifest.profiles.push({ accountRefId, profileId: id, platform: "telegram", providerAccountId: account.id, vaultRevision: createHash("sha256").update(encrypted).digest("hex") });
  return { operator_account_id: accountRefId, platform: "telegram" as const, provider_account_id: account.id, username, label, configured: true, credential_availability: "available", x_chat_passcode_status: "not_applicable", updated_at: updated };
}
