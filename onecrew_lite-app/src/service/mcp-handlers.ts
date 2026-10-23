import { z } from "zod";
import type { OperationRegistry } from "../application/operation-registry.ts";
import type { AccountContext } from "../identity/account-context.ts";
import { assertOperationAvailable } from "../contracts/retired-operations.ts";
import type { McpExecutionCall } from "./mcp-contracts.ts";
import { nativePlatformSearchCatalog } from "../discovery/native-search-contracts.ts";
import { publicReadOperationIds } from "../platforms/public-read-operations.ts";
import { telegramPublicReadOperationIds } from "../platforms/browser/telegram-public-operations.ts";
import { connectionOperationIds } from "../platforms/connection-operations.ts";
import { TELEGRAM_CONNECTION_OPERATION_IDS } from "../platforms/browser/telegram-connection-operations.ts";
import { browserConnectionOperationIds } from "../platforms/browser/operator-connection-operations.ts";
import { discoveryReadOperationIds } from "../platforms/discovery-read-operations.ts";

const creatorId = z.strictObject({ creator_id: z.string() });
const campaignId = z.strictObject({ campaign_id: z.string() });
const campaignApproval = z.strictObject({ campaign_id: z.string(), expected_revision: z.number().int().min(1).max(4294967295) });
const discoverySearch = z.strictObject({ campaign_id: z.string(), expected_revision: z.number().int().min(0).max(4294967295), profile_ref: z.string(), limit: z.number().int().min(0).max(255), recovery: z.strictObject({ expected_stop_code: z.string().min(1), expected_stopped_at: z.number().int().safe() }).nullish() });
const discoveryPlatformSearch = discoverySearch.extend({ platform: z.string() });
const discoveryVideo = discoverySearch.omit({ limit: true, recovery: true }).extend({ search_hit_id: z.string() });
const input = (arguments_: Record<string, unknown>): unknown => ({ input: arguments_ });
const pageInput = (arguments_: Record<string, unknown>): unknown => ({ input: { ...arguments_, after_id: arguments_.after_id ?? null } });
const direct = (arguments_: Record<string, unknown>): unknown => arguments_;
const mappings = [
  ["onecrew.repair.get", "onecrew_repair_get", direct],
  ["onecrew.repair.apply", "onecrew_repair_apply", direct],
  ["onecrew.repair.finish", "onecrew_repair_finish", direct],
  ["onecrew.outreach.action.status", "onecrew.outreach.action.status", direct],
  ["onecrew.outreach.contact_route.last_contact_account", "onecrew.outreach.contact_route.last_contact_account", direct],
  ["onecrew.outreach.conversation.get", "onecrew.outreach.conversation.get", direct],
  ["onecrew.outreach.conversation.list", "onecrew.outreach.conversation.list", direct],
  ["onecrew.triage.list", "onecrew.triage.list", direct],
  ["onecrew.triage.record", "onecrew.triage.record", direct],
  ["onecrew.triage.acknowledge", "onecrew.triage.acknowledge", direct],
  ["onecrew.creator.archive", "onecrew_creator_archive", (arguments_: Record<string, unknown>) => ({ creatorId: creatorId.parse(arguments_).creator_id })],
  ["onecrew.creator.contact_route.add", "onecrew_creator_contact_route_add", input],
  ["onecrew.creator.contact_route.delete.confirm", "onecrew_creator_contact_route_delete_confirm", input],
  ["onecrew.creator.contact_route.delete.prepare", "onecrew_creator_contact_route_delete_prepare", input],
  ["onecrew.creator.contact_route.update", "onecrew_creator_contact_route_update", input],
  ["onecrew.creator.delete", "onecrew_creator_delete", input],
  ["onecrew.creator.export", "onecrew.creator.export", direct],
  ["onecrew.creator.get", "onecrew_creator_get", input],
  ["onecrew.creator.group.bulk_move", "onecrew_creator_group_bulk_move", input],
  ["onecrew.creator.group.create", "onecrew_creator_group_create", input],
  ["onecrew.creator.group.delete", "onecrew_creator_group_delete", input],
  ["onecrew.creator.group.list", "onecrew_creator_group_list", input],
  ["onecrew.creator.group.set", "onecrew_creator_group_set", input],
  ["onecrew.creator.group.update", "onecrew_creator_group_update", input],
  ["onecrew.creator.identity.resolve", "onecrew.creator.identity.resolve", direct],
  ["onecrew.creator.list", "onecrew_creator_list", input],
  ["onecrew.creator.manual.create", "onecrew_creator_manual_create", input],
  ["onecrew.creator.stage.update", "onecrew_creator_update_stage", input],
  ["onecrew.creator.update", "onecrew_creator_update", input],
  ["email.draft.get", "onecrew_email_draft_get", input],
  ["email.draft.list", "onecrew_email_draft_list", input],
  ["email.folder.list", "onecrew_email_folder_list", input],
  ["email.mailbox.get", "onecrew_email_mailbox_get", input],
  ["email.mailbox.sync", "onecrew_email_mailbox_sync", input],
  ["email.mailbox.sync_status", "onecrew_email_mailbox_sync_status", input],
  ["email.message.get", "onecrew_email_message_get", input],
  ["email.template.get", "onecrew_email_template_get", input],
  ["email.template.list", "onecrew_email_template_list", input],
  ["email.thread.get", "onecrew_email_thread_get", input],
  ["email.thread.list", "onecrew_email_thread_list", input],
  ["email.thread.update_disposition", "onecrew_email_thread_update_disposition", input],
  ["youtube.credential.connect", "connect_youtube_api_key", input],
  ["youtube.credential.rotate", "rotate_youtube_api_key", input],
  ["youtube.credential.rename", "rename_youtube_api_key", input],
  ["youtube.credential.delete", "delete_youtube_api_key", (arguments_: Record<string, unknown>) => { const value = z.strictObject({ credential_id: z.string(), confirmed: z.boolean(), idempotency_key: z.string() }).parse(arguments_); return { credentialId: value.credential_id, confirmed: value.confirmed, idempotencyKey: value.idempotency_key }; }],
  ["onecrew.discovery.campaign.get", "onecrew_discovery_campaign_get", (arguments_: Record<string, unknown>) => ({ id: campaignId.parse(arguments_).campaign_id })],
  ["onecrew.discovery.advance", "onecrew_discovery_advance", input],
  ["onecrew.discovery.host_work.claim", "onecrew_discovery_host_work_claim", input],
  ["onecrew.discovery.execution.start", "onecrew_discovery_execution_start", input],
  ["onecrew.discovery.execution.get", "onecrew_discovery_execution_get", () => ({})],
  ["onecrew.discovery.host_work.get", "onecrew_discovery_host_work_get", input],
  ["onecrew.discovery.host_work.submit", "onecrew_discovery_host_work_submit", input],
  ["onecrew.discovery.host_operation.get", "onecrew_discovery_host_operation_get", input],
  ["onecrew.discovery.queries.expand", "onecrew_discovery_queries_expand", input],
  ["onecrew.discovery.source_targets.set", "onecrew_discovery_source_targets_set", input],
  ["onecrew.discovery.web_author.resolve", "onecrew_discovery_web_author_resolve", input],
  ["onecrew.discovery.identity.get", "onecrew_discovery_identity_get", input],
  ["onecrew.discovery.candidates.count", "onecrew_discovery_candidates_count", (arguments_: Record<string, unknown>) => ({ id: campaignId.parse(arguments_).campaign_id })],
  ["onecrew.discovery.candidates.list", "onecrew_discovery_candidates_list", pageInput],
  ["onecrew.discovery.hits.list", "onecrew_discovery_search_hits_list", pageInput],
  ["onecrew.discovery.review_material.get", "onecrew_discovery_review_material_get", input],
  ["onecrew.discovery.review.get", "onecrew_discovery_review_get", input],
  ["onecrew.discovery.profile.get", "onecrew_discovery_profile_get", input],
  ["onecrew.discovery.content.get", "onecrew_discovery_content_get", input],
  ["onecrew.discovery.transcript.get", "onecrew_discovery_transcript_get", input],
  ["onecrew.discovery.public_page.get", "onecrew_discovery_public_page_get", input],
  ["onecrew.discovery.youtube.search_once", "onecrew_discovery_youtube_search_once", (arguments_: Record<string, unknown>) => { const value = discoverySearch.parse(arguments_); return { id: value.campaign_id, expectedRevision: value.expected_revision, profileRef: value.profile_ref, limit: value.limit, ...(value.recovery === undefined ? {} : { recovery: value.recovery }) }; }],
  ["onecrew.discovery.search_once", "onecrew_discovery_search_once", (arguments_: Record<string, unknown>) => { const value = discoveryPlatformSearch.parse(arguments_); return { id: value.campaign_id, expectedRevision: value.expected_revision, profileRef: value.profile_ref, platform: value.platform, limit: value.limit, ...(value.recovery === undefined ? {} : { recovery: value.recovery }) }; }],
  ["onecrew.discovery.youtube.verify_hit_once", "onecrew_discovery_youtube_verify_hit_once", (arguments_: Record<string, unknown>) => { const value = discoveryVideo.parse(arguments_); return { id: value.campaign_id, expectedRevision: value.expected_revision, profileRef: value.profile_ref, searchHitId: value.search_hit_id }; }],
  ["onecrew.discovery.identity.material.get", "onecrew_discovery_identity_material_get", input],
  ["onecrew.discovery.identity.review.submit", "onecrew_discovery_identity_review_submit", input],
  ["onecrew.discovery.identity.review.get", "onecrew_discovery_identity_review_get", input],
  ["onecrew.discovery.identity.review.revoke", "onecrew_discovery_identity_review_revoke", input],
  ["onecrew.discovery.identity.groups.get", "onecrew_discovery_identity_groups_get", input],
  ["onecrew.discovery.campaign.create", "onecrew_discovery_campaign_create", input],
  ["onecrew.discovery.campaign.approve", "onecrew_discovery_campaign_approve", (arguments_: Record<string, unknown>) => { const value = campaignApproval.parse(arguments_); return { id: value.campaign_id, expectedRevision: value.expected_revision }; }],
  ["onecrew.discovery.target.increase", "onecrew_discovery_target_increase", input],
  ["onecrew.discovery.review.submit", "onecrew_discovery_review_submit", input],
] as const;

