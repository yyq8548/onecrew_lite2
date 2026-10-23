import type { createEmbeddedTables } from "onecrew_lite_tables/embedded";
import type { AccountContextService } from "../identity/account-context.ts";
import type { TableSyncCaptureSource } from "./table-sync-worker.ts";

export function createLibraryCaptureSource(accounts: AccountContextService, tables: Awaited<ReturnType<typeof createEmbeddedTables>>): TableSyncCaptureSource {
  return {
    async capture(context, campaign, metadata) {
      accounts.assertCurrent(context);
      const tenant = { user: context.principal.user, tenantId: context.principal.tenantId, baseId: context.principal.baseId };
      const actual = await tables.tenants.resolve(tenant.user);
      if (actual.tenantId !== tenant.tenantId || actual.baseId !== tenant.baseId) throw new Error("table_sync_account_changed");
      accounts.assertCurrent(context);
      const table = await tables.data.getDiscoveryCampaignTable(tenant, campaign);
      if (!table) throw new Error("table_sync_recommendation_table_missing");
      accounts.assertCurrent(context);
      const capture = await tables.service.captureRecommendations(tenant, table.id, metadata);
      accounts.assertCurrent(context);
      return capture;
    },
  };
}
