import { join } from "node:path";
import { DatabaseSync } from "node:sqlite";
import { z } from "zod";
import type { AccountContext } from "../../identity/account-context.ts";
import { verifyPrivateFile } from "../../storage/private-backup.ts";
import type { BrowserAccountDirectoryResolver } from "../browser/binding.ts";
import type { OwnedPlatformAccountResolver } from "../browser/owned-accounts.ts";
import { CreatorProviderReadError } from "./projection.ts";
import type { SavedTelegramCommunityResolver } from "./telegram-read.ts";

export function readSavedTelegramCommunity(database: DatabaseSync, creatorId: string, routeId: string, accountRefId: string): Awaited<ReturnType<SavedTelegramCommunityResolver["resolve"]>> {
  z.uuid().parse(creatorId); z.uuid().parse(routeId); z.uuid().parse(accountRefId);
  const row = database.prepare("SELECT target.id,target.external_target_id,target.target_kind FROM creator_contact_routes route JOIN creators creator ON creator.id=route.creator_id AND creator.archived_at IS NULL JOIN contact_locators locator ON locator.id=route.locator_id AND locator.channel='telegram' JOIN account_refs account ON account.id=?3 AND account.platform='telegram' AND account.kind='profile' AND account.active=1 AND account.availability='available' JOIN locator_resolutions resolution ON resolution.locator_id=locator.id AND resolution.account_ref_id=account.id AND resolution.state='active' JOIN resolved_targets target ON target.id=resolution.resolved_target_id AND target.account_ref_id=account.id AND target.platform='telegram' WHERE route.id=?1 AND route.creator_id=?2").get(routeId, creatorId, accountRefId);
  if (!row || typeof row.id !== "string" || typeof row.external_target_id !== "string" || !row.external_target_id.trim() || row.target_kind !== "channel" && row.target_kind !== "supergroup") throw new CreatorProviderReadError("source_mismatch", "telegram_saved_community_unavailable");
  return { externalId: row.external_target_id.trim(), kind: row.target_kind, source: { kind: "creator_contact_route" as const, source_creator_id: creatorId, source_contact_route_id: routeId, operator_account_ref_id: accountRefId, source_resolved_target_id: row.id } };
}

export class OwnedSavedTelegramCommunityResolver implements SavedTelegramCommunityResolver {
  readonly context: AccountContext;
  readonly accounts: OwnedPlatformAccountResolver;
  readonly directories: BrowserAccountDirectoryResolver;
  constructor(context: AccountContext, accounts: OwnedPlatformAccountResolver, directories: BrowserAccountDirectoryResolver) { this.context = context; this.accounts = accounts; this.directories = directories; }
  async resolve(creatorId: string, routeId: string, accountRefId: string) {
    return this.accounts.withProfile(this.context, accountRefId, "telegram", async (_profile, _secret, assertCurrent) => {
      await assertCurrent(); const directory = await this.directories.accountDirectory(this.context);
      const expected = { accountId: this.context.principal.user.id, tenantId: this.context.principal.tenantId, baseId: this.context.principal.baseId };
      for (const field of ["accountId", "tenantId", "baseId"] as const) if (directory.owner[field] !== expected[field]) throw new Error("Telegram community database belongs to another account");
      const path = join(directory.directory, "creator.sqlite"); await verifyPrivateFile(path); directory.assertOwned();
      const database = new DatabaseSync(path, { readOnly: true });
      const result = (() => { try { return readSavedTelegramCommunity(database, creatorId, routeId, accountRefId); } finally { database.close(); } })();
      directory.assertOwned(); await assertCurrent(); return result;
    });
  }
}