export interface McpRegisteredHandler {
  readonly operation_id: string;
  readonly validatesConnection?: true;
  readonly requiresSearchPlan?: true;
  validate(call: McpExecutionCall): void;
  invoke(call: McpExecutionCall, account: AccountContext): Promise<unknown>;
}
export function registeredLocalMcpHandlers(registry: OperationRegistry): readonly McpRegisteredHandler[] {
  const registered = new Set(registry.names());
  const localMappings = [...mappings, ...discoveryReadOperationIds.map(id => [id, id, direct] as const)];
  const local = localMappings.filter(([, command]) => registered.has(command)).map(([operation_id, command, wrap]) => {
    assertOperationAvailable(operation_id);
    return Object.freeze({ operation_id,
      validate(call: McpExecutionCall) {
        if (call.operation_id !== operation_id) throw new Error("mcp_handler_operation_mismatch");
        registry.validate(command, wrap(call.arguments));
      },
      async invoke(call: McpExecutionCall, account: AccountContext) {
        if (call.operation_id !== operation_id) throw new Error("mcp_handler_operation_mismatch");
        return registry.execute(command, wrap(call.arguments), account.principal.user.id, account.epoch);
      },
    });
  });
  const native = [...nativePlatformSearchCatalog().map(entry => ({ operation_id: entry.operation_id, requiresSearchPlan: true })), ...[...publicReadOperationIds, ...telegramPublicReadOperationIds, ...connectionOperationIds].map(operation_id => ({ operation_id, requiresSearchPlan: false }))].filter(entry => registered.has(entry.operation_id)).map(entry => {
    const scope = (call: McpExecutionCall) => {
      if (call.operation_id !== entry.operation_id) throw new Error("mcp_handler_operation_mismatch");
      return { operation_id: call.operation_id, schema_revision: call.schema_revision, connection: call.connection, arguments: call.arguments };
    };
    return Object.freeze({ operation_id: entry.operation_id, validatesConnection: true as const, ...(entry.requiresSearchPlan ? { requiresSearchPlan: true as const } : {}),
      validate(call: McpExecutionCall) { registry.validate(entry.operation_id, scope(call)); },
      invoke(call: McpExecutionCall, account: AccountContext) { return registry.execute(entry.operation_id, scope(call), account.principal.user.id, account.epoch); },
    });
  });
  const lifecycleIds = [...browserConnectionOperationIds, ...TELEGRAM_CONNECTION_OPERATION_IDS, ...["x", "instagram", "telegram"].map(platform => `${platform}.connection.disconnect`)];
  const lifecycle = lifecycleIds.filter(id => registered.has(id)).map(operation_id => Object.freeze({ operation_id,
    validate(call: McpExecutionCall) {
      if (call.operation_id !== operation_id || call.connection !== null) throw new Error("mcp_handler_connection_mismatch");
      registry.validate(operation_id, call.arguments);
    },
    invoke(call: McpExecutionCall, account: AccountContext) { return registry.execute(operation_id, call.arguments, account.principal.user.id, account.epoch); },
  }));
  return [...local, ...native, ...lifecycle];
}
