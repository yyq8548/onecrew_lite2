import { z } from "zod";
import type { OperationRegistry } from "../application/operation-registry.ts";
import type { AccountContext, AccountContextService } from "../identity/account-context.ts";
import type { CreatorRepositoryOwner } from "../creators/operations.ts";
import { identityGetInput, identityGroupsInput, identityPairInput, identityRevokeInput, identitySubmitInput } from "./identity-contracts.ts";
import type { DiscoveryIdentityRepository } from "./identity-repository.ts";

export interface DiscoveryIdentityRepositoryResolver { resolve(context: AccountContext): Promise<Readonly<{ owner: CreatorRepositoryOwner; repository: DiscoveryIdentityRepository }>> }
export function registerDiscoveryIdentityOperations(registry: OperationRegistry, resolver: DiscoveryIdentityRepositoryResolver, accounts: AccountContextService): void {
  async function execute<T>(context: AccountContext, operation: (repository: DiscoveryIdentityRepository) => T): Promise<T> {
    accounts.assertCurrent(context);
    const resolved = await resolver.resolve(context);
    accounts.assertCurrent(context);
    const expected = { accountId: context.principal.user.id, tenantId: context.principal.tenantId, baseId: context.principal.baseId };
    for (const field of ["accountId", "tenantId", "baseId"] as const) if (resolved.owner[field] !== expected[field]) throw new Error("Identity repository does not belong to the verified account");
    const result = operation(resolved.repository);
    accounts.assertCurrent(context);
    return result;
  }
  registry.register("onecrew_discovery_identity_material_get", z.object({ input: identityPairInput }).strict(), async ({ input }, context) => execute(context, repository => repository.material(input)));
  registry.register("onecrew_discovery_identity_review_submit", z.object({ input: identitySubmitInput }).strict(), async ({ input }, context) => execute(context, repository => repository.submit(context.principal.user.id, input)));
  registry.register("onecrew_discovery_identity_review_get", z.object({ input: identityGetInput }).strict(), async ({ input }, context) => execute(context, repository => repository.get(context.principal.user.id, input)));
  registry.register("onecrew_discovery_identity_review_revoke", z.object({ input: identityRevokeInput }).strict(), async ({ input }, context) => execute(context, repository => repository.revoke(context.principal.user.id, input)));
  registry.register("onecrew_discovery_identity_groups_get", z.object({ input: identityGroupsInput }).strict(), async ({ input }, context) => execute(context, repository => repository.groups(input)));
}
