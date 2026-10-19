import type { ColumnFiltersState, SortingState } from "@tanstack/react-table";
import type { CreatorListFacets, CreatorListSort, ListCreatorsInput } from "./creator";
import { readTimestampDateRange } from "./date-range-filter";

export type CreatorListCriteria = Omit<ListCreatorsInput, "limit" | "cursor" | "include_facets">;

export function normalizeCreatorListCriteria(criteria: CreatorListCriteria = {}) {
  return {
    query: criteria.query?.trim() || null,
    group: criteria.group ?? null,
    include_archived: criteria.include_archived ?? false,
    stages: [...new Set(criteria.stages ?? [])].sort(),
    primary_platforms: [...new Set(criteria.primary_platforms ?? [])].sort(),
    last_touch_range: criteria.last_touch_range ?? null,
    updated_range: criteria.updated_range ?? null,
    sorting: criteria.sorting ?? [],
  };
}

const SORT_FIELDS: Record<string, CreatorListSort["field"]> = {
  creator: "display_name",
  followers: "followers",
  score: "score",
  lastTouch: "last_touch_at",
  updatedAt: "updated_at",
  createdAt: "created_at",
};

/** Translate persisted table values into the canonical Creator list query, keeping
 * calendar ranges in Unix seconds with an exclusive upper bound. */
export function creatorTableCriteria(filters: ColumnFiltersState, sorting: SortingState): CreatorListCriteria {
  const value = (id: string) => filters.find((filter) => filter.id === id)?.value;
  const selected = <T extends string>(id: string, allowed: readonly T[]): T[] => {
    const values = value(id);
    return Array.isArray(values) ? allowed.filter((item) => values.includes(item)) : [];
  };
  const range = (id: string) => {
    const stored = readTimestampDateRange(value(id));
    return stored ? { from: stored.from, to_exclusive: stored.toExclusive } : null;
  };
  return {
    stages: selected("stage", ["prospect", "engaged", "collaborating", "archived"] as const),
    primary_platforms: selected("platform", ["x", "instagram", "telegram", "youtube", "tiktok"] as const),
    last_touch_range: range("lastTouch"),
    updated_range: range("updatedAt"),
    sorting: sorting.flatMap(({ id, desc }) => SORT_FIELDS[id]
      ? [{ field: SORT_FIELDS[id], direction: desc ? "desc" as const : "asc" as const }]
      : []),
  };
}

export function creatorFacetValues(facets: CreatorListFacets | null | undefined) {
  return {
    stage: new Map(facets?.stages.map(({ value, count }) => [value, count]) ?? []),
    platform: new Map(facets?.primary_platforms.map(({ value, count }) => [value, count]) ?? []),
  };
}

export function canFetchCreatorPage(state: {
  hasNextPage: boolean;
  isFetching: boolean;
  isPlaceholderData: boolean;
  isPending: boolean;
  searchPending?: boolean;
}) {
  return state.hasNextPage && !state.isFetching && !state.isPlaceholderData
    && !state.isPending && !state.searchPending;
}
