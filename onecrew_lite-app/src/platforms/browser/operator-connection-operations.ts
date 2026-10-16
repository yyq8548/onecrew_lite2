import { z } from "zod";
import type { AccountContext } from "../../identity/account-context.ts";
import type { OperationRegistry } from "../../application/operation-registry.ts";
import type { OperatorBrowserConnections } from "./operator-browser-connection.ts";
import type { TelegramConnectionRuntime } from "./telegram-connection.ts";
import { chromeStatus } from "./chrome-host.ts";
import { proxyConfigSchema } from "./proxy.ts";

export const browserConnectionOperationIds = ["x", "instagram"].flatMap(platform => ["browser.start", "browser.status", "browser.complete", "browser.cancel", "reconnect.prepare", "reconnect.begin", "reconnect.status", "reconnect.finalize", "reconnect.cancel"].map(operation => `${platform}.connection.${operation}`));

export function registerOperatorConnectionOperations(registry: OperationRegistry, browser: OperatorBrowserConnections, telegram: TelegramConnectionRuntime) {
  const wrapped = z.strictObject({ input: z.unknown() });
  const select = (raw: unknown) => { const { flow_id } = z.object({ flow_id: z.uuid() }).parse(raw); if (browser.owns(flow_id)) return browser; if (telegram.owns(flow_id)) return telegram; throw new Error("operator account connection flow not found"); };
  registry.register("onecrew_operator_account_connection_prepare", wrapped, async (input, context) => { const { platform } = z.object({ platform: z.enum(["x", "instagram", "telegram"]) }).parse(input.input); return platform === "telegram" ? telegram.prepare(context, input.input) : browser.prepare(context, input.input); });
  for (const method of ["begin", "status", "finalize", "cancel"] as const) registry.register(`onecrew_operator_account_connection_${method}`, wrapped, async (input, context) => select(input.input)[method](context, input.input));
  registry.register("onecrew_operator_account_connection_handoff_in_app", z.strictObject({ flowId: z.uuid() }), async (input, context) => select({ flow_id: input.flowId }).handoff(context, input));
  registry.register("onecrew_operator_account_browser_status_in_app", z.strictObject({}), async () => chromeStatus());
  registry.register("onecrew_operator_account_connection_configure_browser_in_app", z.strictObject({ flowId: z.uuid(), profileName: z.string(), proxy: proxyConfigSchema }), async (input, context) => browser.configure(context, input));
  for (const platform of ["x", "instagram"] as const) {
    const operation = z.strictObject({ operation_id: z.uuid() });
    async function requirePlatform(context: AccountContext, id: string) { const status = await browser.directStatus(context, id); if (status.platform !== platform) throw new Error("browser.operation_platform_mismatch"); return status; }
    registry.register(`${platform}.connection.browser.start`, z.strictObject({ platform: z.literal(platform), profile_name: z.string().trim().min(1).max(120), proxy: proxyConfigSchema, idempotency_key: z.string().min(1).max(128) }), async (input, context) => browser.startDirect(context, input));
    registry.register(`${platform}.connection.browser.status`, operation, async (input, context) => requirePlatform(context, input.operation_id));
    registry.register(`${platform}.connection.browser.complete`, operation.extend({ login_completed: z.literal(true), idempotency_key: z.string().min(1).max(128) }), async (input, context) => { await requirePlatform(context, input.operation_id); await browser.finalize(context, { flow_id: input.operation_id, confirmed: true, idempotency_key: input.idempotency_key }); return browser.directStatus(context, input.operation_id); });
    registry.register(`${platform}.connection.browser.cancel`, operation.extend({ confirmed: z.literal(true) }), async (input, context) => { await requirePlatform(context, input.operation_id); await browser.cancel(context, { flow_id: input.operation_id, confirmed: input.confirmed }); return browser.directStatus(context, input.operation_id); });
    registry.register(`${platform}.connection.reconnect.prepare`, z.strictObject({ platform: z.literal(platform), intent: z.literal("reconnect"), method: z.literal("browser"), operator_account_id: z.uuid(), label: z.string().max(120).nullish() }), async (input, context) => browser.prepare(context, input));
    for (const method of ["begin", "status", "finalize", "cancel"] as const) {
      const flow = z.strictObject({ flow_id: z.uuid() }), schema = method === "status" ? flow : method === "cancel" ? flow.extend({ confirmed: z.literal(true) }) : flow.extend({ confirmed: z.literal(true), idempotency_key: z.string().min(1).max(128) });
      registry.register(`${platform}.connection.reconnect.${method}`, schema, async (input, context) => { const status = await browser.status(context, { flow_id: input.flow_id }); if (status.platform !== platform || status.intent !== "reconnect") throw new Error("browser.reconnect_flow_mismatch"); return browser[method](context, input); });
    }
  }
  return { owns(flowId: string) { return browser.owns(flowId) || telegram.owns(flowId); }, async prepare(context: AccountContext, input: unknown) { const { platform } = z.object({ platform: z.string() }).parse(input); return platform === "telegram" ? telegram.prepare(context, input) : browser.prepare(context, input); } };
}
