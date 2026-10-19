import { z } from "zod";
import { isU64 } from "@onecrew/runtime-contracts/result-wire";
import { parseApplicationJson, stringifyApplicationJson } from "@onecrew/runtime-contracts/json";
import { CATEGORY_SEARCH_PLATFORMS, TAG_SEARCH_PLATFORMS } from "./opencli-discovery";

export const DISCOVERY_DEFAULTS = { minSubscribers: 2000, targetCount: 30, maxCalls: null } as const;
const text = z.string().max(4000);
const labels = z.record(z.string(), text);
const preferencesSchema = z.object({
  format: z.literal(1), brief: text, region: text, language: text, category: text,
  minSubscribers: z.custom<number | bigint>(isU64),
  followersEdited: z.boolean(), targetCount: z.number().int().min(1).max(10000),
  maxCalls: z.number().int().min(1).max(10000).nullable(), keywords: text, v2exNodes: text,
  redditCommunities: text, tagInputs: labels, categoryInputs: labels,
  platforms: z.array(z.string()).max(100), requireDemo: z.boolean(), requireToolLink: z.boolean(),
});
export type DiscoveryDraftPreferences = z.infer<typeof preferencesSchema>;
const KEY = "onecrew-discovery-form-v1";

export function loadDiscoveryPreferences(storage: Pick<Storage, "getItem">): DiscoveryDraftPreferences | null {
  try { const value = preferencesSchema.safeParse(parseApplicationJson(storage.getItem(KEY) ?? "null")); return value.success ? value.data : null; }
  catch { return null; }
}
export function saveDiscoveryPreferences(storage: Pick<Storage, "setItem">, input: DiscoveryDraftPreferences): boolean {
  const parsed = preferencesSchema.safeParse(input);
  if (!parsed.success) return false;
  try { storage.setItem(KEY, stringifyApplicationJson(parsed.data)); return true; } catch { return false; }
}

// Explicit user edits accompany the product brief; blank fields do not invent
// location/language restrictions. The follower override also travels as a typed
// field and is enforced by the owner independently of the model response.
export function discoveryPlanningBrief(input: Pick<DiscoveryDraftPreferences, "brief" | "region" | "language" | "category" | "keywords"> & { platformRestriction?: readonly string[]; defaultPlatforms?: readonly string[]; expandAllSources?: boolean }): string {
  const settings = [
    input.language.trim() && `Content language: ${input.language.trim()}`,
    input.category.trim() ? `Content category: ${input.category.trim()}` : input.expandAllSources && "Content category: unrestricted",
    input.keywords.trim() && `Preferred search keywords: ${input.keywords.trim()}`,
    input.expandAllSources && "Search all supported entries; replace any previous platform restriction.",
    !input.platformRestriction && !input.expandAllSources && input.defaultPlatforms?.length && `Default search entries when no platform is requested: ${input.defaultPlatforms.join(", ")}`,
    input.platformRestriction && `Search only these entries: ${input.platformRestriction.join(", ")}`,
  ].filter(Boolean);
  return [input.brief.trim(), ...settings].join("\n");
}

// Free-text query hints may be expanded by AI. Named site scopes must remain
// exact (a community, node, tag or category is not a translatable query).
export function discoveryPlanningScopeOverrides<T extends { platform: string; keywords: string[] }>(scopes: T[]): T[] {
  return scopes.filter(scope => scope.keywords.length > 0 && (
    scope.platform === "v2ex-node" || scope.platform === "reddit-subreddit"
    || TAG_SEARCH_PLATFORMS.some(platform => platform === scope.platform)
    || CATEGORY_SEARCH_PLATFORMS.some(platform => platform === scope.platform)));
}

/** Remove only the planner settings suffix when reopening a generated plan. */
export function discoveryProductBrief(brief: string): string {
  return brief.split(/\n(?:Creator location:|Content language:|Content category:|Preferred search keywords:|Search only these entries:|Default search entries when no platform is requested:|Search all supported entries;)/)[0].trim();
}

/** Editing another condition must not spread the keyword union to every site. */
export function discoveryEditedScopes(
  scopes: { platform: string; keywords: string[] }[],
  previous: { platform: string; keywords: string[] }[] | undefined,
  keywords: string,
) {
  if (!previous || keywords !== [...new Set(previous.flatMap(scope => scope.keywords))].slice(0, 20).join(", ")) return scopes;
  return scopes.map(scope => {
    const scoped = scope.platform === "v2ex-node" || scope.platform === "reddit-subreddit"
      || TAG_SEARCH_PLATFORMS.some(platform => platform === scope.platform)
      || CATEGORY_SEARCH_PLATFORMS.some(platform => platform === scope.platform);
    const saved = previous.find(item => item.platform === scope.platform);
    return !scoped && saved ? { ...scope, keywords: [...saved.keywords] } : scope;
  });
}

export function discoveryEditIsComplete(editing: boolean, category: string, scopes: { keywords: string[] }[], language = "") {
  return editing && !language.trim() && !!category.trim() && scopes.length > 0 && scopes.every(scope => scope.keywords.length > 0);
}
