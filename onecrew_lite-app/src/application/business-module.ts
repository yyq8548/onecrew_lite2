import { AccountDatabases } from "../identity/account-databases.ts";
import { registerCreatorOperations } from "../creators/operations.ts";
import { getDesktopDirectory } from "../native-capabilities/desktop-directory.ts";
import type { UnifiedAccountService } from "../identity/account-service.ts";
import { AccessGmailProvider } from "../mail/gmail-access.ts";
import { registerGmailBulkOperations } from "../mail/gmail-bulk-account-binding.ts";
import { GmailBulkWorker } from "../mail/gmail-bulk-worker.ts";
import { registerMailHistoryOperations } from "../mail/mail-history-operations.ts";
import { registerGmailAccessOperations } from "../mail/gmail-access-operations.ts";
import { registerMailThreadMaintenanceOperations } from "../mail/mail-thread-maintenance.ts";
import type { InstanceLease } from "../service/instance-lock.ts";
import { BrowserSessions } from "../service/browser-sessions.ts";
import { OperationRegistry } from "./operation-registry.ts";
import { DomainChanges } from "./domain-changes.ts";
import { GmailReplySync, registerGmailReplySyncOperation } from "../mail/gmail-reply-sync.ts";
import { McpSessions, type McpSurface } from "../service/mcp-sessions.ts";
import { OwnerResults } from "./owner-results.ts";
import type { PlatformVaultReader } from "../platforms/browser/platform-vault.ts";
import { OwnedPlatformVaultResolver } from "../platforms/browser/owned-accounts.ts";
import { NativeCreatorSaveProfileResolver } from "../platforms/candidates/resolver.ts";
import { OwnedSavedTelegramCommunityResolver } from "../platforms/candidates/saved-telegram-community.ts";
import { setTimeout as delay } from "node:timers/promises";
import type { createEmbeddedTables } from "onecrew_lite_tables/embedded";
import { DiscoveryAdmissionGate } from "../discovery/admission-gate.ts";
import { DiscoveryExecutionSupervisor } from "../discovery/execution-supervisor.ts";
import { createDiscoveryLibraryProjection } from "../sync/discovery-library.ts";
import { createLibraryCaptureSource } from "../sync/library-capture.ts";
import { createTableSyncModule } from "../sync/table-sync-module.ts";
import { registerDiscoveryIdentityOperations } from "../discovery/identity-operations.ts";
import type { AccountContext } from "../identity/account-context.ts";
import { createDefaultDiscoveryDependencies } from "../discovery/dependencies.ts";
import { createDiscoveryRuntime } from "../discovery/runtime.ts";
import { join } from "node:path";
import { lstat } from "node:fs/promises";
import { z } from "zod";
import { createPrivateDirectory, verifyPrivateDirectory } from "../storage/private-backup.ts";
import { ManagedBrowserOwnership } from "../platforms/browser/extension-ownership.ts";
import { AccountBrowserBindingResolver } from "../platforms/browser/binding.ts";
import { ManagedBrowserConnections } from "../platforms/browser/managed-connection.ts";
import { resolveOpenCliInstallation } from "../platforms/opencli/installation.ts";
import { projectMcpConnections } from "../platforms/browser/connection-inventory.ts";
import { registerOpenCliOperations } from "../platforms/opencli/operations.ts";
import { OpenCliAddons } from "../platforms/opencli/addons.ts";
import { runtimeAssetPath } from "../packaging/runtime-context.ts";
import { probeProxy, proxyConfigSchema } from "../platforms/browser/proxy.ts";
import { registerNativeSearchOperations } from "../platforms/browser/native-search-operations.ts";
import { WorkspaceCardRequests } from "../service/workspace-card-requests.ts";
import { OwnerPlatformVaultStore } from "../platforms/browser/owner-vault-store.ts";
import { registerYoutubeCredentialOperations } from "../platforms/browser/youtube-credentials-operations.ts";
import { registerPublicReadOperations } from "../platforms/public-read-operations.ts";
import { OperatorBrowserConnections } from "../platforms/browser/operator-browser-connection.ts";
import { TelegramConnectionRuntime } from "../platforms/browser/telegram-connection.ts";
import { registerTelegramConnectionOperations } from "../platforms/browser/telegram-connection-operations.ts";
import { registerOperatorConnectionOperations } from "../platforms/browser/operator-connection-operations.ts";
import { registerTelegramPublicReadOperations } from "../platforms/browser/telegram-public-operations.ts";
import { registerOperatorProfileOperations } from "../platforms/browser/operator-profile-operations.ts";
import { registerOperatorDisconnectOperations } from "../platforms/browser/operator-disconnect.ts";
import { registerConnectionOperations } from "../platforms/connection-operations.ts";
import { registerDiscoveryReadOperations } from "../platforms/discovery-read-operations.ts";
import { registerCreatorHistoryOperations } from "../creators/history-operations.ts";

