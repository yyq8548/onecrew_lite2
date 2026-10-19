import { z } from "zod";
import type { OperationRegistry } from "../application/operation-registry.ts";
import type { AccountDatabases } from "../identity/account-databases.ts";
import type { UnifiedAccountService } from "../identity/account-service.ts";
import type { DiscoveryAdmissionGate } from "../discovery/admission-gate.ts";
import type { TableSyncCaptureSource } from "./table-sync-worker.ts";
import { TableSyncWorker } from "./table-sync-worker.ts";
import { TableSyncClient } from "./table-sync-client.ts";
import { TableSyncStore } from "./table-sync-store.ts";

// 定时器只处理真实账号的持久队列；开启同步仍须原服务授权和逐次检查。
export function createTableSyncModule(identities: UnifiedAccountService, databases: AccountDatabases, operations: OperationRegistry, gate: DiscoveryAdmissionGate, capture: TableSyncCaptureSource, blocksBackendSync: () => boolean) {
  const accounts = identities.contexts;
  const store = new TableSyncStore(accounts, databases);
  const worker = new TableSyncWorker(accounts, store, new TableSyncClient(identities), gate, capture, blocksBackendSync);
  let stopped = false;
  let pending: Promise<void> | undefined;
  let error: unknown;
  let retryAt = 0;
  const wake = () => {
    const current = accounts.currentIfActive();
    if (stopped || pending || !current || performance.now() < retryAt) return;
    const task = (async () => {
      if (!await worker.hasQueued(current) || stopped) return;
      const context = await identities.refresh();
      await worker.tick(context);
    })();
    pending = task.then(() => { error = undefined; pending = undefined; }, failure => {
      error = failure; pending = undefined; retryAt = performance.now() + 300_000;
      console.error("Dynamic table synchronization stopped:", failure);
    });
  };
  operations.register("onecrew_table_sync_status", z.object({ campaignId: z.string() }).strict(), (input, context) => worker.status(context, input.campaignId));
  operations.register("onecrew_table_sync_configure", z.object({ input: z.object({ campaign_id: z.string(), enabled: z.boolean() }).strict() }).strict(), async ({ input }, context) => {
    const result = await worker.configure(context, input.campaign_id, input.enabled); retryAt = 0; wake(); return result;
  });
  operations.register("onecrew_table_sync_consent", z.object({ input: z.object({ action: z.enum(["grant", "revoke"]), operation_id: z.uuid(), policy_fingerprint: z.string().nullable() }).strict() }).strict(), async ({ input }, context) => {
    const result = await worker.changeConsent(context, input); retryAt = 0; wake(); return result;
  });
  const timer = setInterval(wake, 30_000); timer.unref();
  return {
    worker,
    status: () => ({ running: pending !== undefined, failed: error !== undefined }),
    stopAcceptingWork() { stopped = true; clearInterval(timer); worker.stopAcceptingWork(); },
    async close() {
      stopped = true; clearInterval(timer); worker.stopAcceptingWork();
      await pending;
      store.close();
      if (error !== undefined) throw new AggregateError([error], "Dynamic table synchronization did not drain successfully");
    },
  };
}
