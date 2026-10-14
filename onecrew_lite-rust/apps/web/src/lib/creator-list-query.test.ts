import { InfiniteQueryObserver, QueryClient } from "@tanstack/react-query";
import { afterEach, describe, expect, it, vi } from "vitest";
import { invoke } from "./local-runtime";
import type { CreatorListPage } from "./creator";
import { storedFilters } from "@/components/data-table/filter-state";
import { canFetchCreatorPage, creatorFacetValues, creatorTableCriteria } from "./creator-list-query";
import { creatorsInfiniteQueryOptions } from "./runtime-query";

vi.mock("./local-runtime", () => ({ invoke: vi.fn(), isRuntimeAvailable: () => true }));
afterEach(() => { vi.clearAllMocks(); vi.unstubAllGlobals(); });

const firstPage: CreatorListPage = {
  items: [], next_cursor: "next-page",
  facets: {
    total: 205,
    stages: [{ value: "engaged", count: 180 }, { value: "prospect", count: 25 }],
    primary_platforms: [{ value: "youtube", count: 205 }],
  },
};

describe("Creator full-list query", () => {
  it("restores selection and calendar boundaries before the first request and binds every page to them", async () => {
    vi.stubGlobal("localStorage", { getItem: () => JSON.stringify([
      { id: "stage", value: ["engaged", "prospect", "engaged"] },
      { id: "platform", value: ["youtube"] },
      { id: "lastTouch", value: { from: 1725148800, toExclusive: 1725235200 } },
      { id: "updatedAt", value: { from: 1725062400, toExclusive: 1725321600 } },
    ]) });
    const criteria = {
      ...creatorTableCriteria(storedFilters("leads-creators-unfiltered")!, [{ id: "followers", desc: true }]),
      query: "  maker  ", group: { kind: "uncategorized" as const }, include_archived: true,
    };
    const expected = {
      query: "maker", group: { kind: "uncategorized" }, include_archived: true,
      stages: ["engaged", "prospect"], primary_platforms: ["youtube"],
      last_touch_range: { from: 1725148800, to_exclusive: 1725235200 },
      updated_range: { from: 1725062400, to_exclusive: 1725321600 },
      sorting: [{ field: "followers", direction: "desc" }],
    };
    const options = creatorsInfiniteQueryOptions(true, criteria);
    expect(options.queryKey).toEqual(["creator-creators", "list", expected]);
    const reordered = creatorsInfiniteQueryOptions(true, { ...criteria, stages: ["prospect", "engaged"] });
    expect(reordered.queryKey).toEqual(options.queryKey);
    vi.mocked(invoke).mockResolvedValueOnce(firstPage).mockResolvedValueOnce({ items: [], next_cursor: null, facets: null });
    const client = new QueryClient();
    const observer = new InfiniteQueryObserver(client, options);
    await observer.refetch();
    await observer.fetchNextPage();
    expect(vi.mocked(invoke).mock.calls).toEqual([
      ["onecrew_creator_list", { input: { ...expected, limit: 100, cursor: null, include_facets: true } }],
      ["onecrew_creator_list", { input: { ...expected, limit: 100, cursor: "next-page", include_facets: false } }],
    ]);
    const pages = observer.getCurrentResult().data!.pages;
    expect(creatorFacetValues(pages[0].facets).stage.get("engaged")).toBe(180);
    expect(pages[1].facets).toBeNull();
    client.clear();
  });

  it("changes the cache identity for each server criterion", () => {
    const base = creatorsInfiniteQueryOptions(false).queryKey;
    const variants = [
      { query: "Ada" }, { include_archived: true }, { group: { kind: "uncategorized" as const } },
      { stages: ["archived" as const] }, { primary_platforms: ["x" as const] },
      { last_touch_range: { from: 100 } }, { updated_range: { to_exclusive: 200 } },
      { sorting: [{ field: "score" as const, direction: "asc" as const }] },
    ];
    for (const criteria of variants) expect(creatorsInfiniteQueryOptions(false, criteria).queryKey).not.toEqual(base);
  });

  it("prevents old-cursor paging while a new criterion displays placeholder rows", async () => {
    vi.mocked(invoke).mockResolvedValueOnce(firstPage);
    const client = new QueryClient();
    const observer = new InfiniteQueryObserver(client, creatorsInfiniteQueryOptions(true));
    const unsubscribe = observer.subscribe(() => {});
    await observer.refetch();
    expect(canFetchCreatorPage(observer.getCurrentResult())).toBe(true);
    let finish!: (page: CreatorListPage) => void;
    vi.mocked(invoke).mockReturnValueOnce(new Promise((resolve) => { finish = resolve; }));
    observer.setOptions(creatorsInfiniteQueryOptions(true, { stages: ["archived"] }));
    const pending = observer.getCurrentResult();
    expect(pending.isPlaceholderData).toBe(true);
    expect(pending.data!.pages[0].next_cursor).toBe("next-page");
    expect(canFetchCreatorPage(pending)).toBe(false);
    expect(vi.mocked(invoke).mock.calls[1][1]).toMatchObject({ input: { cursor: null, stages: ["archived"] } });
    finish({ items: [], next_cursor: null, facets: null });
    await vi.waitFor(() => expect(observer.getCurrentResult().isFetching).toBe(false));
    expect(canFetchCreatorPage(observer.getCurrentResult())).toBe(false);
    unsubscribe();
    client.clear();
  });

  it("blocks pagination during initial loading, refetch, and the search debounce", () => {
    const ready = { hasNextPage: true, isFetching: false, isPlaceholderData: false, isPending: false };
    for (const busy of [{ isPending: true }, { isFetching: true }, { searchPending: true }]) {
      expect(canFetchCreatorPage({ ...ready, ...busy })).toBe(false);
    }
  });
});
