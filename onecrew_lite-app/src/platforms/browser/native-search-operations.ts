import type { OperationRegistry } from "../../application/operation-registry.ts";
import type { OwnerResults } from "../../application/owner-results.ts";
import type { AccountContextService } from "../../identity/account-context.ts";
import { searchPlanScope } from "../../discovery/search-plan-contracts.ts";
import { nativePlatformSearchCatalog } from "../../discovery/native-search-contracts.ts";
import { BoundSearchAuthorization } from "../../results/search-plan.ts";
import type { OwnedPlatformAccountResolver } from "./owned-accounts.ts";
import { readNativeYoutubeSearch } from "./native-youtube-search.ts";
import { readNativeInstagramUsers } from "./native-instagram-search.ts";
import { readNativeTelegramCommunities } from "./native-telegram-search.ts";
import { readNativeXAdvancedSearch } from "./native-x-search.ts";
import { z } from "zod";

export function registerNativeSearchOperations(registry: OperationRegistry, accounts: AccountContextService, owners: OwnerResults, platforms: OwnedPlatformAccountResolver): void {
  for (const entry of nativePlatformSearchCatalog()) registry.register(entry.operation_id, searchPlanScope.refine(value => value.operation_id === entry.operation_id), async (rawScope, context) => {
    const scope = searchPlanScope.parse(structuredClone(rawScope));
    const bound = owners.authorizeNativeSearch(context, scope);
    const validate = () => { accounts.assertCurrent(context); BoundSearchAuthorization.prototype.validateRequest.call(bound.authorization, bound.operationType, bound.request); context.signal.throwIfAborted(); };
    validate();
    if (scope.connection.kind === "service_credential") {
      if (scope.operation_id !== "youtube.channels.search" && scope.operation_id !== "youtube.videos.search") throw new Error("search_plan_connection_invalid");
      return platforms.withYoutubeCredential(context, scope.connection.credential_id, async (_credential, key, assertOwned, request) => {
        const current = async () => { validate(); await assertOwned(); validate(); };
        await current(); return readNativeYoutubeSearch(key, scope.operation_id === "youtube.channels.search" ? "channel" : "video", scope.arguments, context.signal, current, request);
      });
    }
    if (scope.connection.kind !== "operator_account") throw new Error("search_plan_connection_invalid");
    const platform = z.enum(["x", "instagram", "telegram"]).parse(scope.connection.toolkit);
    return platforms.withProfile(context, scope.connection.operator_account_id, platform, async (profile, secret, assertOwned) => {
      const current = async () => { validate(); await assertOwned(); validate(); }; await current();
      if (scope.operation_id === "instagram.users.search" && platform === "instagram") return readNativeInstagramUsers(secret, scope.arguments, context.signal, current);
      if (scope.operation_id === "telegram.communities.search" && platform === "telegram") return readNativeTelegramCommunities(secret, profile.provider_account_id, scope.arguments, context.signal, current);
      if (scope.operation_id === "x.posts.search.advanced" && platform === "x") return readNativeXAdvancedSearch(secret, scope.arguments, context.signal, current);
      throw new Error("search_plan_connection_invalid");
    });
  });
}
