import type { Hono } from "hono";
import { z } from "zod";
import type { OperationRegistry } from "../application/operation-registry.ts";
import type { OwnerResults } from "../application/owner-results.ts";
import { ownerOperationRevisions } from "../application/owner-results.ts";
import { parseApplicationJson, stringifyApplicationJson } from "../contracts/application-json.ts";
import { McpSessions } from "./mcp-sessions.ts";
import { loadCurrentMcpRegistry, McpCatalogue } from "./mcp-catalogue.ts";
import { McpCatalogueSearch } from "./mcp-catalogue-search.ts";
import { registeredLocalMcpHandlers } from "./mcp-handlers.ts";
import { mcpConnectionListInput, mcpResultPresentInput } from "./mcp-contracts.ts";
import { retiredOperations } from "../contracts/retired-operations.ts";

export interface McpCoordinatorDependencies {
  registry: OperationRegistry;
  sessions: McpSessions;
  results: OwnerResults;
  authorize: (session: string) => Promise<unknown>;
  connections?: (session: string, toolkits: readonly string[]) => Promise<unknown>;
  requireCompleteCatalogue?: true;
}

export async function createMcpCoordinator(dependencies: McpCoordinatorDependencies) {
  const data = await loadCurrentMcpRegistry();
  const handlers = registeredLocalMcpHandlers(dependencies.registry);
  const available = [...handlers.map(handler => handler.operation_id), ...Object.keys(ownerOperationRevisions)];
  if (dependencies.requireCompleteCatalogue) {
    const actual = new Set(available), retired = new Set<string>(retiredOperations);
    const missing = data.entries.filter(entry => entry.kind === "operation" && !retired.has(entry.metadata.id) && !actual.has(entry.metadata.id)).map(entry => entry.metadata.id);
    if (missing.length) throw new Error(`mcp_handler_missing:${missing.join(",")}`);
  }
  const catalogue = new McpCatalogue(data, available);
  const search = new McpCatalogueSearch(catalogue, data);
  return Object.freeze({
    tools: (surface: Parameters<McpCatalogue["tools"]>[0]) => catalogue.tools(surface),
    directory: () => catalogue.directory("full"),
    async call(session: string, name: string, arguments_: unknown): Promise<unknown> {
      await dependencies.authorize(session);
      const selected = dependencies.sessions.authorize(session);
      if (name === "onecrew_tool_search") return search.search(selected.surface, arguments_);
      if (name === "onecrew_tool_get") return catalogue.get(selected.surface, arguments_);
      if (name === "onecrew_connection_list") {
        const input = mcpConnectionListInput.parse(arguments_);
        if (!dependencies.connections) throw Object.assign(new Error("Verified account inventory is unavailable"), { code: "mcp.connection_inventory_unavailable", status: 503 });
        return dependencies.connections(session, input.toolkits);
      }
      if (name === "onecrew_result_present") {
        const input = mcpResultPresentInput.parse(arguments_);
        return { kind: "snapshot", snapshot: dependencies.results.present(session, input.result_ref, input.locale) };
      }
      if (name === "onecrew_result_state") return dependencies.results.state(session, arguments_);
      if (name === "onecrew_tool_execute") return dependencies.results.execute(session, arguments_, catalogue, handlers);
      throw Object.assign(new Error("MCP tool is unavailable"), { code: "mcp.tool_unavailable", status: 404 });
    },
  });
}
export type McpCoordinator = Awaited<ReturnType<typeof createMcpCoordinator>>;

export function configureMcpCoordinatorRoutes(app: Hono, coordinator: McpCoordinator): void {
  app.post("/_local/mcp/tool", async context => {
    const input = z.strictObject({ session_id: z.string(), name: z.string(), arguments: z.unknown() }).parse(parseApplicationJson(new Uint8Array(await context.req.arrayBuffer())));
    return context.body(stringifyApplicationJson(await coordinator.call(input.session_id, input.name, input.arguments)), 200, { "Content-Type": "application/json; charset=UTF-8" });
  });
}
