import { z } from "zod";
import type { AccountContext, AccountContextService } from "../../identity/account-context.ts";
import type { OperationRegistry } from "../../application/operation-registry.ts";
import { parseApplicationJson, stringifyApplicationJson } from "../../contracts/application-json.ts";
import { decryptVaultEnvelope } from "../../storage/vault-reader.ts";
import { OwnerPlatformVaultStore, assertOwnedProfile, encryptOwnerVaultSecret, type OwnerPlatformReadScope, type OwnerPlatformWriteScope } from "./owner-vault-store.ts";
import { platformProfileSecretSchema, type PlatformProfileSecret } from "./platform-vault.ts";
import { probeProxy, proxyConfigSchema } from "./proxy.ts";
import { verifyBrowserLoginIdentity } from "./browser-login-identity.ts";

export function readOwnedOperatorProfile(scope: OwnerPlatformReadScope, id: string) {
  const entry = assertOwnedProfile(scope, id), row = z.object({ id: z.uuid(), platform: z.enum(["x", "instagram", "telegram"]), provider_account_id: z.string(), username: z.string(), encrypted_secret: z.instanceof(Uint8Array), created_at: z.string(), updated_at: z.string() }).parse(scope.vault.prepare("SELECT * FROM profiles WHERE id=?").get(entry.profileId));
  if (row.platform !== entry.platform || row.provider_account_id !== entry.providerAccountId) throw new Error("operator_profile.identity_mismatch");
  const clear = decryptVaultEnvelope(scope.key, row.encrypted_secret); try { const secret = platformProfileSecretSchema.parse(parseApplicationJson(clear)); if (secret.provider_account_id !== entry.providerAccountId) throw new Error("operator_profile.secret_identity_mismatch"); return { entry, row, secret }; } finally { clear.fill(0); }
}
export function replaceOwnedOperatorProfile(scope: OwnerPlatformWriteScope, id: string, expectedRevision: string, secret: PlatformProfileSecret, username?: string) {
  const current = readOwnedOperatorProfile(scope, id); if (current.entry.vaultRevision !== expectedRevision || secret.provider_account_id !== current.entry.providerAccountId) throw new Error("operator_profile.changed");
  const clear = Buffer.from(stringifyApplicationJson(platformProfileSecretSchema.parse(secret))), now = String(Math.floor(Date.now() / 1000));
  try { scope.vault.prepare("UPDATE profiles SET username=?,encrypted_secret=?,updated_at=? WHERE id=?").run(username ?? current.row.username, encryptOwnerVaultSecret(scope.key, clear), now, current.entry.profileId); }
  finally { clear.fill(0); }
  if (username !== undefined) scope.creator.prepare("UPDATE account_refs SET label=?,availability='available',updated_at=? WHERE id=?").run(username, Number(now), id);
  return { id: current.row.id, platform: current.row.platform, provider_account_id: current.row.provider_account_id, username: username ?? current.row.username, created_at: current.row.created_at, updated_at: now };
}
export function registerOperatorProfileOperations(registry: OperationRegistry, accounts: AccountContextService, store: OwnerPlatformVaultStore, telegram: { refreshProfile(context: AccountContext, id: string): Promise<unknown> }, changed: (context: AccountContext) => void) {
  const profile = z.strictObject({ profileId: z.uuid() });
  registry.register("set_profile_proxy", profile.extend({ proxy: proxyConfigSchema }), async (input, context) => {
    const current = await store.read(context, scope => readOwnedOperatorProfile(scope, input.profileId));
    if (current.row.platform === "telegram" && input.proxy.kind !== "socks5") throw new Error("Telegram profiles require a SOCKS5 proxy");
    const geo = await probeProxy(input.proxy, context.signal); accounts.assertCurrent(context);
    await store.mutate(context, scope => replaceOwnedOperatorProfile(scope, input.profileId, current.entry.vaultRevision, { ...current.secret, proxy: input.proxy, proxy_egress: geo.country ? `${geo.ip} · ${geo.country}` : geo.ip })); changed(context); return null;
  });
  registry.register("clear_profile_proxy", profile, async (input, context) => {
    await store.mutate(context, scope => { const current = readOwnedOperatorProfile(scope, input.profileId); if (current.row.platform === "telegram") throw new Error("Telegram profiles require a SOCKS5 proxy and cannot use a direct connection"); return replaceOwnedOperatorProfile(scope, input.profileId, current.entry.vaultRevision, { ...current.secret, proxy: null, proxy_egress: null }); }); changed(context); return null;
  });
  registry.register("refresh_profile", profile, async (input, context) => {
    const current = await store.read(context, scope => readOwnedOperatorProfile(scope, input.profileId));
    if (current.row.platform === "telegram") return telegram.refreshProfile(context, input.profileId);
    const geo = current.secret.proxy ? await probeProxy(proxyConfigSchema.parse(current.secret.proxy), context.signal) : { timezone: null, accept_language: null, ip: "", country: "" };
    const verified = await verifyBrowserLoginIdentity(current.row.platform, current.secret, geo, context.signal, async () => accounts.assertCurrent(context), true);
    if (verified.providerAccountId !== current.entry.providerAccountId) throw new Error("operator_profile.remote_identity_mismatch");
    const result = await store.mutate(context, scope => replaceOwnedOperatorProfile(scope, input.profileId, current.entry.vaultRevision, { ...verified.secret, proxy_egress: geo.ip ? geo.country ? `${geo.ip} · ${geo.country}` : geo.ip : current.secret.proxy_egress }, verified.username)); changed(context); return result;
  });
}
