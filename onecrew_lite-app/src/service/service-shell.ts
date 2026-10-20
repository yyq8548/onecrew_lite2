import { z } from "zod";
import type { Hono } from "hono";
import type { OperationRegistry } from "../application/operation-registry.ts";
import type { AccessLifecycle, AccessLifecycleStatus } from "../identity/access-lifecycle.ts";
import type { InstanceLease } from "./instance-lock.ts";
import type { McpSessions } from "./mcp-sessions.ts";
import { dataStartupStatus, localServiceStatus, loginProjection, type DataStartupStatus, type LocalServiceStatus } from "@onecrew/runtime-contracts/status";
import { componentsReport } from "@onecrew/runtime-contracts/discovery";
import { getPackagedRuntime } from "../packaging/runtime-context.ts";
import { stringifyApplicationJson } from "../contracts/application-json.ts";
import { bootstrapCommands as bootstrapShellCommands } from "@onecrew/runtime-contracts/bootstrap";

export interface ServiceShellOptions {
  lease: InstanceLease;
  access: AccessLifecycle;
  sessions: McpSessions;
  buildIdentity: Readonly<{ executable_sha256: string; source_tree_sha256: string; dependency_lock_sha256: string }>;
  directory: () => unknown;
  pluginStatus?: (refresh: boolean) => Promise<LocalServiceStatus["plugin"]>;
  dataStatus?: () => Promise<DataStartupStatus>;
  activity?: () => Promise<Pick<LocalServiceStatus, "workspace_activity" | "workspace_history" | "workspace_activity_error">>;
  componentEvidence?: () => Promise<{ library_version: string | null; library_state: string; opencli_version: string | null; opencli_state: string }>;
  workspaceConnections?: () => Promise<NonNullable<LocalServiceStatus["workspace_connections"]>>;
}
export { bootstrapShellCommands };

