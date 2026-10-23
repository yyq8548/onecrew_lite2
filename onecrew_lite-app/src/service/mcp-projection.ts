import { stringifyApplicationJson } from "../contracts/application-json.ts";
import { presentationProjection, type ExecutedCallOutcome, type PresentationProjection } from "../results/contracts.ts";
import type { McpOperationEntry } from "./mcp-catalogue.ts";

type Field = { label: string; value: string };
type Section = PresentationProjection["sections"][number];
type Path = (string | number)[];
interface Fields { fields: Field[]; truncated: number; withheld: number }
const bytes = (value: string) => Buffer.byteLength(value);
const sorted = (value: Record<string, unknown>) => Object.keys(value).sort((a, b) => Buffer.compare(Buffer.from(a), Buffer.from(b)));
const object = (value: unknown): value is Record<string, unknown> => value !== null && typeof value === "object" && !Array.isArray(value);
const titleKeys = ["display_name", "name", "title", "subject", "handle", "username", "screen_name", "full_name", "text", "id"];
export function mcpSafeFieldKey(key: string): boolean {
  const normalized = key.replace(/[^A-Za-z0-9]/gu, "").toLowerCase();
  return normalized !== "raw" && !["secret", "password", "passwd", "token", "cookie", "authorization", "apikey", "privatekey", "clientsecret", "rawpayload", "rawresponse", "rawrequest"].some(term => normalized.includes(term));
}
function label(path: Path): string {
  let value = "";
  for (const part of path) {
    if (typeof part === "number") { value += `[${part + 1}]`; continue; }
    const words = part.split(/[^A-Za-z0-9]/u).filter(Boolean).map(word => word[0]!.toUpperCase() + word.slice(1));
    if (!words.length) throw new Error("projection_unnameable_field");
    value += `${value ? " · " : ""}${words.join(" ")}`;
  }
  value ||= "Result";
  if (bytes(value) > 8192) throw new Error("projection_bounds");
  return value;
}
function scalar(value: unknown): string {
  let text: string;
  if (typeof value === "string") text = !value.length || /[\p{Cc}]/u.test(value.replace(/[\n\t]/gu, "")) ? stringifyApplicationJson(value) : value;
  else if (value === null || typeof value === "boolean" || typeof value === "number" || typeof value === "bigint") text = stringifyApplicationJson(value);
  else throw new Error("projection_encoding");
  if (!text.length || bytes(text) > 8192) throw new Error("projection_bounds");
  return text;
}
export function mcpExactFields(value: unknown, exact: boolean, maximum = 128): Fields {
  const result: Fields = { fields: [], truncated: 0, withheld: 0 }, labels = new Set<string>();
  const push = (path: Path, text: string) => {
    const name = label(path);
    if (labels.has(name)) throw new Error("projection_label_collision");
    labels.add(name);
    if (result.fields.length >= maximum) { if (exact) throw new Error("projection_bounds"); result.truncated++; }
    else result.fields.push({ label: name, value: text });
  };
  const walk = (node: unknown, path: Path, depth: number): void => {
    if (depth > 16) throw new Error("projection_bounds");
    if (Array.isArray(node)) {
      if (!node.length) push(path, "[]");
      else node.forEach((child, index) => walk(child, [...path, index], depth + 1));
    } else if (object(node)) {
      const keys = sorted(node);
      if (!keys.length) push(path, "{}");
      for (const key of keys) {
        if (!mcpSafeFieldKey(key)) { if (exact) throw new Error("projection_unsafe_field"); result.withheld++; continue; }
        walk(node[key], [...path, key], depth + 1);
      }
    } else push(path, scalar(node));
  };
  walk(value, [], 0);
  if (!result.fields.length) throw new Error("projection_bounds");
  return result;
}
function bounded(value: string, maximum: number): string { let result = ""; for (const character of value) { if (bytes(result + character) > maximum) break; result += character; } return result; }
function named(record: Record<string, unknown>): { path: Path; value: string } | null {
  const text = (value: unknown): value is string => typeof value === "string" && !!value.trim();
  for (const key of titleKeys) if (text(record[key])) return { path: [key], value: record[key] };
  for (const key of titleKeys) {
    const matches = sorted(record).filter(parent => mcpSafeFieldKey(parent) && object(record[parent]) && text(record[parent][key]));
    if (matches.length > 1) return null;
    if (matches.length === 1) { const parent = matches[0]!, child = record[parent]; if (object(child) && text(child[key])) return { path: [parent, key], value: child[key] }; }
  }
  return null;
}
function locate(value: unknown): string[] | null {
  const records = (node: unknown): node is Record<string, unknown>[] => Array.isArray(node) && node.length >= 2 && node.every(object);
  if (records(value)) return [];
  if (!object(value)) return null;
  const keys = sorted(value).filter(mcpSafeFieldKey), direct = keys.filter(key => records(value[key]));
  if (direct.length) return direct.length === 1 ? [direct[0]!] : null;
  const nested = keys.flatMap(key => { const child = value[key]; return object(child) ? sorted(child).filter(inner => mcpSafeFieldKey(inner) && records(child[inner])).map(inner => [key, inner]) : []; });
  return nested.length === 1 ? nested[0]! : null;
}
const notice = (body: string, heading: string | null = null): Section => ({ kind: "notice", heading, body, tone: "neutral" });
function omissions(value: Fields, shown = value.fields.length): Section[] {
  return [...(value.truncated ? [notice(`Showing ${shown} of ${shown + value.truncated} values. The complete result was returned with this call.`)] : []), ...(value.withheld ? [notice(`${value.withheld} field(s) are not shown here because their names match the rule for values this card never displays. The complete result was returned with this call.`)] : [])];
}
function prune(value: unknown, path: readonly string[]): unknown {
  if (!path.length || !object(value)) return null;
  const [head, ...tail] = path, result: Record<string, unknown> = {};
  for (const key of sorted(value)) {
    if (key !== head) result[key] = value[key];
    else if (tail.length) { const child = prune(value[key], tail); if (!object(child) || Object.keys(child).length) result[key] = child; }
  }
  return result;
}
function outputSections(value: unknown, title: string, maximum: number, detailBytes: number, maximumFields = maximum): Section[] {
  const path = locate(value);
  if (path === null) { const fields = mcpExactFields(value, false, maximumFields); return [{ kind: "fields", heading: title, fields: fields.fields }, ...omissions(fields)]; }
  let records = value;
  for (const key of path) { if (!object(records)) throw new Error("projection_encoding"); records = records[key]; }
  if (!Array.isArray(records)) throw new Error("projection_encoding");
  const items: { title: string; description: string | null }[] = [];
  let used = 0, closed = false, withheld = 0;
  records.forEach((record: unknown, index) => {
    if (!object(record)) throw new Error("projection_encoding");
    const name = named(record), fields = mcpExactFields(record, false), skipped = name ? label(name.path) : null;
    const collapsed = name?.value.trim().split(/\s+/u).join(" ") ?? `[${index + 1}]`, itemTitle = bytes(collapsed) > 256 ? `${bounded(collapsed, 256)}…` : collapsed;
    let detail = "";
    for (const field of fields.fields) {
      if (field.label === skipped) continue;
      const line = `${field.label}: ${field.value}`;
      if (bytes(detail) + bytes(line) + 1 > detailBytes) { detail += "…"; break; }
      detail += `${detail ? "\n" : ""}${line}`;
    }
    const size = bytes(itemTitle) + bytes(detail);
    if (closed || items.length >= maximum) return;
    if (items.length && used + size > 48 * 1024) { closed = true; return; }
    used += size; withheld += fields.withheld; items.push({ title: itemTitle, description: detail || null });
  });
  const lead = path.length ? prune(value, path) : null;
  const fields = object(lead) && Object.keys(lead).length ? mcpExactFields(lead, false, maximumFields) : { fields: [], truncated: 0, withheld: 0 };
  fields.withheld += withheld;
  return [...(fields.fields.length ? [{ kind: "fields" as const, heading: title, fields: fields.fields }] : []), { kind: "items", heading: title, items }, ...(records.length > items.length ? [notice(`Showing ${items.length} of ${records.length} records. The complete result was returned with this call.`)] : []), ...omissions(fields)];
}
export function mcpConfirmationProjection(operation: McpOperationEntry, arguments_: Record<string, unknown>): PresentationProjection {
  return presentationProjection.parse({ kind: operation.view_kind, title: bounded(operation.metadata.title, 512), summary: bounded(operation.metadata.summary, 512), sections: [{ kind: "fields", heading: "Review every submitted value", fields: mcpExactFields(arguments_, true).fields }] });
}
export function mcpOutputResourceRefs(value: unknown): ExecutedCallOutcome["resource_refs"] {
  const result: ExecutedCallOutcome["resource_refs"] = [];
  const kind = (key: string) => ["creator_id", "candidate_id"].includes(key) ? "creator" as const : ["conversation_id", "thread_id"].includes(key) ? "conversation" as const : ["connection_id", "operator_account_id", "credential_id", "mailbox_ref_id"].includes(key) ? "connection" as const : ["external_id", "channel_id", "post_id", "message_id", "community_id", "video_id", "discovery_id"].includes(key) ? "provider_object" as const : null;
  const walk = (node: unknown, path: Path, depth: number): void => {
    if (depth > 16 || result.length >= 64) return;
    if (Array.isArray(node)) node.forEach((child, index) => walk(child, [...path, index], depth + 1));
    else if (object(node)) for (const key of sorted(node).filter(mcpSafeFieldKey)) walk(node[key], [...path, key], depth + 1);
    else if (typeof node === "string") {
      const key = path.at(-1), type = typeof key === "string" ? kind(key) : null;
      if (type && /^[A-Za-z0-9._:-]{1,256}$/u.test(node) && !result.some(item => item.kind === type && item.id === node)) {
        const nameable = path.every(part => typeof part === "number" || /[A-Za-z0-9]/u.test(part));
        const candidate = nameable ? path.map(part => typeof part === "number" ? `[${part + 1}]` : part.split(/[^A-Za-z0-9]/u).filter(Boolean).map(word => word[0]!.toUpperCase() + word.slice(1)).join(" ")).reduce((text, part) => `${text}${text && !part.startsWith("[") ? " · " : ""}${part}`, "") : null;
        result.push({ kind: type, id: node, label: candidate !== null && bytes(candidate) <= 8192 ? candidate : null });
      }
    }
  };
  walk(value, [], 0); return result;
}
export function mcpCompletedSections(output: unknown, operation: McpOperationEntry, batchSize: number): Section[] {
  // 原只读前缀按本阶段完整读取数分配字段，按原始批次调用数预留字节和区段。
  if (!Number.isInteger(batchSize) || batchSize < 1 || batchSize > 8) throw new Error("projection_bounds");
  let maximum = 128, maximumFields = 128, detail = 1024;
  const budget = Math.floor(64 * 1024 / batchSize);
  const title = bounded(operation.metadata.title, 512), summary = `${title} completed.`;
  const operations: Section = { kind: "items", heading: "Operations", items: [{ title, description: summary }] };
  const fits = (sections: Section[]) => sections.length <= Math.floor(32 / batchSize) && bytes(stringifyApplicationJson({ kind: operation.view_kind, title: "Execution result", summary, sections })) <= budget;
  for (;;) {
    const projected = outputSections(output, title, maximum, detail, maximumFields);
    const sections = operation.effect !== "read_local" && operation.effect !== "read_open_world" ? [operations] : [operations, ...projected];
    if (fits(sections)) return sections;
    if (maximum > 1) maximum = Math.max(1, Math.floor(maximum / 2));
    else if (detail > 128) detail = Math.max(128, Math.floor(detail / 2));
    else if (maximumFields > 1) maximumFields = Math.max(1, Math.floor(maximumFields / 2));
    else {
      const path = locate(output);
      let records = output;
      if (path !== null) for (const key of path) { if (!object(records)) throw new Error("projection_encoding"); records = records[key]; }
      const projected = mcpExactFields(path === null ? output : prune(output, path), false, 1);
      const messages = path !== null && Array.isArray(records)
        ? [notice(`Showing 0 of ${records.length} records. The complete result was returned with this call.`), ...omissions({ fields: [], truncated: path.length ? projected.fields.length + projected.truncated : 0, withheld: projected.withheld })]
        : [notice(`Showing 0 of ${projected.fields.length + projected.truncated} values. The complete result was returned with this call.`), ...omissions({ fields: [], truncated: 0, withheld: projected.withheld })];
      const retained = [operations, notice(messages.map(value => value.kind === "notice" ? value.body : "").join("\n"), title)];
      if (!fits(retained)) throw new Error("projection_bounds");
      return retained;
    }
  }
}
