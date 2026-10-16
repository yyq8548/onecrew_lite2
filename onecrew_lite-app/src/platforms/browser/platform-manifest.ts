import { z } from "zod";
const owner = z.strictObject({ accountId: z.string().min(1), tenantId: z.string().min(1), baseId: z.string().min(1) });
export const platformAccountOwnerSchema = owner;
const revision = z.string().regex(/^[a-f\d]{64}$/u);
const fields = { owner, deviceId: z.string().min(1), profiles: z.array(z.strictObject({ accountRefId: z.uuid(), profileId: z.uuid(), platform: z.enum(["x", "instagram", "telegram"]), providerAccountId: z.string().min(1), vaultRevision: revision })).max(1000), services: z.array(z.strictObject({ accountRefId: z.uuid(), credentialId: z.uuid(), provider: z.literal("youtube"), vaultRevision: revision })).max(1000) };
export const accountVaultDirectory = z.string().refine(value => value.startsWith("platform-vault-") && z.uuid().safeParse(value.slice("platform-vault-".length)).success);
export const platformAccountManifestSchema = z.discriminatedUnion("format", [z.strictObject({ format: z.literal(1), ...fields }), z.strictObject({ format: z.literal(2), ...fields, storage: z.strictObject({ kind: z.literal("account_vault"), directory: accountVaultDirectory }) })]).refine(value => { const ids = [...value.profiles, ...value.services].map(record => record.accountRefId); return new Set(ids).size === ids.length && new Set(value.profiles.map(record => record.profileId)).size === value.profiles.length && new Set(value.services.map(record => record.credentialId)).size === value.services.length; });
export type PlatformAccountManifest = z.infer<typeof platformAccountManifestSchema>;
export type OwnerPlatformManifest = Extract<PlatformAccountManifest, { format: 2 }>;
