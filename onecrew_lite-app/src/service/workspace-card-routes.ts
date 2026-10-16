import type { Hono } from "hono";
import { z } from "zod";
import { parseApplicationJson } from "../contracts/application-json.ts";
import { WorkspaceCardRequests, workspaceCardRequest } from "./workspace-card-requests.ts";

export interface WorkspaceCardState { accountId: string | null; campaignId: string | null; chinese: boolean }
export function configureWorkspaceCardRoutes(app: Hono, cards: WorkspaceCardRequests, readState: () => Promise<WorkspaceCardState>): void {
  app.post("/_local/mcp/workspace/register", async context => {
    const input = z.strictObject({ view_id: z.uuid() }).parse(parseApplicationJson(new Uint8Array(await context.req.arrayBuffer())));
    const state = await readState();
    cards.register(input.view_id, state.accountId, state.campaignId);
    return context.json({ registered: true });
  });
  app.post("/_local/mcp/workspace/request", async context => {
    const input = workspaceCardRequest.parse(parseApplicationJson(new Uint8Array(await context.req.arrayBuffer())));
    const state = await readState();
    return context.json(state.accountId === null || state.campaignId === null ? { kind: "none" as const } : cards.request(input, state.accountId, state.campaignId, state.chinese));
  });
}
