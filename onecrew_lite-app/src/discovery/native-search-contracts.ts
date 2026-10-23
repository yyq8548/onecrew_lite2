import { readFileSync } from "node:fs";
import { runtimeAssetPath } from "../packaging/runtime-context.ts";
import { z } from "zod";
import { parseApplicationJson } from "../contracts/application-json.ts";
import { searchPlanScope, searchScopeApprovalToken } from "./search-plan-contracts.ts";
import { youtubeSearchInput, instagramSearchInput, telegramSearchInput, advancedXSearchInput, compileAdvancedXQuery } from "./native-search-inputs.ts";

const schemas = z.record(z.string(), z.record(z.string(), z.unknown())).parse(parseApplicationJson(readFileSync(runtimeAssetPath("discovery/native-search-schemas.json", new URL("./native-search-schemas.json", import.meta.url)))));
// GitHub 0.7.1原生产registry的cfg(test)导出；该版本值包含原schema描述，不能由请求指定替换。
const definitions = {
  "youtube.channels.search": { toolkit: "youtube", kind: "service_credential", revision: "sha256:2e668efba3de5dd30b262658b5f254aa72f4f40e54dd8c19f0fcab69c7b95306", operationType: Symbol("youtube.channels.search") },
  "youtube.videos.search": { toolkit: "youtube", kind: "service_credential", revision: "sha256:2e668efba3de5dd30b262658b5f254aa72f4f40e54dd8c19f0fcab69c7b95306", operationType: Symbol("youtube.videos.search") },
  "instagram.users.search": { toolkit: "instagram", kind: "operator_account", revision: "sha256:621e690f1a0ccfb1106e9caf0625a9f410549b6f549e9326b90d9c5d6ed3e06f", operationType: Symbol("instagram.users.search") },
  "telegram.communities.search": { toolkit: "telegram", kind: "operator_account", revision: "sha256:6fafa5a54e6dfc5d0a36225a6818e2d7df770e45b190cb65eeb48b1c13b75486", operationType: Symbol("telegram.communities.search") },
  "x.posts.search.advanced": { toolkit: "x", kind: "operator_account", revision: "sha256:9d59e33ec27aa15cd9892a2c6ff9645e8abfbe62e514c02780614e00b39068c5", operationType: Symbol("x.posts.search.advanced") },
} as const;
export function nativePlatformSearchCatalog() { return Object.entries(definitions).map(([operation_id, definition]) => ({ operation_id, schema_revision: definition.revision, connection: { kind: definition.kind, toolkit: definition.toolkit }, input_schema: structuredClone(schemas[definition.toolkit]!) })); }
export interface CompiledNativePlatformSearch { readonly token: string; readonly operationType: symbol; readonly request: unknown }
export function compileNativePlatformSearch(raw: unknown, purpose: "initial" | "execute"): CompiledNativePlatformSearch {
  const scope = searchPlanScope.parse(raw);
  if (!Object.hasOwn(definitions, scope.operation_id)) throw new Error("search_plan_unknown_operation");
  const definition = definitions[scope.operation_id as keyof typeof definitions];
  if (scope.schema_revision !== definition.revision) throw new Error("search_plan_schema_revision_mismatch");
  if (scope.connection.kind !== definition.kind || scope.connection.toolkit !== definition.toolkit) throw new Error("search_plan_connection_invalid");
  if (purpose === "initial") {
    if (scope.arguments.confirmed_plan_ref !== undefined && scope.arguments.confirmed_plan_ref !== null) throw new Error("search_plan_reference_in_scope");
    if (scope.arguments.search_plan_policy !== undefined && scope.arguments.search_plan_policy !== "require_confirmation") throw new Error("search_plan_direct_policy_in_scope");
  }
  let args: Record<string, unknown>; let request: unknown;
  if (definition.toolkit === "youtube") {
    const input = youtubeSearchInput.parse(scope.arguments); args = { ...input };
    if (scope.connection.kind !== "service_credential") throw new Error("search_plan_connection_invalid");
    request = { credential_id: scope.connection.credential_id, query: input.query, count: input.count, order: input.order, published_after: input.published_after, cursor: input.cursor, region_code: input.region_code, language: input.language };
  } else {
    if (scope.connection.kind !== "operator_account") throw new Error("search_plan_connection_invalid");
    const operator_account_id = scope.connection.operator_account_id;
    if (definition.toolkit === "x") {
      const input = advancedXSearchInput.parse(scope.arguments); compileAdvancedXQuery(input); args = { ...input };
      const { search_plan_policy: _policy, confirmed_plan_ref: _reference, result_mode, count, cursor, ...criteria } = input;
      request = { operator_account_id, criteria, result_mode, count, cursor };
    } else {
      const input = (definition.toolkit === "instagram" ? instagramSearchInput : telegramSearchInput).parse(scope.arguments); args = { ...input };
      request = { operator_account_id, query: input.query, count: input.count };
    }
  }
  if (purpose === "initial" && args.cursor !== undefined && args.cursor !== null) throw new Error("search_plan_continuation_in_initial_plan");
  if (args.search_plan_policy === "user_requested_direct" && args.confirmed_plan_ref !== null) throw new Error("search_plan_scope_mismatch");
  delete args.cursor; delete args.search_plan_policy; delete args.confirmed_plan_ref;
  return { token: searchScopeApprovalToken({ ...scope, arguments: args }), operationType: definition.operationType, request };
}
