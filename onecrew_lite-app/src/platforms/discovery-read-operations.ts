import { z } from "zod";
import type { OperationRegistry } from "../application/operation-registry.ts";
import type { AccountContext, AccountContextService } from "../identity/account-context.ts";
import { discoveryIntentProposal, validateDiscoveryIntent } from "../discovery/intent.ts";
import { stringifyApplicationJson } from "../contracts/application-json.ts";
import { readLinkHubExtraction } from "./candidates/link-hub.ts";
import { VERIFIED_OPENCLI_VERSION } from "./opencli/installation.ts";
import { readWorkflowGuide, workflowGuideInput } from "../service/workflow-guides.ts";

export const discoveryReadOperationIds = ["onecrew.discovery.intent.validate", "onecrew.discovery.sources.get", "onecrew.link_hub.extract", "onecrew.workflow.guide"] as const;
const reportSchema = z.object({ version: z.string().nullable(), daemon_running: z.boolean(), extension_connected: z.boolean(), profiles: z.array(z.object({ context_id: z.string(), alias: z.string().nullable() })), search_capabilities: z.array(z.object({ platform: z.string(), command: z.string(), browser_required: z.boolean(), registered: z.boolean() })), issue: z.string().nullable() });
export interface DiscoveryReadDependencies { setupReport(context: AccountContext): Promise<unknown> }
export function projectDiscoverySources(raw: unknown) {
  const report = reportSchema.parse(raw);
  return { version: report.version, daemon_running: report.daemon_running, extension_connected: report.extension_connected, profiles: report.profiles.map(({ context_id, alias }) => ({ context_id, alias })), sources: report.search_capabilities.map(source => ({ ...source, scope_kind: "query", allowed_values: [] as string[] })), issue: report.issue };
}
export const discoveryIntentValidateInput = z.strictObject({ request: z.string(), proposal: discoveryIntentProposal });
export function registerDiscoveryReadOperations(registry: OperationRegistry, accounts: AccountContextService, dependencies: DiscoveryReadDependencies): void {
  const sources = async (context: AccountContext) => { accounts.assertCurrent(context); const result = projectDiscoverySources(await dependencies.setupReport(context)); accounts.assertCurrent(context); return result; };
  registry.register("onecrew.discovery.sources.get", z.strictObject({}), async (_input, context) => sources(context));
  registry.register("onecrew.discovery.intent.validate", discoveryIntentValidateInput, async (input, context) => {
    accounts.assertCurrent(context); if (Buffer.byteLength(stringifyApplicationJson(input)) > 32768) throw new Error("discovery_proposal_too_large");
    const inventory = await sources(context); if (inventory.version === null) throw new Error("discovery_sources_unavailable"); if (inventory.version !== VERIFIED_OPENCLI_VERSION) throw new Error("discovery_sources_version_incompatible");
    const registered = inventory.sources.filter(source => source.registered).map(source => source.platform); if (!registered.length) throw new Error("discovery_sources_unavailable");
    const issues = validateDiscoveryIntent(input.request, input.proposal, registered); return { valid: issues.length === 0, issues, registered_source_count: registered.length };
  });
  registry.register("onecrew.link_hub.extract", z.strictObject({ url: z.string() }), async ({ url }, context) => { accounts.assertCurrent(context); const extracted = await readLinkHubExtraction(url, context.signal); accounts.assertCurrent(context); return extracted; });
  registry.register("onecrew.workflow.guide", workflowGuideInput, async (input, context) => { accounts.assertCurrent(context); const guide = await readWorkflowGuide(input.topic ?? null); accounts.assertCurrent(context); return guide; });
}
