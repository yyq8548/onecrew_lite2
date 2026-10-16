import { randomUUID } from "node:crypto";
import { Server } from "@modelcontextprotocol/sdk/server/index.js";
import { StdioServerTransport } from "@modelcontextprotocol/sdk/server/stdio.js";
import { CallToolRequestSchema, ListToolsRequestSchema, ListResourcesRequestSchema, ReadResourceRequestSchema, ToolSchema } from "@modelcontextprotocol/sdk/types.js";
import { z } from "zod";
import { readVerifiedRuntime, type RuntimeDescriptor } from "./runtime.ts";
import { parseApplicationJson, stringifyApplicationJson } from "../contracts/application-json.ts";
import { decodeMcpInput } from "./mcp-input.ts";
import { encodeMcpResult } from "./mcp-result.ts";
import { loadMcpAppResources } from "./mcp-app-resources.ts";
import { loadCurrentMcpRegistry } from "./mcp-catalogue.ts";
import { mcpSurface, type McpSurface } from "./mcp-sessions.ts";
import { wireResultSnapshot } from "@onecrew/runtime-contracts/result-wire";
import { getPackagedRuntime } from "../packaging/runtime-context.ts";
import { localServiceStatus } from "@onecrew/runtime-contracts/status";
import { FrozenMcpOutputValidator, McpJsonSchemaValidator, nativeMcpOutputSchema } from "./mcp-json-schema.ts";
import { workspaceCardRequest } from "./workspace-card-requests.ts";

const noArguments = { type: "object" as const, properties: {}, additionalProperties: false };
const empty = z.strictObject({});
const accessStatus = z.object({ state: z.enum(["signed_out", "authorizing", "signed_in", "unavailable"]), user: z.object({ id: z.string(), email: z.string() }).nullable(), flowId: z.string().optional(), issue: z.string().optional() });
const localStatus = z.object({ service: z.literal("onecrew_lite"), instance_id: z.uuid(), production_ready: z.boolean(), resource_identity: z.object({ executable_sha256: z.string(), source_tree_sha256: z.string(), dependency_lock_sha256: z.string() }).nullable() });
const bootstrap = [
  { name: "onecrew_status", description: "Read actual OneCrew service, startup and Access status. Unavailable component evidence is reported explicitly.", inputSchema: noArguments },
  { name: "onecrew_login", description: "Begin Access login. Open authorize_url and wait for the user to log in; never request passwords or callback secrets.", inputSchema: noArguments },
  { name: "onecrew_login_cancel", description: "Cancel the current Access authorization.", inputSchema: noArguments },
  { name: "onecrew_logout", description: "Revoke the local Access session.", inputSchema: noArguments },
  { name: "onecrew_web", description: "Return one-use workbench and Library URLs and display their current read-only status card.", inputSchema: noArguments },
];
const sessionReply = z.strictObject({ session_id: z.string(), surface: mcpSurface, account_id: z.string(), auth_epoch: z.string() });
const workspaceReply = z.strictObject({ url: z.url(), instance_id: z.uuid() });
const workspaceCardTool = { name: "onecrew_workspace_card_request", description: "Claim a server-authored display notification for an existing status view. Grants no business authority.", inputSchema: { type: "object" as const, additionalProperties: false, required: ["view_id"], properties: { view_id: { type: "string", format: "uuid" }, manual: { type: "boolean" }, baseline_campaign_id: { type: ["string", "null"], format: "uuid" } } }, _meta: { ui: { visibility: ["app"] } } };

