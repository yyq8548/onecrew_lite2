import { z } from "zod";
import type { OperationRegistry } from "../application/operation-registry.ts";
import type { AccountContext, AccountContextService } from "../identity/account-context.ts";
import { listCreatorsInput, manualCreatorInput, pinCreatorInput, stageCreatorInput, text, updateCreatorInput } from "./contracts.ts";
import type { CreatorRepository } from "./repository.ts";
import { groupCreateInput, groupDeleteInput, groupListInput, groupMoveInput, groupSetInput, groupUpdateInput } from "./groups.ts";
import { contactAddInput, contactConfirmInput, contactDeleteInput, contactUpdateInput, identityResolveInput } from "./contacts.ts";
import { exportInput, writeDesktopExport } from "./transfer-export.ts";
import { importFileInput, importOperationInput } from "./import.ts";
import { decodeWorkbook } from "./workbook.ts";

export interface CreatorRepositoryOwner { accountId: string; tenantId: string; baseId: string }
export interface CreatorRepositoryResolver {
  resolve(context: AccountContext): Promise<Readonly<{ owner: CreatorRepositoryOwner; repository: CreatorRepository }>>;
}
export type CreatorChangeScope = "creators" | "creator_groups" | "email_folders" | "email_threads" | "email_drafts" | "outreach";
export function registerCreatorOperations(registry: OperationRegistry, resolver: CreatorRepositoryResolver, accounts: AccountContextService, onChanged?: (context: AccountContext, scopes: readonly CreatorChangeScope[]) => void, getDesktopDirectory?: () => Promise<string>): void {
  async function execute<T>(context: AccountContext, operation: (repository: CreatorRepository) => T, changes?: readonly CreatorChangeScope[] | ((result: T) => readonly CreatorChangeScope[])): Promise<T> {
    accounts.assertCurrent(context);
    const resolved = await resolver.resolve(context);
    accounts.assertCurrent(context);
    const expected = { accountId: context.principal.user.id, tenantId: context.principal.tenantId, baseId: context.principal.baseId };
    for (const field of ["accountId", "tenantId", "baseId"] as const) if (resolved.owner[field] !== expected[field]) throw new Error("Creator repository does not belong to the verified account");
    const result = operation(resolved.repository);
    const scopes = typeof changes === "function" ? changes(result) : changes;
    if (scopes?.length) onChanged?.(context, scopes);
    return result;
  }
  const creatorIdInput = z.object({ creator_id: text }).strict();
  registry.register("onecrew_creator_list", z.object({ input: listCreatorsInput }).strict(), async ({ input }, context) => execute(context, repository => repository.list(input)));
  registry.register("onecrew_creator_get", z.object({ input: creatorIdInput }).strict(), async ({ input }, context) => execute(context, repository => repository.get(input.creator_id)));
  registry.register("onecrew_creator_manual_create", z.object({ input: manualCreatorInput }).strict(), async ({ input }, context) => execute(context, repository => repository.create(input), ["creators"]));
  registry.register("onecrew_creator_update", z.object({ input: updateCreatorInput }).strict(), async ({ input }, context) => execute(context, repository => repository.update(input), ["creators"]));
  registry.register("onecrew_creator_delete", z.object({ input: creatorIdInput }).strict(), async ({ input }, context) => execute(context, repository => repository.delete(input.creator_id), ["creators"]));
  registry.register("onecrew_creator_group_list", z.object({ input: groupListInput }).strict(), async ({ input }, context) => execute(context, repository => repository.groupList(input)));
  registry.register("onecrew_creator_group_create", z.object({ input: groupCreateInput }).strict(), async ({ input }, context) => execute(context, repository => repository.groupCreate(input), ["creator_groups"]));
  registry.register("onecrew_creator_group_update", z.object({ input: groupUpdateInput }).strict(), async ({ input }, context) => (await execute(context, repository => repository.groupUpdate(input), result => result.changed ? ["creator_groups", "creators"] : [])).group);
  registry.register("onecrew_creator_group_delete", z.object({ input: groupDeleteInput }).strict(), async ({ input }, context) => execute(context, repository => repository.groupDelete(input), ["creator_groups", "creators"]));
  registry.register("onecrew_creator_group_set", z.object({ input: groupSetInput }).strict(), async ({ input }, context) => execute(context, repository => repository.groupSet(input), result => result.moved_count > 0 ? ["creator_groups", "creators"] : []));
  registry.register("onecrew_creator_group_bulk_move", z.object({ input: groupMoveInput }).strict(), async ({ input }, context) => execute(context, repository => repository.groupMove(input), result => result.moved_count > 0 ? ["creator_groups", "creators"] : []));
  registry.register("onecrew_creator_update_stage", z.object({ input: stageCreatorInput }).strict(), async ({ input }, context) => (await execute(context, repository => repository.updateStage(input), result => result.changed ? ["creators"] : [])).relationship);
  registry.register("onecrew_creator_pin", z.object({ input: pinCreatorInput }).strict(), async ({ input }, context) => execute(context, repository => repository.pin(input), ["creators"]));
  registry.register("onecrew_creator_archive", z.object({ creatorId: text }).strict(), async ({ creatorId }, context) => (await execute(context, repository => repository.archive(creatorId), result => result.changed ? ["creators"] : [])).creator);
  registry.register("onecrew_creator_contact_route_add", z.object({ input: contactAddInput }).strict(), async ({ input }, context) => execute(context, repository => repository.contactAdd(input), result => result.status === "added" ? ["creators"] : []));
  registry.register("onecrew_creator_contact_route_update", z.object({ input: contactUpdateInput }).strict(), async ({ input }, context) => execute(context, repository => repository.contactUpdate(input), result => result.status === "updated" ? ["creators"] : []));
  registry.register("onecrew_creator_contact_route_delete_prepare", z.object({ input: contactDeleteInput }).strict(), async ({ input }, context) => execute(context, repository => repository.contactDeletePrepare(input)));
  registry.register("onecrew_creator_contact_route_delete_confirm", z.object({ input: contactConfirmInput }).strict(), async ({ input }, context) => (await execute(context, repository => repository.contactDeleteConfirm(input), outcome => {
    if (!outcome.changed || outcome.result.status !== "deleted") return [];
    const scopes: CreatorChangeScope[] = ["creators"], deleted = outcome.result.deleted;
    if (deleted.email_threads > 0 || deleted.email_messages > 0 || deleted.email_recipients > 0) scopes.push("email_folders", "email_threads");
    if (deleted.email_drafts > 0) { if (!scopes.includes("email_folders")) scopes.push("email_folders"); scopes.push("email_drafts"); }
    if (deleted.direct_message_conversations > 0 || deleted.direct_messages > 0 || deleted.outreach_touchpoints > 0) scopes.push("outreach");
    return scopes;
  })).result);
  registry.register("onecrew.creator.identity.resolve", identityResolveInput, async (input, context) => execute(context, repository => repository.identityResolve(input)));
  const exportHandler = async (input: z.infer<typeof exportInput>, context: AccountContext) => { exportInput.parse(input); const snapshot = await execute(context, repository => repository.exportSnapshot()); accounts.assertCurrent(context); return writeDesktopExport(snapshot, getDesktopDirectory); };
  registry.register("onecrew_creator_export", z.object({ input: exportInput }).strict(), async ({ input }, context) => exportHandler(input, context));
  registry.register("onecrew.creator.export", exportInput, exportHandler);
  registry.register("onecrew_creator_import_file_prepare", importFileInput, async ({ file_name, bytes }, context) => {
    if (Buffer.byteLength(file_name) > 255 || /[\/\\\0]/.test(file_name) || !file_name.toLowerCase().endsWith(".xlsx")) throw new Error("Creator import requires an .xlsx file name");
    const bundle = await decodeWorkbook(Buffer.from(bytes));
    const prepared = await execute(context, repository => repository.importPrepare(bundle));
    return { ...prepared, file_name, ...(prepared.status === "confirmation_required" ? { preview_rows: bundle.creators } : {}) };
  });
  registry.register("onecrew_creator_import_confirm", z.object({ input: importOperationInput }).strict(), async ({ input }, context) => (await execute(context, repository => repository.importConfirm(input), outcome => outcome.changed ? ["creators"] : [])).result);
  registry.register("onecrew_creator_import_cancel", z.object({ input: importOperationInput }).strict(), async ({ input }, context) => execute(context, repository => repository.importCancel(input)));
}
