import { createHash } from "node:crypto";
import { readFile } from "node:fs/promises";
import { z } from "zod";
import { parseApplicationJson } from "../contracts/application-json.ts";
import { assertOperationAvailable } from "../contracts/retired-operations.ts";
import { runtimeAssetPath } from "../packaging/runtime-context.ts";
import { mcpSurface, type McpSurface } from "./mcp-sessions.ts";
import { mcpEffect, mcpEntryId, mcpToolkit, mcpToolGetInput } from "./mcp-contracts.ts";

const object = z.record(z.string(), z.unknown());
const metadata = z.object({ id: mcpEntryId, title: z.string(), summary: z.string(), aliases: z.array(z.string()), domain_id: z.string(), toolkit: mcpToolkit, surfaces: z.array(mcpSurface) });
const requirement = z.discriminatedUnion("kind", [z.strictObject({ kind: z.literal("none") }), z.strictObject({ kind: z.literal("operator_account"), toolkit: z.enum(["x", "instagram", "telegram"]) }), z.strictObject({ kind: z.literal("service_credential"), toolkit: z.literal("youtube") }), z.strictObject({ kind: z.literal("mailbox"), toolkit: z.literal("email") })]);
const operationEntry = z.object({ kind: z.literal("operation"), metadata, input_revision: z.string(), input_schema: object, output_revision: z.string(), output_schema: object, effect: mcpEffect, approval: z.enum(["none", "owner_guarded", "attended_prompt"]), approval_request: z.unknown(), connection: requirement, safety: object, secret: z.enum(["none", "transient", "vault_bound"]), sensitive_input_paths: z.array(z.string()), supports_search_plan: z.boolean(), view_kind: z.string() });
const playbookEntry = z.object({ kind: z.literal("playbook"), metadata, toolkit_ids: z.array(mcpToolkit), primary_operation_ids: z.array(mcpEntryId), supporting_operation_ids: z.array(mcpEntryId), recommended_steps: z.array(z.object({ instruction: z.string(), operation_ids: z.array(mcpEntryId) })), known_pitfalls: z.array(z.string()), use_case: z.string(), difficulty: z.string() });
const guideEntry = z.object({ kind: z.literal("toolkit_guide"), metadata, purpose: z.string(), connection_model: z.string(), identifier_semantics: z.array(z.string()), capability_categories: z.array(z.string()), gotchas: z.array(z.string()) });
const entry = z.discriminatedUnion("kind", [operationEntry, playbookEntry, guideEntry]);
const executionContract = z.object({ id: mcpEntryId, toolkit: mcpToolkit, title: z.string(), summary: z.string(), schema_revision: z.string(), effect: mcpEffect, approval: z.enum(["none", "owner_guarded", "attended_prompt"]), connection: requirement, input_schema: object, output_schema: object.nullable(), safety: object.optional(), secret: z.string().optional() });
const source = z.object({ entries: z.array(entry), surfaces: z.array(z.object({ surface: mcpSurface, directory: z.array(z.unknown()), get: z.array(z.object({ operations: z.array(executionContract) })) })), tools: z.array(z.object({ surface: z.string(), tools: z.array(object) })) });
export type McpOperationEntry = z.infer<typeof operationEntry>;
export type McpPlaybookEntry = z.infer<typeof playbookEntry>;
export type McpGuideEntry = z.infer<typeof guideEntry>;
export type McpExecutionContract = z.infer<typeof executionContract>;
export type McpCatalogueData = z.infer<typeof source>;
export const originalMcpRegistrySha256 = "a63a789c4e0ec45ac7ec5ae0121ee1793f5c159551ab4c68258ab20117870d50";