// stdio 只转发给持有实例锁的服务；账号、数据库、确认权威和发送队列均由该服务持有。
export async function createMcpAdapter(directory: string, surface: McpSurface = "full") {
  mcpSurface.parse(surface);
  const resources = await loadMcpAppResources(), registry = await loadCurrentMcpRegistry();
  const selected = registry.tools.find(value => value.surface.toLowerCase() === surface);
  if (!selected) throw new Error("mcp_surface_manifest_missing");
  const ui = { ui: { resourceUri: resources.uri, visibility: ["model", "app"] }, "openai/outputTemplate": resources.uri };
  const tools = [...bootstrap.map(tool => tool.name === "onecrew_web" ? { ...tool, _meta: ui } : tool), workspaceCardTool, ...selected.tools.map(raw => {
    const tool = ToolSchema.parse(raw);
    return { ...tool, ...(tool.outputSchema ? { outputSchema: nativeMcpOutputSchema(tool.outputSchema) } : {}), ...(tool.name === "onecrew_result_present" ? { _meta: ui } : {}) };
  })];
  const frozenOutputs = new FrozenMcpOutputValidator(registry, surface);
  const version = getPackagedRuntime()?.manifest.productVersion ?? "0.7.1";
  const server = new Server({ name: "onecrew_lite", version }, { capabilities: { tools: {}, resources: {} }, jsonSchemaValidator: new McpJsonSchemaValidator(), instructions: "Bootstrap: onecrew_status, onecrew_login, onecrew_login_cancel, onecrew_logout, onecrew_web. Discover only registered operations through onecrew_tool_search, get their exact contract with onecrew_tool_get, and execute at most eight calls. Present each new result_ref once with onecrew_result_present and the conversation locale. Wait for actual user confirmation; never invent bindings or a confirmed plan. MCP has no mail or social sending capability." });
  let session: { runtime: string; descriptor: RuntimeDescriptor; id: string } | undefined;
  let closingSession: Promise<void> | undefined;
  async function closeSession(): Promise<void> {
    if (closingSession) return closingSession;
    const active = session;
    session = undefined;
    if (active) {
      closingSession = control(active.descriptor, "POST", "/_local/mcp/session/close", AbortSignal.timeout(5_000), { session_id: active.id }).then(() => {});
      await closingSession;
    }
  }
  const closeProtocol = server.close.bind(server);
  server.close = async () => { await closeSession(); await closeProtocol(); };
  server.onclose = () => {
    void closeSession().catch(error => {
      process.stderr.write(`MCP control session could not be closed: ${error instanceof Error ? error.message : "mcp_session_close_failed"}\n`);
      process.exitCode = 1;
    });
  };
  async function control(runtime: RuntimeDescriptor, method: "GET" | "POST", path: string, signal: AbortSignal, body?: unknown): Promise<unknown> {
    const response = await fetch(`${runtime.url}${path}`, { method, headers: { authorization: `Bearer ${runtime.control_token}`, ...(body === undefined ? {} : { "content-type": "application/json" }) }, ...(body === undefined ? {} : { body: stringifyApplicationJson(body) }), redirect: "error", signal: AbortSignal.any([signal, AbortSignal.timeout(120_000)]) });
    const bytes = new Uint8Array(await response.arrayBuffer());
    if (bytes.byteLength > 40 * 1024 * 1024) throw new Error("mcp_service_response_too_large");
    const value = parseApplicationJson(bytes);
    if (!response.ok) {
      if (response.status === 401 || response.status === 403) session = undefined;
      const error = z.object({ error: z.object({ code: z.string().regex(/^[A-Za-z0-9._-]{1,128}$/u) }) }).safeParse(value);
      throw Object.assign(new Error(error.success ? error.data.error.code : `mcp_service_http_${response.status}`), { status: response.status });
    }
    return value;
  }
  async function businessSession(runtime: RuntimeDescriptor, signal: AbortSignal): Promise<string> {
    if (session?.runtime === runtime.instance_id) return session.id;
    const issued = sessionReply.parse(await control(runtime, "POST", "/_local/mcp/session", signal, { surface }));
    if (issued.surface !== surface) throw new Error("mcp_session_surface_mismatch");
    session = { runtime: runtime.instance_id, descriptor: runtime, id: issued.session_id }; return issued.session_id;
  }
  function ownedWorkspace(runtime: RuntimeDescriptor, raw: unknown, path: "/" | "/library"): string {
    const value = workspaceReply.parse(raw), url = new URL(value.url);
    if (value.instance_id !== runtime.instance_id || url.origin !== runtime.url || url.pathname !== path || url.search || !/^#onecrew_ticket=[A-Za-z0-9_-]+$/u.test(url.hash)) throw new Error("mcp_workspace_owner_mismatch");
    return value.url;
  }
  async function status(runtime: RuntimeDescriptor, signal: AbortSignal) {
    const [access, service, shell] = await Promise.all([control(runtime, "GET", "/_local/access/status", signal).then(value => accessStatus.parse(value)), control(runtime, "GET", "/_local/status", signal).then(value => localStatus.parse(value)), control(runtime, "GET", "/_local/service/status", signal).then(value => localServiceStatus.parse(value))]);
    if (service.instance_id !== runtime.instance_id) throw new Error("mcp_status_owner_mismatch");
    if (shell.pid !== runtime.pid || service.resource_identity && shell.binary_sha256 !== service.resource_identity.executable_sha256) throw new Error("mcp_status_component_mismatch");
    return { ...shell, service: "onecrew_lite", instance_id: runtime.instance_id, access, resource_identity: service.resource_identity, production_ready: service.production_ready };
  }
  server.setRequestHandler(ListResourcesRequestSchema, async () => resources.list());
  server.setRequestHandler(ReadResourceRequestSchema, async request => resources.read(request.params.uri));
  server.setRequestHandler(ListToolsRequestSchema, async () => ({ tools }));
  server.setRequestHandler(CallToolRequestSchema, async (request, extra) => {
    const input = decodeMcpInput(request.params.arguments ?? {}), name = request.params.name;
    if (!tools.some(tool => tool.name === name)) throw new Error("mcp_tool_not_registered");
    const runtime = await readVerifiedRuntime(directory);
    if (name === "onecrew_workspace_card_request") {
      const request = workspaceCardRequest.parse(input);
      return encodeMcpResult(await control(runtime, "POST", "/_local/mcp/workspace/request", extra.signal, request));
    }
    if (bootstrap.some(tool => tool.name === name)) {
      empty.parse(input);
      if (name === "onecrew_status") return encodeMcpResult(await status(runtime, extra.signal));
      if (name === "onecrew_web") {
        const current = await status(runtime, extra.signal);
        const workspace = ownedWorkspace(runtime, await control(runtime, "POST", "/_local/workspace", extra.signal), "/");
        const libraryStatus = z.strictObject({ ready: z.boolean(), web_assets_sha256: z.string().nullable(), instance_id: z.uuid() }).parse(await control(runtime, "GET", "/_local/library/status", extra.signal));
        if (libraryStatus.instance_id !== runtime.instance_id) throw new Error("mcp_library_owner_mismatch");
        const library = libraryStatus.ready ? ownedWorkspace(runtime, await control(runtime, "POST", "/_local/library/workspace", extra.signal), "/library") : null;
        const value = { kind: "workspace", url: workspace, library_url: library, library_error: library === null ? "workspace_library_assets_unavailable" : null, locale: current.locale, startup: current.startup.phase, workspace_activity: current.workspace_activity, workspace_activity_error: current.workspace_activity_error };
        const result = encodeMcpResult(value);
        const viewId = randomUUID();
        await control(runtime, "POST", "/_local/mcp/workspace/register", extra.signal, { view_id: viewId });
        return { ...result, _meta: { ...result._meta, "onecrew/workspace": { ...value, url: `${runtime.url}/`, library_url: library === null ? null : `${runtime.url}/library` }, viewUUID: viewId } };
      }
      session = undefined;
      if (name === "onecrew_login") {
        const result = z.strictObject({ flowId: z.string(), authorizeUrl: z.url() }).parse(await control(runtime, "POST", "/_local/access/login", extra.signal));
        return encodeMcpResult({ flow_id: result.flowId, authorize_url: result.authorizeUrl });
      }
      return encodeMcpResult(await control(runtime, "POST", name === "onecrew_logout" ? "/_local/access/logout" : "/_local/access/cancel", extra.signal));
    }
    const session_id = await businessSession(runtime, extra.signal), result = await control(runtime, "POST", "/_local/mcp/tool", extra.signal, { session_id, name, arguments: input });
    if (name === "onecrew_result_present") {
      const shown = z.object({ kind: z.literal("snapshot"), snapshot: wireResultSnapshot }).strict().parse(result), state = shown.snapshot.state;
      const output = { result_ref: shown.snapshot.binding.result_ref, status: state.kind, failure: state.kind === "failed" ? { code: state.code, summary: state.summary, retryable: state.retryable } : null };
      frozenOutputs.validate(name, output);
      return encodeMcpResult(output, shown.snapshot);
    }
    frozenOutputs.validate(name, result);
    return encodeMcpResult(result);
  });
  return server;
}

export async function serveMcpStdio(directory: string): Promise<void> {
  const server = await createMcpAdapter(directory);
  let closing = false;
  const close = () => {
    if (closing) return;
    closing = true;
    void server.close().catch(error => {
      process.stderr.write(`MCP stdio shutdown failed: ${error instanceof Error ? error.message : "mcp_shutdown_failed"}\n`);
      process.exitCode = 1;
    });
  };
  process.stdin.once("end", close);
  process.stdin.once("close", close);
  await server.connect(new StdioServerTransport());
}