// 主服务持有实例锁后装配。这里不自动创建账号数据库，也不自动发送邮件。
export function createBusinessModule(lease: InstanceLease, identities: UnifiedAccountService, tables: Awaited<ReturnType<typeof createEmbeddedTables>>, origin: string, generationRoot?: string, platformVault?: PlatformVaultReader, initializeNewAccounts = false) {
  const databases = new AccountDatabases(lease, identities.contexts, generationRoot);
  const browserSessions = new BrowserSessions(identities.contexts);
  const changes = new DomainChanges(identities.contexts);
  const mcpSessions = new McpSessions(identities.contexts);
  const workspaceCards = new WorkspaceCardRequests();
  const ownerVaultStore = platformVault ? new OwnerPlatformVaultStore(lease, identities.contexts, identities, databases, platformVault) : undefined;
  const ownedPlatforms = platformVault ? new OwnedPlatformVaultResolver(identities.contexts, identities, databases, platformVault, ownerVaultStore) : undefined;
  const operatorBrowsers = ownerVaultStore ? new OperatorBrowserConnections(identities.contexts, databases, ownerVaultStore, context => changes.publish(context, ["creators"])) : undefined;
  const telegramConnections = ownerVaultStore && operatorBrowsers ? new TelegramConnectionRuntime(identities.contexts, ownerVaultStore, context => changes.publish(context, ["creators"]), { openTelegramCredentials: (context, proxy) => operatorBrowsers.openTelegramCredentials(context, proxyConfigSchema.parse(proxy)) }) : undefined;
  const extensionOwnership = new ManagedBrowserOwnership(identities.contexts, databases);
  const browserBindings = new AccountBrowserBindingResolver(identities.contexts, identities, databases, platformVault, extensionOwnership);
  let preparingOpenCliHome: Promise<void> = Promise.resolve();
  async function openCliHome(context: AccountContext): Promise<string> {
    const account = await databases.accountDirectory(context), home = join(account.directory, "opencli-home");
    const operation = preparingOpenCliHome.then(async () => {
      for (const path of [home, join(home, ".opencli")]) {
        const exists = await lstat(path).then(() => true, error => { if (error.code === "ENOENT") return false; throw error; });
        if (exists) await verifyPrivateDirectory(path); else await createPrivateDirectory(path);
      }
      account.assertOwned(); identities.contexts.assertCurrent(context);
    });
    preparingOpenCliHome = operation.then(() => {}, () => {}); await operation; return home;
  }
  const managedBrowsers = new ManagedBrowserConnections(identities.contexts, databases, extensionOwnership, browserBindings, origin, async context => ({ installation: await resolveOpenCliInstallation(), configurationDirectory: join(await openCliHome(context), ".opencli") }));
  const results = new OwnerResults(identities.contexts, mcpSessions, databases.creatorsResolver(), ownedPlatforms ? async context => new NativeCreatorSaveProfileResolver(context, ownedPlatforms, new OwnedSavedTelegramCommunityResolver(context, ownedPlatforms, databases)) : undefined, context => changes.publish(context, ["creators"]));
  let closing = false;
  function assertAcceptingWork(): void {
    if (closing) throw Object.assign(new Error("Unified service is stopping"), { code: "runtime.stopping", status: 503 });
  }
  async function refreshAccount() {
    assertAcceptingWork();
    const context = await identities.refresh();
    assertAcceptingWork();
    if (initializeNewAccounts) await databases.ensureNewAccount(context);
    if (youtubeCredentials) await youtubeCredentials.recover(context);
    if (operatorDisconnect) await operatorDisconnect.recover(context);
    assertAcceptingWork();
    return context;
  }
  const operations = new OperationRegistry(identities.contexts, async () => {
    await refreshAccount();
  });
  if (ownedPlatforms) registerNativeSearchOperations(operations, identities.contexts, results, ownedPlatforms);
  if (ownedPlatforms) registerPublicReadOperations(operations, identities.contexts, ownedPlatforms);
  if (ownedPlatforms) registerTelegramPublicReadOperations(operations, identities.contexts, ownedPlatforms);
  const youtubeCredentials = ownerVaultStore && ownedPlatforms ? registerYoutubeCredentialOperations(operations, identities.contexts, ownerVaultStore, context => changes.publish(context, ["creators"]), id => ownedPlatforms.youtubeRequest(id)) : undefined;
  if (telegramConnections && operatorBrowsers) { registerTelegramConnectionOperations(operations, telegramConnections); registerOperatorConnectionOperations(operations, operatorBrowsers, telegramConnections); }
  if (telegramConnections && ownerVaultStore) registerOperatorProfileOperations(operations, identities.contexts, ownerVaultStore, telegramConnections, context => changes.publish(context, ["creators"]));
  const operatorDisconnect = ownerVaultStore ? registerOperatorDisconnectOperations(operations, identities.contexts, ownerVaultStore, context => changes.publish(context, ["creators"])) : undefined;
  const admission = new DiscoveryAdmissionGate(identities.contexts);
  const supervisor = new DiscoveryExecutionSupervisor(identities.contexts, admission);
  const discoveryDependencies = createDefaultDiscoveryDependencies({ accounts: identities.contexts, identities, directories: databases, browsers: { resolve: (context, profile) => browserBindings.resolveProfileRef(context, profile) }, openCliHome, ...(platformVault ? { vault: platformVault } : {}) });
  const sync = createTableSyncModule(identities, databases, operations, admission, createLibraryCaptureSource(identities.contexts, tables), () => supervisor.blocksBackendSync());
  const projection = createDiscoveryLibraryProjection(identities.contexts, databases, tables, origin);
  const discoveryLibrary = {
    readiness: projection.readiness,
    async publish(...arguments_: Parameters<typeof projection.publish>) {
      const result = await projection.publish(...arguments_);
      await sync.worker.registerResult(arguments_[0], arguments_[1]);
      return result;
    },
  };
  const discovery = createDiscoveryRuntime({ accounts: identities.contexts, repositories: databases.discoveryResolver(), directories: databases, dependencies: discoveryDependencies, supervisor, library: discoveryLibrary, onStarted: (context, campaign) => workspaceCards.started(context.principal.user.id, campaign) });
  const bulk = databases.bulkResolver();
  if (ownedPlatforms) {
    operations.register("list_profiles", z.strictObject({}), async (_, context) => ownedPlatforms.profiles(context));
    operations.register("list_youtube_api_keys", z.strictObject({}), async (_, context) => ownedPlatforms.youtubeCredentials(context));
  }
  operations.register("probe_proxy", z.strictObject({ proxy: proxyConfigSchema }), async (input, context) => { const result = await probeProxy(input.proxy, context.signal); identities.contexts.assertCurrent(context); return result; });
  const openCliOperations = registerOpenCliOperations(operations, identities.contexts, browserBindings, { addons: async context => new OpenCliAddons(runtimeAssetPath("opencli-addons", new URL("../../../onecrew_lite-rust/integrations/opencli", import.meta.url)), await openCliHome(context)) });
  registerDiscoveryReadOperations(operations, identities.contexts, { setupReport: context => openCliOperations.status(context) });
  async function connectionInventory(context: AccountContext) {
    identities.contexts.assertCurrent(context);
    if (!ownedPlatforms) throw new Error("platform_inventory_vault_unavailable");
    const platforms = await ownedPlatforms.inventory(context), repositories = await databases.resolve(context);
    const mailboxes = repositories.history.mailboxes().map(mailbox => ({ mailbox_ref_id: mailbox.mailbox_ref_id, email_address: mailbox.email_address, display_label: mailbox.display_name?.trim() || mailbox.label.trim() || mailbox.email_address, provider: mailbox.provider, availability: mailbox.available ? "available" as const : "unavailable" as const }));
    mailboxes.sort((a, b) => Buffer.compare(Buffer.from(a.email_address), Buffer.from(b.email_address)) || Buffer.compare(Buffer.from(a.mailbox_ref_id), Buffer.from(b.mailbox_ref_id)));
    identities.contexts.assertCurrent(context); return { ...platforms, mailboxes };
  }
  operations.register("onecrew_managed_browser_connection_prepare", z.strictObject({}), async (_, context) => managedBrowsers.prepare(context));
  operations.register("onecrew_managed_browser_bindings", z.strictObject({}), async (_, context) => browserBindings.listOwned(context));
  operations.register("onecrew_managed_browser_connection_reopen", z.strictObject({ bindingId: z.uuid(), confirmed: z.literal(true) }), async (input, context) => { const prepared = await managedBrowsers.prepareReopen(context, input.bindingId); return managedBrowsers.begin(context, prepared.flow_id, input.confirmed); });
  operations.register("onecrew_managed_browser_connection_status", z.strictObject({ flowId: z.uuid() }), async (input, context) => managedBrowsers.status(context, input.flowId));
  operations.register("onecrew_managed_browser_connection_begin", z.strictObject({ flowId: z.uuid(), confirmed: z.literal(true) }), async (input, context) => managedBrowsers.begin(context, input.flowId, input.confirmed));
  operations.register("onecrew_managed_browser_connection_finalize", z.strictObject({ flowId: z.uuid(), contextId: z.string().min(1), confirmed: z.literal(true) }), async (input, context) => managedBrowsers.finalize(context, input.flowId, input.contextId, input.confirmed));
  operations.register("onecrew_managed_browser_connection_cancel", z.strictObject({ flowId: z.uuid(), confirmed: z.literal(true) }), async (input, context) => managedBrowsers.cancel(context, input.flowId));
  const access = new AccessGmailProvider(identities, databases.mailboxResolver(identities));
  if (ownerVaultStore && telegramConnections && youtubeCredentials) registerConnectionOperations(operations, identities.contexts, { store: ownerVaultStore, telegram: telegramConnections, youtube: youtubeCredentials, gmailProfile: (context, mailbox) => access.profile({ accountId: context.principal.user.id, tenantId: context.principal.tenantId, baseId: context.principal.baseId }, mailbox, context.signal) });
  const replies = new GmailReplySync(identities.contexts, identities, databases.repliesResolver(), access, (context, scopes) => changes.publish(context, scopes));
  registerGmailBulkOperations(operations, bulk, identities.contexts, () => wakeMail());
  registerMailHistoryOperations(operations, databases.historyResolver(), identities.contexts, (context, id) => replies.status(context, id));
  registerMailThreadMaintenanceOperations(operations, databases.maintenanceResolver(), identities.contexts, (context, scopes) => changes.publish(context, scopes));
  registerGmailAccessOperations(operations, identities.contexts, identities, databases.mailboxResolver(identities), (context, scopes) => changes.publish(context, scopes));
  registerCreatorOperations(operations, databases.creatorsResolver(), identities.contexts, (context, scopes) => changes.publish(context, scopes), getDesktopDirectory);
  registerCreatorHistoryOperations(operations, identities.contexts, databases.creatorHistoryResolver(), (context, scopes) => changes.publish(context, scopes));
  discovery.registerOperations(operations);
  registerDiscoveryIdentityOperations(operations, databases.identityResolver(), identities.contexts);
  registerGmailReplySyncOperation(operations, replies);
  const worker = new GmailBulkWorker(identities.contexts, bulk, access, (context, scopes) => changes.publish(context, scopes));
  const shutdown = new AbortController();
  let pending: ReturnType<GmailBulkWorker["tick"]> | undefined;
  let draining: Promise<void> | undefined;
  let drainError: unknown;
  let wakeRequested = false;
  async function tickMail() {
    assertAcceptingWork();
    if (pending) throw new Error("Mail module tick is already running");
    const task = (async () => {
      await identities.refresh();
      return worker.tick(shutdown.signal);
    })();
    pending = task;
    try { return await task; }
    finally { if (pending === task) pending = undefined; }
  }
  function wakeMail(): void {
    assertAcceptingWork();
    wakeRequested = true;
    if (draining) return;
    drainError = undefined;
    const task = (async () => {
      do {
        wakeRequested = false;
        while (!closing && !shutdown.signal.aborted) {
          await delay(2_000);
          if (closing || shutdown.signal.aborted) break;
          if ((await tickMail()).state === "idle") break;
        }
      } while (wakeRequested && !closing);
    })();
    draining = task.then(() => { draining = undefined; if (wakeRequested && !closing) wakeMail(); }, error => {
      drainError = error;
      draining = undefined;
      console.error("Gmail bulk worker stopped:", error);
    });
  }
  return {
    operations,
    browserSessions,
    changes,
    mcpSessions,
    workspaceCards,
    results,
    managedBrowsers,
    openCliStatus: openCliOperations.status,
    async workspaceBrowserReady(context: AccountContext) {
      const status = await openCliOperations.status(context);
      const owned = await browserBindings.listOwned(context);
      identities.contexts.assertCurrent(context);
      return status.issue === null && owned.some(binding => status.profiles.some(profile => profile.context_id === binding.context_id));
    },
    connectionInventory,
    async mcpConnections(session: string, toolkits: readonly string[]) {
      const selected = mcpSessions.authorize(session), inventory = await connectionInventory(selected.account);
      mcpSessions.authorize(session); return projectMcpConnections(inventory, toolkits);
    },
    discoveryServices: { admission, supervisor, runtime: discovery, library: discoveryLibrary, repositories: databases.discoveryResolver(), directories: databases, ownedPlatforms, dependencies: discoveryDependencies },
    tableSyncStatus: sync.status,
    async authorizeMcp(session: string) {
      assertAcceptingWork();
      await refreshAccount();
      assertAcceptingWork();
      return mcpSessions.authorize(session);
    },
    async issueMcpSession(surface: McpSurface) {
      assertAcceptingWork();
      const context = await refreshAccount();
      assertAcceptingWork();
      return mcpSessions.create(context, surface);
    },
    async issueBrowserTicket(expected?: AccountContext): Promise<string> {
      assertAcceptingWork();
      const context = await refreshAccount();
      assertAcceptingWork();
      if (expected) identities.contexts.assertCurrent(expected);
      return browserSessions.issueTicket(context);
    },
    async tick() {
      if (draining) throw new Error("Mail module is draining confirmed work");
      return tickMail();
    },
    mailWorkerStatus() { return { running: draining !== undefined, failed: drainError !== undefined }; },
    stopAcceptingWork(): void {
      closing = true;
      shutdown.abort();
      replies.stopAcceptingWork();
      discovery.stopAcceptingWork();
      sync.stopAcceptingWork();
      browserSessions.clear();
      mcpSessions.close();
      results.close();
      openCliOperations.close();
      identities.invalidate();
      changes.close();
    },
    async close(): Promise<void> {
      closing = true;
      shutdown.abort();
      replies.stopAcceptingWork();
      discovery.stopAcceptingWork();
      sync.stopAcceptingWork();
      browserSessions.clear();
      mcpSessions.close();
      results.close();
      openCliOperations.close();
      identities.invalidate();
      changes.close();
      // 等待原账号回执事务结束，关闭连接后继续报告任务错误。
      const settled = await Promise.allSettled([managedBrowsers.close(), ...(operatorBrowsers ? [operatorBrowsers.close()] : []), ...(telegramConnections ? [telegramConnections.close()] : []), ...(ownedPlatforms ? [ownedPlatforms.close()] : []), replies.close(), discovery.close(), sync.close(), ...(draining ? [draining] : []), ...(pending ? [pending] : [])]);
      databases.close();
      const failures = settled.flatMap(result => result.status === "rejected" ? [result.reason as unknown] : []);
      if (drainError !== undefined && !failures.includes(drainError)) failures.push(drainError);
      if (failures.length) throw new AggregateError(failures, "Unified business tasks did not drain successfully");
    },
  };
}