export async function loadOriginalMcpRegistry(): Promise<McpCatalogueData> {
  const bytes = await readFile(runtimeAssetPath("mcp-registry-data.json", new URL("./mcp-registry-data.json", import.meta.url)));
  if (createHash("sha256").update(bytes).digest("hex") !== originalMcpRegistrySha256) throw new Error("mcp_registry_source_changed");
  const data = source.parse(parseApplicationJson(bytes));
  if (new Set(data.entries.map(value => value.metadata.id)).size !== data.entries.length) throw new Error("mcp_registry_duplicate_entry");
  return data;
}
export async function loadCurrentMcpRegistry(): Promise<McpCatalogueData> {
  const data = await loadOriginalMcpRegistry();
  for (const entry of data.entries) if (entry.kind === "toolkit_guide") {
    if (entry.metadata.toolkit === "email") entry.connection_model = "Gmail mailboxes use Google OAuth through Access and an account-bound mailbox reference. Local historical drafts and templates do not authorize sending. User sending is available only through GmailBulk on the Mail page.";
    if (entry.metadata.toolkit === "instagram") entry.metadata.summary = "Read public Instagram accounts and profiles, and manage the connected operator session.";
    if (entry.metadata.toolkit === "telegram") entry.metadata.summary = "Read public Telegram communities and administrators, and manage the connected operator session.";
    if (entry.metadata.toolkit === "onecrew") entry.connection_model = "OneCrew records and historical receipts are local. Use each operation's exact input and connection contract. User sending is available only through GmailBulk on the Mail page.";
  }
  const retiredFields = new Set(["x_chat_passcode_status", "x_chat_passcode", "x_chat_preference"]);
  function removeRetiredFields(schema: unknown): void {
    if (Array.isArray(schema)) { for (const value of schema) removeRetiredFields(value); return; }
    if (schema === null || typeof schema !== "object") return;
    const value = schema as Record<string, unknown>;
    if (value.properties !== null && typeof value.properties === "object" && !Array.isArray(value.properties)) for (const name of retiredFields) delete (value.properties as Record<string, unknown>)[name];
    if (Array.isArray(value.required)) value.required = value.required.filter(name => !retiredFields.has(String(name)));
    for (const child of Object.values(value)) removeRetiredFields(child);
  }
  const revision = (schema: unknown) => `sha256:${createHash("sha256").update(JSON.stringify(schema)).digest("hex")}`;
  const requirementCriterion = { type: "object", additionalProperties: false, required: ["requirement"], properties: { requirement: { type: "integer", minimum: 0, maximum: 31 } } };
  function updateDiscoverySchema(raw: unknown): void {
    if (Array.isArray(raw)) { raw.forEach(updateDiscoverySchema); return; }
    if (raw === null || typeof raw !== "object") return;
    const schema = raw as Record<string, unknown>;
    if (schema.properties !== null && typeof schema.properties === "object" && !Array.isArray(schema.properties)) {
      const properties = schema.properties as Record<string, Record<string, unknown>>;
      if (properties.criterion?.type === "string" && Array.isArray(properties.criterion.enum) && properties.criterion.enum.includes("match_fit")) properties.criterion = { anyOf: [properties.criterion, structuredClone(requirementCriterion)] };
      if (properties.findings?.type === "array" && properties.findings.maxItems === 6) properties.findings.maxItems = 40;
    }
    Object.values(schema).forEach(updateDiscoverySchema);
  }
  for (const entry of data.entries) if (entry.kind === "operation" && entry.metadata.id.startsWith("onecrew.discovery.")) {
    if (["onecrew.discovery.search_once", "onecrew.discovery.youtube.search_once"].includes(entry.metadata.id)) {
      const properties = entry.input_schema.properties as Record<string, unknown>;
      properties.recovery = { anyOf: [{ type: "null" }, { type: "object", additionalProperties: false, required: ["expected_stop_code", "expected_stopped_at"], properties: { expected_stop_code: { type: "string", minLength: 1 }, expected_stopped_at: { type: "integer", minimum: Number.MIN_SAFE_INTEGER, maximum: Number.MAX_SAFE_INTEGER } } }] };
    }
    for (const direction of ["input", "output"] as const) {
      const schema = entry[`${direction}_schema`];
      updateDiscoverySchema(schema);
      entry[`${direction}_revision`] = revision(schema);
    }
    for (const surface of data.surfaces) for (const group of surface.get) for (const operation of group.operations) if (operation.id === entry.metadata.id) {
      operation.schema_revision = entry.input_revision;
      operation.input_schema = structuredClone(entry.input_schema);
      operation.output_schema = structuredClone(entry.output_schema);
    }
  }
  for (const entry of data.entries) if (entry.kind === "operation" && /^(?:x|instagram|telegram)\.connection\./u.test(entry.metadata.id)) {
    for (const direction of ["input", "output"] as const) {
      const schema = entry[`${direction}_schema`], before = JSON.stringify(schema);
      removeRetiredFields(schema);
      if (JSON.stringify(schema) !== before) entry[`${direction}_revision`] = revision(schema);
    }
    entry.sensitive_input_paths = entry.sensitive_input_paths.filter(path => ![...retiredFields].some(name => path.split("/").includes(name)));
    for (const surface of data.surfaces) for (const group of surface.get) for (const operation of group.operations) if (operation.id === entry.metadata.id) {
      operation.schema_revision = entry.input_revision;
      operation.input_schema = structuredClone(entry.input_schema);
      operation.output_schema = structuredClone(entry.output_schema);
    }
  }
  return data;
}

