import { z } from "zod";
import type { OperationRegistry } from "../application/operation-registry.ts";
import type { AccountContext, AccountContextService } from "../identity/account-context.ts";
import type { CreatorRepositoryOwner } from "./operations.ts";
import type { CreatorHistoryRepository } from "./history-repository.ts";
import { historyConversationListInput, historyTriageListInput, historyTriageRecordInput } from "./history-contracts.ts";

export interface CreatorHistoryResolver { resolve(context: AccountContext): Promise<Readonly<{ owner: CreatorRepositoryOwner; repository: CreatorHistoryRepository }>> }
export function registerCreatorHistoryOperations(registry: OperationRegistry, accounts: AccountContextService, resolver: CreatorHistoryResolver, onChanged?: (context: AccountContext, scopes: readonly ["reply_triage"]) => void): void {
  async function execute<T>(context: AccountContext, operation: (repository: CreatorHistoryRepository) => T, changed = false) {
    accounts.assertCurrent(context); const resolved = await resolver.resolve(context); accounts.assertCurrent(context);
    if (resolved.owner.accountId !== context.principal.user.id || resolved.owner.tenantId !== context.principal.tenantId || resolved.owner.baseId !== context.principal.baseId) throw new Error("Creator history repository does not belong to the verified account");
    const value = operation(resolved.repository); if (changed) onChanged?.(context, ["reply_triage"]); return value;
  }
  registry.register("onecrew.outreach.action.status", z.strictObject({ idempotency_key: z.string() }), (input, context) => execute(context, repository => repository.actionStatus(input.idempotency_key)));
  registry.register("onecrew.outreach.contact_route.last_contact_account", z.strictObject({ creator_id: z.string(), contact_route_id: z.string() }), (input, context) => execute(context, repository => repository.lastContact(input.creator_id, input.contact_route_id)));
  registry.register("onecrew.outreach.conversation.get", z.strictObject({ conversation_id: z.string() }), (input, context) => execute(context, repository => repository.conversationGet(input.conversation_id)));
  registry.register("onecrew.outreach.conversation.list", historyConversationListInput, (input, context) => execute(context, repository => repository.conversationList(input)));
  registry.register("onecrew.triage.list", historyTriageListInput, (input, context) => execute(context, repository => repository.triageList(input)));
  registry.register("onecrew.triage.record", historyTriageRecordInput, (input, context) => execute(context, repository => repository.triageRecord(input), true));
  registry.register("onecrew.triage.acknowledge", z.strictObject({ triage_id: z.string() }), (input, context) => execute(context, repository => repository.triageAcknowledge(input.triage_id), true));
}
