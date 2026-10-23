import { createHash } from "node:crypto";
import { stringify } from "lossless-json";
import { searchPlanScope } from "@onecrew/runtime-contracts/search-plan";
export * from "@onecrew/runtime-contracts/search-plan";
function sortedJson(value: unknown): string {
  const keys = new Set<string>();
  const copy = (item: unknown): unknown => {
    if (Array.isArray(item)) return item.map(copy);
    if (item !== null && typeof item === "object") {
      const result: Record<string, unknown> = Object.create(null) as Record<string, unknown>;
      for (const [key, value] of Object.entries(item)) { keys.add(key); result[key] = copy(value); }
      return result;
    }
    return item;
  };
  const data = copy(value), encoded = stringify(data, [...keys].sort((a, b) => Buffer.compare(Buffer.from(a), Buffer.from(b))));
  if (encoded === undefined) throw new Error("invalid_search_scope"); return encoded;
}
export function searchScopeApprovalToken(raw: unknown): string {
  const scope = searchPlanScope.parse(raw);
  return `sha256:${createHash("sha256").update(sortedJson(scope)).digest("hex")}`;
}