export class McpCatalogue {
  readonly #data: McpCatalogueData;
  readonly #available: Set<string>;
  constructor(data: McpCatalogueData, actualOperationIds: Iterable<string>) {
    this.#data = data; this.#available = new Set(actualOperationIds);
    for (const id of this.#available) { assertOperationAvailable(id); if (!data.entries.some(value => value.kind === "operation" && value.metadata.id === id)) throw new Error("mcp_handler_contract_missing"); }
  }
  operation(id: string, surface: McpSurface): McpOperationEntry | undefined {
    if (!this.#available.has(id)) return undefined;
    const selected = this.#data.entries.find(value => value.kind === "operation" && value.metadata.id === id && value.metadata.surfaces.includes(surface));
    return selected?.kind === "operation" ? structuredClone(selected) : undefined;
  }
  operations(surface: McpSurface): McpOperationEntry[] {
    return this.#data.entries.filter((value): value is McpOperationEntry => value.kind === "operation" && this.#available.has(value.metadata.id) && value.metadata.surfaces.includes(surface)).map(value => structuredClone(value));
  }
  playbooks(surface: McpSurface): McpPlaybookEntry[] {
    return this.#data.entries.filter((value): value is McpPlaybookEntry => value.kind === "playbook" && value.metadata.surfaces.includes(surface) && [...value.primary_operation_ids, ...value.supporting_operation_ids, ...value.recommended_steps.flatMap(step => step.operation_ids)].every(id => this.operation(id, surface) !== undefined)).map(value => structuredClone(value));
  }
  guides(surface: McpSurface): McpGuideEntry[] {
    const toolkits = new Set(this.operations(surface).map(value => value.metadata.toolkit));
    return this.#data.entries.filter((value): value is McpGuideEntry => value.kind === "toolkit_guide" && value.metadata.surfaces.includes(surface) && toolkits.has(value.metadata.toolkit)).map(value => structuredClone(value));
  }
  directory(surface: McpSurface) {
    const operations = this.operations(surface);
    return this.guides(surface).map(guide => ({ id: guide.metadata.id, toolkit: guide.metadata.toolkit, title: guide.metadata.title, summary: guide.metadata.summary, purpose: guide.purpose, connection_model: guide.connection_model, identifier_semantics: guide.identifier_semantics, capability_categories: guide.capability_categories, gotchas: guide.gotchas, operations: operations.filter(value => value.metadata.toolkit === guide.metadata.toolkit).map(value => ({ id: value.metadata.id, title: value.metadata.title, summary: value.metadata.summary })) }));
  }
  get(surface: McpSurface, raw: unknown) {
    const input = mcpToolGetInput.parse(raw);
    const operations = input.operation_ids.map(id => {
      if (!this.operation(id, surface)) throw new Error("operation_unavailable");
      const value = this.#data.surfaces.find(value => value.surface === surface)?.get.flatMap(value => value.operations).find(value => value.id === id);
      if (!value) throw new Error("mcp_operation_contract_missing");
      return { ...structuredClone(value), output_schema: input.include_output_schema ? structuredClone(value.output_schema) : null };
    });
    return { operations };
  }
  tools(surface: McpSurface): Record<string, unknown>[] {
    const selected = this.#data.tools.find(value => value.surface.toLowerCase() === surface);
    if (!selected) throw new Error("mcp_surface_manifest_missing");
    return structuredClone(selected.tools);
  }
}
