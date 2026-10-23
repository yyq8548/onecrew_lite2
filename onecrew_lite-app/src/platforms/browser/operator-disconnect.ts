import { z } from "zod";
import type { OperationRegistry } from "../../application/operation-registry.ts";
import type { AccountContext, AccountContextService } from "../../identity/account-context.ts";
import { OwnerPlatformVaultStore, assertOwnedProfile, type OwnerPlatformReadScope } from "./owner-vault-store.ts";
import { OperatorDisconnectRepository } from "./operator-disconnect-repository.ts";

export const operatorDisconnectInput = z.strictObject({ operator_account_id: z.string(), confirmed: z.boolean(), idempotency_key: z.string() });
function resolve(scope: OwnerPlatformReadScope, id: string, expectedPlatform?: string) {
  const repository = new OperatorDisconnectRepository(scope.creator), reference = repository.account(id);
  if (reference.kind !== "profile" || !["x", "instagram", "telegram"].includes(reference.platform)) throw new Error("operator account was not found");
  if (expectedPlatform !== undefined && reference.platform !== expectedPlatform) throw new Error("operator account platform does not match the requested operation");
  const entry = scope.manifest.profiles.find(value => value.accountRefId === id);
  if (entry) {
    assertOwnedProfile(scope, id);
    const value = z.object({ username: z.string(), platform: z.string(), provider_account_id: z.string() }).parse(scope.vault.prepare("SELECT username,platform,provider_account_id FROM profiles WHERE id=?").get(entry.profileId));
    if (value.platform !== reference.platform || value.provider_account_id !== entry.providerAccountId || value.username.replace(/[A-Z]/gu, character => character.toLowerCase()) !== reference.label.replace(/^@+/u, "").replace(/[A-Z]/gu, character => character.toLowerCase())) throw new Error("operator account was not found");
  } else if (!scope.creator.prepare("SELECT key FROM idempotency_records WHERE action='operator_account_disconnect' AND request_fingerprint=?").get(id)) throw new Error("platform_store.owner_mapping_required");
  return repository;
}

export class OperatorDisconnect {
  readonly #accounts: AccountContextService; readonly #store: OwnerPlatformVaultStore; readonly #changed: ((context: AccountContext) => void) | undefined;
  constructor(accounts: AccountContextService, store: OwnerPlatformVaultStore, changed?: (context: AccountContext) => void) { this.#accounts = accounts; this.#store = store; this.#changed = changed; }
  async disconnect(context: AccountContext, raw: unknown, expectedPlatform?: "x" | "instagram" | "telegram") {
    this.#accounts.assertCurrent(context); const input = operatorDisconnectInput.parse(raw);
    if (!input.confirmed) throw new Error("confirmed=true is required");
    const id = z.uuid().parse(input.operator_account_id), key = input.idempotency_key.trim();
    if (!key || Buffer.byteLength(key) > 128) throw new Error("idempotency_key must contain 1 to 128 characters");
    const claim = await this.#store.mutate(context, scope => resolve(scope, id, expectedPlatform).begin(key, id));
    if (claim.state === "completed") return { operator_account_id: id, replayed: true };
    const completion = await Promise.resolve().then(async () => {
      if (claim.state === "acquired") await this.#store.mutate(context, scope => resolve(scope, id, expectedPlatform).markStarted(key, id));
      if (claim.state !== "remote_verified") {
        await this.#store.mutate(context, scope => {
          const repository = resolve(scope, id, expectedPlatform), entry = scope.manifest.profiles.find(value => value.accountRefId === id);
          if (entry) { scope.vault.prepare("DELETE FROM profiles WHERE id=?").run(entry.profileId); scope.manifest.profiles = scope.manifest.profiles.filter(value => value.accountRefId !== id); }
          repository.markVerified(key, id);
        });
      }
      return this.#store.mutate(context, scope => resolve(scope, id, expectedPlatform).complete(key, id));
    }).then(result => ({ ok: true as const, result }), error => ({ ok: false as const, error }));
    this.#changed?.(context);
    if (!completion.ok) throw new Error("operator disconnect is incomplete; retry the same intent with the same idempotency_key", { cause: completion.error });
    return { operator_account_id: id, replayed: completion.result.replayed };
  }
  async recover(context: AccountContext): Promise<void> {
    const pending = await this.#store.readCreator(context, creator => new OperatorDisconnectRepository(creator).pending());
    for (const value of pending) await this.disconnect(context, { operator_account_id: value.account_ref_id, confirmed: true, idempotency_key: value.key });
  }
}
export function registerOperatorDisconnectOperations(registry: OperationRegistry, accounts: AccountContextService, store: OwnerPlatformVaultStore, onChanged?: (context: AccountContext) => void): OperatorDisconnect {
  const operation = new OperatorDisconnect(accounts, store, onChanged);
  registry.register("onecrew_operator_account_disconnect", z.strictObject({ input: operatorDisconnectInput }), async ({ input }, context) => operation.disconnect(context, input));
  for (const platform of ["x", "instagram", "telegram"] as const) registry.register(`${platform}.connection.disconnect`, operatorDisconnectInput, async (input, context) => operation.disconnect(context, input, platform));
  return operation;
}
