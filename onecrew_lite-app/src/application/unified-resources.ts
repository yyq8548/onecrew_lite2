import { lstatSync } from "node:fs";
import { isAbsolute, join, relative, resolve, sep } from "node:path";
import { readFile, realpath } from "node:fs/promises";
import type { Context } from "hono";
import { getCookie } from "hono/cookie";
import { streamSSE } from "hono/streaming";
import { createEmbeddedTables, createSystemRoutes } from "onecrew_lite_tables/embedded";
import type { AccountContext } from "../identity/account-context.ts";
import { UnifiedAccountService } from "../identity/account-service.ts";
import { assertNativeTelegramCryptography } from "../native-capabilities/telegram-crypto.ts";
import type { RuntimeBootstrap } from "../service/runtime.ts";
import { browserCookieName } from "../service/browser-sessions.ts";
import { createPrivateDirectory, verifyPrivateDirectory } from "../storage/private-backup.ts";
import { createBusinessModule } from "./business-module.ts";
import { WebAssets } from "../service/web-assets.ts";
import { AccessLifecycle } from "../identity/access-lifecycle.ts";
import { hostname } from "node:os";
import { initializeNewInstallVault } from "../storage/new-install-vault.ts";
import { mcpSurface } from "../service/mcp-sessions.ts";
import { z } from "zod";
import { parseApplicationJson, stringifyApplicationJson } from "../contracts/application-json.ts";
import { readActiveGeneration } from "../migration/activation.ts";
import { dataPathIdentity, readDevelopmentBuildIdentity } from "../service/build-identity.ts";
import { PlatformVaultReader } from "../platforms/browser/platform-vault.ts";
import { decodeMcpInput } from "../service/mcp-input.ts";
import { configureMcpCoordinatorRoutes, createMcpCoordinator } from "../service/mcp-coordinator.ts";
import { getPackagedRuntime, runtimeExecutablePath } from "../packaging/runtime-context.ts";
import { BootstrapSessions } from "../service/bootstrap-sessions.ts";
import { ServiceShell, bootstrapShellCommands, configureServiceShellRoutes, registerServiceShellOperations } from "../service/service-shell.ts";
import { CodexClientAdapter, defaultCodexHome } from "../client-adapters/codex.ts";
import { configureWorkspaceCardRoutes } from "../service/workspace-card-routes.ts";
import { inspectUnifiedData } from "../service/data-status.ts";

export interface UnifiedResourcesOptions {
  vaultPath: string;
  masterKeyPath: string;
  webAssetsDirectory?: string;
  libraryWebAssetsDirectory?: string;
  candidateGenerationRoot?: string;
}