// 状态读取不安装组件、不批准任务；未取得的组件证据保留明确的 unavailable。
export class ServiceShell {
  readonly #options: ServiceShellOptions;
  readonly version = getPackagedRuntime()?.manifest.productVersion ?? "0.9.0";
  #locale: "en-US" | "zh-CN" = "en-US";
  constructor(options: ServiceShellOptions) {
    if (!/^[a-f0-9]{64}$/u.test(options.buildIdentity.executable_sha256)) throw new Error("service_build_identity_invalid");
    this.#options = options;
  }
  locale() { return this.#locale; }
  setLocale(input: unknown): null { this.#options.lease.assertOwned(); this.#locale = z.enum(["en-US", "zh-CN"]).parse(input); return null; }
  async pluginStatus(refresh = false): Promise<LocalServiceStatus["plugin"]> {
    this.#options.lease.assertOwned();
    const value = this.#options.pluginStatus ? await this.#options.pluginStatus(refresh) : { state: "unavailable", verification: "not_checked", installed_version: null, enabled: null, last_verified_at: null, diagnostic_code: "codex_plugin_inspection_unavailable" };
    this.#options.lease.assertOwned(); return localServiceStatus.shape.plugin.parse(value);
  }
  async dataStatus(): Promise<DataStartupStatus> {
    this.#options.lease.assertOwned();
    const value = this.#options.dataStatus ? await this.#options.dataStatus() : { phase: "ready", diagnostic_code: "data_preflight_unavailable", failure: null, preflight: null, postflight: null, upgrade: null };
    this.#options.lease.assertOwned(); return dataStartupStatus.parse(value);
  }
  #login(access: AccessLifecycleStatus) {
    if (access.state === "unavailable") return null;
    const evidence = this.#options.access.pairingEvidence();
    return loginProjection.parse({ browser_opened: false, callback_received: evidence.callbackReceived, flow_id: access.flowId ?? evidence.flowId,
      issue: access.issue === "access.pairing_cancelled" ? "canceled" : access.issue === "access.pairing_expired" ? "callback_expired" : null,
      status: { state: access.state === "signed_in" ? "active" : access.state === "authorizing" ? "pairing_pending" : "pairing_required",
        action: access.state === "signed_in" ? null : access.state === "authorizing" ? "await_pairing_callback" : "begin_pairing", credential_id: access.credentialId ?? null, installation_id: access.installationId ?? null,
        issue: access.issue === "access.pairing_cancelled" ? "pairing_cancelled" : access.issue === "access.pairing_expired" ? "pairing_expired" : null } });
  }
  async status(): Promise<LocalServiceStatus> {
    this.#options.lease.assertOwned();
    const [access, plugin, data, activity] = await Promise.all([this.#options.access.status(), this.pluginStatus(), this.dataStatus(), this.#options.activity?.() ?? { workspace_activity: null, workspace_history: [], workspace_activity_error: "workspace_activity_unavailable" }]);
    this.#options.lease.assertOwned();
    return localServiceStatus.parse({ version: this.version, binary_sha256: this.#options.buildIdentity.executable_sha256, pid: process.pid, running: true, transport: "local_http", mcp_clients: this.#options.sessions.activeSessionCount(), locale: this.#locale,
      startup: { phase: data.phase, ...(data.diagnostic_code ? { diagnostic_code: data.diagnostic_code } : {}), ...(data.failure ? { failure: data.failure } : {}) }, data: data.postflight ?? data.preflight,
      login: this.#login(access),
      login_error: access.issue ?? null, plugin,
      cli_update: { state: "unavailable", current_version: this.version, latest_version: null, checked_at: null, source: null, diagnostic_code: "workspace_update_required" },
      next_actions: [{ kind: data.phase !== "ready" ? data.phase === "failed" ? "inspect_data" : "await_startup" : access.state === "signed_in" ? "open_web" : access.state === "authorizing" ? "await_login" : "login", command: null, tool: data.phase !== "ready" ? "onecrew_status" : access.state === "signed_in" ? "onecrew_web" : access.state === "authorizing" ? "onecrew_status" : "onecrew_login" }], ...activity });
  }
  async components() {
    const plugin = await this.pluginStatus(true), evidence = await this.#options.componentEvidence?.();
    this.#options.lease.assertOwned();
    return componentsReport.parse({ version: this.version, built_at: getPackagedRuntime()?.manifest.builtAt ?? 0, binary_sha256: this.#options.buildIdentity.executable_sha256, checked_at: Math.floor(Date.now() / 1000), plugin_version: plugin.installed_version, plugin_state: plugin.state === "installed" && plugin.enabled === true && plugin.diagnostic_code === null ? "ready" : "needs_attention", library_version: evidence?.library_version ?? null, library_state: evidence?.library_state ?? "unavailable", opencli_version: evidence?.opencli_version ?? null, opencli_state: evidence?.opencli_state ?? "unavailable" });
  }
  async workspaceConnections() {
    this.#options.lease.assertOwned();
    const access = await this.#options.access.status();
    const value = access.state === "signed_in" && this.#options.workspaceConnections ? await this.#options.workspaceConnections() : { browser: "account_required", library: "account_required", checked_at: Math.floor(Date.now() / 1000) };
    this.#options.lease.assertOwned();
    return localServiceStatus.shape.workspace_connections.unwrap().parse(value);
  }
  async logout() { this.#options.lease.assertOwned(); await this.#options.access.logout(); return this.#options.access.status(); }
  directory() { this.#options.lease.assertOwned(); return this.#options.directory(); }
  async executeBootstrap(name: string, raw: unknown): Promise<unknown> {
    this.#options.lease.assertOwned();
    if (!(bootstrapShellCommands as readonly string[]).includes(name)) throw Object.assign(new Error("bootstrap_command_denied"), { code: "bootstrap.command_denied", status: 403 });
    if (name === "set_application_locale") { const input = z.strictObject({ locale: z.enum(["en-US", "zh-CN"]) }).parse(raw); return this.setLocale(input.locale); }
    z.strictObject({}).parse(raw);
    if (name === "service_status") return this.status();
    if (name === "onecrew_components_check") return this.components();
    if (name === "onecrew_codex_plugin_status_refresh") return this.pluginStatus(true);
    if (name === "local_data_status") return this.dataStatus();
    if (name === "creator_sync_pairing_begin" || name === "creator_sync_pairing_reopen") {
      const started = name === "creator_sync_pairing_begin" ? await this.#options.access.begin() : await this.#options.access.reopen();
      const projection = loginProjection.parse((await this.status()).login);
      if (projection.flow_id !== started.flowId || projection.status.state !== "pairing_pending") throw new Error("access_pairing_owner_changed");
      return { authorize_url: started.authorizeUrl, projection };
    }
    if (name === "creator_sync_pairing_cancel") await this.#options.access.cancel();
    const projection = (await this.status()).login;
    if (projection === null) throw Object.assign(new Error("access_pairing_status_unavailable"), { code: "access.pairing_status_unavailable", status: 503 });
    return projection;
  }
}
export function registerServiceShellOperations(registry: OperationRegistry, shell: ServiceShell): void {
  const empty = z.strictObject({});
  registry.register("service_status", empty, async () => shell.status());
  registry.register("set_application_locale", z.strictObject({ locale: z.enum(["en-US", "zh-CN"]) }), async ({ locale }) => shell.setLocale(locale));
  registry.register("onecrew_components_check", empty, async () => shell.components());
  registry.register("onecrew_logout", empty, async () => shell.logout());
  registry.register("onecrew_codex_plugin_status_refresh", empty, async () => shell.pluginStatus(true));
  registry.register("local_data_status", empty, async () => shell.dataStatus());
  registry.register("toolkit_directory", empty, async () => shell.directory());
  for (const name of ["creator_sync_pairing_status", "creator_sync_pairing_begin", "creator_sync_pairing_reopen", "creator_sync_pairing_cancel"] as const) registry.register(name, empty, async input => shell.executeBootstrap(name, input));
}
export function configureServiceShellRoutes(app: Hono, shell: ServiceShell): void {
  app.get("/_local/workspace/connections", async context => context.json(await shell.workspaceConnections()));
  app.get("/_local/service/status", async context => context.body(stringifyApplicationJson(await shell.status()), 200, { "Content-Type": "application/json; charset=UTF-8" }));
}