// 所有资源在统一实例锁内创建，Library 只提供进程内表格与 tenant，不启动第二个服务器。
export function unifiedResources(options: UnifiedResourcesOptions): RuntimeBootstrap {
  return async (lease, profile) => {
    lease.assertOwned();
    const active = options.candidateGenerationRoot === undefined ? await readActiveGeneration(lease) : null;
    const generationRoot = options.candidateGenerationRoot === undefined ? active?.generationRoot ?? lease.directory : await realpath(await verifyPrivateDirectory(options.candidateGenerationRoot));
    const child = relative(lease.directory, generationRoot);
    if (child === ".." || child.startsWith(`..${sep}`) || isAbsolute(child)) throw new Error("Unified generation must belong to the instance directory");
    const vaultPath = options.candidateGenerationRoot === undefined ? active?.vaultPath ?? options.vaultPath : join(generationRoot, "vault", "vault.sqlite");
    const masterKeyPath = options.candidateGenerationRoot === undefined ? active?.masterKeyPath ?? options.masterKeyPath : join(generationRoot, "masterkey");
    if (profile.originSource === "new-install" && generationRoot === lease.directory) await initializeNewInstallVault(lease, vaultPath, masterKeyPath);
    const webAssets = options.webAssetsDirectory === undefined ? undefined : await WebAssets.load(options.webAssetsDirectory);
    const libraryWebAssets = options.libraryWebAssetsDirectory === undefined ? undefined : await WebAssets.load(options.libraryWebAssetsDirectory);
    const directory = join(generationRoot, "tables");
    if (lstatSync(directory, { throwIfNoEntry: false })) await verifyPrivateDirectory(directory);
    else {
      if (profile.originSource !== "new-install" || generationRoot !== lease.directory) throw new Error("Legacy table data must be migrated before unified service startup");
      await createPrivateDirectory(directory);
    }
    const tables = await createEmbeddedTables(directory, `http://127.0.0.1:${profile.port}`);
    let ready = false;
    let initializedLifecycle: AccessLifecycle | undefined;
    let initializedBusiness: ReturnType<typeof createBusinessModule> | undefined;
    try {
      const identities = new UnifiedAccountService(vaultPath, masterKeyPath, tables.tenants);
      await assertNativeTelegramCryptography();
      if (process.platform !== "win32" && process.platform !== "darwin") throw new Error("Unified service requires Windows or macOS");
      const packaged = getPackagedRuntime();
      const accessLifecycle = new AccessLifecycle(lease, { vaultPath, masterKeyPath, appVersion: packaged ? packaged.manifest.productVersion : "0.9.0", platform: process.platform === "win32" ? "windows" : "macos", deviceLabel: hostname() }, identities);
      initializedLifecycle = accessLifecycle;
      const business = createBusinessModule(lease, identities, tables, `http://127.0.0.1:${profile.port}`, generationRoot, new PlatformVaultReader(vaultPath, masterKeyPath), profile.originSource === "new-install" && active === null && generationRoot === lease.directory);
      initializedBusiness = business;
      const buildIdentity = await readDevelopmentBuildIdentity();
      const codex = packaged ? new CodexClientAdapter({ executable: runtimeExecutablePath(), runtimeVersion: packaged.manifest.productVersion, binarySha256: buildIdentity.executable_sha256, dataDirectory: lease.directory, codexHome: defaultCodexHome(), pluginDirectory: join(lease.directory, "client-adapters", "codex") }) : undefined;
      const libraryVersion = packaged ? packaged.manifest.components.find(component => component.name === "onecrew_lite_tables")?.version ?? null : z.object({ version: z.string() }).parse(parseApplicationJson(await readFile(new URL("../../../onecrew_lite-library/package.json", import.meta.url), "utf8"))).version;
      async function componentEvidence() {
        const context = identities.contexts.currentIfActive();
        if (!context) return { library_version: libraryVersion, library_state: "ready", opencli_version: null, opencli_state: "account_required" };
        const status = await business.openCliStatus(context);
        return { library_version: libraryVersion, library_state: "ready", opencli_version: status.version, opencli_state: status.issue ?? "ready" };
      }
      let mcpCoordinator: Awaited<ReturnType<typeof createMcpCoordinator>>;
      let connectionsFlight: { context: AccountContext; started: number; promise: Promise<{ browser: "ready" | "not_connected" | "unavailable"; library: "ready" | "unavailable"; checked_at: number }> } | undefined;
      async function workspaceConnections() {
        const context = identities.contexts.current();
        if (connectionsFlight?.context === context && performance.now() - connectionsFlight.started < 10_000) return connectionsFlight.promise;
        const promise = (async () => {
          const [browser, library] = await Promise.all([
            business.workspaceBrowserReady(context).then(value => value ? "ready" as const : "not_connected" as const, () => "unavailable" as const),
            tables.service.listTables({ user: context.principal.user, tenantId: context.principal.tenantId, baseId: context.principal.baseId }, { page: 1, size: 1 }).then(() => libraryWebAssets ? "ready" as const : "unavailable" as const, () => "unavailable" as const),
          ]);
          identities.contexts.assertCurrent(context);
          return { browser, library, checked_at: Math.floor(Date.now() / 1000) };
        })();
        connectionsFlight = { context, started: performance.now(), promise };
        return promise;
      }
      async function workspaceActivity() {
        const context = identities.contexts.currentIfActive();
        if (!context) return { workspace_activity: null, workspace_history: [], workspace_activity_error: null };
        const activity = await business.discoveryServices.runtime.workspaceActivity(context);
        identities.contexts.assertCurrent(context);
        return { workspace_activity: activity, workspace_history: business.discoveryServices.runtime.workspaceHistory(context), workspace_activity_error: null };
      }
      const shell = new ServiceShell({ lease, access: accessLifecycle, sessions: business.mcpSessions, buildIdentity, directory: () => mcpCoordinator.directory(), componentEvidence, workspaceConnections, activity: workspaceActivity, dataStatus: () => inspectUnifiedData({ lease, generationRoot, vaultPath, originSource: profile.originSource, candidate: options.candidateGenerationRoot !== undefined, ...(active ? { activation: active } : {}) }), ...(codex ? { pluginStatus: (refresh: boolean) => codex.status(refresh) } : {}) });
      registerServiceShellOperations(business.operations, shell);
      mcpCoordinator = await createMcpCoordinator({ registry: business.operations, sessions: business.mcpSessions, results: business.results, authorize: business.authorizeMcp, connections: business.mcpConnections, requireCompleteCatalogue: true });
      const bootstrapSessions = new BootstrapSessions();
      const resourceIdentity = Object.freeze({ ...buildIdentity, data_paths_sha256: dataPathIdentity({ dataDirectory: resolve(lease.directory), generationRoot, vaultPath: resolve(vaultPath), masterKeyPath: resolve(masterKeyPath), tablesDirectory: directory }), candidate: options.candidateGenerationRoot !== undefined, profile_port: profile.port });
      const boundRequests = new WeakMap<Request, AccountContext>();
      const tableRoutes = tables.createRoutes(async request => {
        const selected = boundRequests.get(request);
        if (!selected) throw new Error("Table request has no verified account context");
        identities.contexts.assertCurrent(selected);
        return { user: selected.principal.user, tenantId: selected.principal.tenantId, baseId: selected.principal.baseId };
      });
      const prefix = "/api/v1/dynamic-tables";
      async function handleTable(context: Context): Promise<Response> {
        await identities.refresh();
        const selected = identities.contexts.forSession(context.req.header("x-onecrew-account-id") ?? "", context.req.header("x-onecrew-auth-epoch") ?? "");
        const url = new URL(context.req.url);
        url.pathname = url.pathname.slice(prefix.length) || "/";
        const signal = AbortSignal.any([context.req.raw.signal, selected.signal]);
        const request = new Request(new Request(url, context.req.raw), { signal });
        boundRequests.set(request, selected);
        try {
          const response = await tableRoutes.fetch(request);
          identities.contexts.assertCurrent(selected);
          // 账号切换也取消持续事件流，旧账号数据不会继续写入新页面连接。
          const body = response.body?.pipeThrough(new TransformStream<Uint8Array, Uint8Array>({
            transform(chunk, controller) {
              identities.contexts.assertCurrent(selected);
              controller.enqueue(chunk);
            },
          }), { signal });
          return new Response(body ?? null, { status: response.status, statusText: response.statusText, headers: response.headers });
        } finally { boundRequests.delete(request); }
      }
      ready = true;
      return {
        ...business,
        bootstrapSessions,
        bootstrapCommands: bootstrapShellCommands,
        executeBootstrap: (name, input) => shell.executeBootstrap(name, input),
        async promoteBootstrap(flowId) {
          const expected = accessLifecycle.completedPairingContext(flowId);
          return expected ? business.issueBrowserTicket(expected) : undefined;
        },
        async issueBrowserTicket() {
          const access = await accessLifecycle.status();
          if (access.state === "signed_in") return business.issueBrowserTicket();
          if (access.state === "signed_out" || access.state === "authorizing") return bootstrapSessions.issueTicket();
          throw Object.assign(new Error("Access is unavailable"), { code: "access.unavailable", status: 503 });
        },
        stopAcceptingWork() { bootstrapSessions.clear(); business.stopAcceptingWork(); },
        resourceIdentity,
        ...(webAssets ? { webAssets } : {}),
        ...(libraryWebAssets ? { libraryWebAssets } : {}),
        configureRoutes(app) {
          configureServiceShellRoutes(app, shell);
          configureWorkspaceCardRoutes(app, business.workspaceCards, async () => {
            const access = await accessLifecycle.status();
            if (access.state === "unavailable") throw Object.assign(new Error("access.unavailable"), { code: "access.unavailable", status: 503 });
            if (access.state !== "signed_in") return { accountId: null, campaignId: null, chinese: shell.locale() === "zh-CN" };
            const selected = identities.contexts.current();
            if (access.user?.id !== selected.principal.user.id) throw Object.assign(new Error("access.account_changed"), { code: "access.account_changed", status: 409 });
            const status = await shell.status(); identities.contexts.assertCurrent(selected);
            return { accountId: selected.principal.user.id, campaignId: status.workspace_activity?.campaign_id ?? null, chinese: shell.locale() === "zh-CN" };
          });
          app.route("/api/v1/system", createSystemRoutes());
          configureMcpCoordinatorRoutes(app, mcpCoordinator);
          app.get("/_local/mail/worker", context => context.json(business.mailWorkerStatus()));
          app.get("/_local/table-sync/worker", context => context.json(business.tableSyncStatus()));
          app.get("/api/v1/access/status", async context => {
            await identities.refresh();
            const selected = identities.contexts.forSession(context.req.header("x-onecrew-account-id") ?? "", context.req.header("x-onecrew-auth-epoch") ?? "");
            return context.json({ state: "signed_in", user: selected.principal.user });
          });
          app.post("/api/v1/access/logout", async context => {
            await identities.refresh();
            identities.contexts.forSession(context.req.header("x-onecrew-account-id") ?? "", context.req.header("x-onecrew-auth-epoch") ?? "");
            await accessLifecycle.logout(); return context.json(await accessLifecycle.status());
          });
          app.post("/_local/mcp/review/prepare", async context => {
            const input = z.object({ session_id: z.string(), call: z.unknown() }).strict().parse(parseApplicationJson(new Uint8Array(await context.req.arrayBuffer())));
            await business.authorizeMcp(input.session_id);
            return context.body(stringifyApplicationJson(await business.results.prepare(input.session_id, input.call)), 200, { "Content-Type": "application/json; charset=UTF-8" });
          });
          app.post("/_local/mcp/result/present", async context => {
            const input = z.object({ session_id: z.string(), result_ref: z.string(), locale: z.enum(["en-US", "zh-CN"]).default("en-US") }).strict().parse(parseApplicationJson(new Uint8Array(await context.req.arrayBuffer())));
            await business.authorizeMcp(input.session_id);
            const snapshot = business.results.present(input.session_id, input.result_ref, input.locale);
            return context.body(stringifyApplicationJson({ kind: "snapshot", snapshot }), 200, { "Content-Type": "application/json; charset=UTF-8" });
          });
          app.post("/_local/mcp/result/state", async context => {
            const input = z.object({ session_id: z.string(), input: z.record(z.string(), z.unknown()) }).strict().parse(parseApplicationJson(new Uint8Array(await context.req.arrayBuffer())));
            await business.authorizeMcp(input.session_id);
            const stateInput = Object.hasOwn(input.input, "application_json") ? decodeMcpInput(input.input) : input.input;
            return context.body(stringifyApplicationJson(await business.results.state(input.session_id, stateInput)), 200, { "Content-Type": "application/json; charset=UTF-8" });
          });
          app.post("/_local/mcp/session", async context => {
            const input = z.object({ surface: mcpSurface }).strict().parse(parseApplicationJson(new Uint8Array(await context.req.arrayBuffer())));
            return context.json(await business.issueMcpSession(input.surface));
          });
          app.post("/_local/mcp/session/close", async context => {
            const input = z.object({ session_id: z.string() }).strict().parse(parseApplicationJson(new Uint8Array(await context.req.arrayBuffer())));
            business.mcpSessions.revoke(input.session_id);
            return context.json({ closed: true });
          });
          app.get("/_local/access/status", async context => context.json(await accessLifecycle.status()));
          app.post("/_local/access/login", async context => context.json(await accessLifecycle.begin()));
          app.post("/_local/access/cancel", async context => { await accessLifecycle.cancel(); return context.json(await accessLifecycle.status()); });
          app.post("/_local/access/logout", async context => { await accessLifecycle.logout(); return context.json(await accessLifecycle.status()); });
          app.all(prefix, handleTable);
          app.all(`${prefix}/*`, handleTable);
          app.get("/api/events", async context => {
            await identities.refresh();
            const selected = context.req.header("authorization")
              ? identities.contexts.forSession(context.req.header("x-onecrew-account-id") ?? "", context.req.header("x-onecrew-auth-epoch") ?? "")
              : business.browserSessions.authorize(getCookie(context, browserCookieName(context.req.url)), undefined, false);
            return streamSSE(context, async output => {
              const cancellation = new AbortController();
              const reader = business.changes.subscribe(selected, AbortSignal.any([context.req.raw.signal, cancellation.signal])).getReader();
              output.onAbort(() => cancellation.abort());
              let writeFailure: unknown;
              const heartbeat = setInterval(() => {
                void output.write(": keep-alive\n\n").then(undefined, error => { writeFailure = error; cancellation.abort(error); });
              }, 15_000);
              try {
                for (;;) {
                  const item = await reader.read();
                  if (item.done) break;
                  await output.writeSSE({ data: JSON.stringify(item.value) });
                }
                if (writeFailure) throw writeFailure;
              } finally {
                clearInterval(heartbeat);
                cancellation.abort();
                reader.releaseLock();
              }
            });
          });
        },
        async close() {
          bootstrapSessions.clear();
          try { await accessLifecycle.close(); }
          finally {
            try { await business.close(); }
            finally {
              identities.invalidate();
              await tables.close();
            }
          }
        },
      };
    } finally {
      if (!ready) {
        try { await initializedLifecycle?.close(); }
        finally { try { await initializedBusiness?.close(); } finally { await tables.close(); } }
      }
    }
  };
}
