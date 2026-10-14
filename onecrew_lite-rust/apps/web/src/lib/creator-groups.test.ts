import { describe, expect, it } from "vitest";

import {
  creatorGroupsInfiniteQueryOptions,
  shouldAutoFetchCreatorGroupPage,
} from "@/lib/creator-groups";
import { creatorsInfiniteQueryOptions } from "@/lib/runtime-query";

describe("Creator group query contracts", () => {
  it("binds group membership into the Creator cursor-query identity", () => {
    const options = creatorsInfiniteQueryOptions(false, {
      include_archived: true,
      query: "  Ada  ",
      group: { kind: "group", group_id: "group-1" },
    });

    expect(options.queryKey).toEqual([
      "creator-creators",
      "list",
      {
        query: "Ada",
        stages: [],
        primary_platforms: [],
        last_touch_range: null,
        updated_range: null,
        sorting: [],
        group: { kind: "group", group_id: "group-1" },
        include_archived: true,
      },
    ]);
  });

  it("keeps the authoritative group collection in its own query family", () => {
    const options = creatorGroupsInfiniteQueryOptions(false, {
      include_archived: true,
      query: "  launch  ",
    });

    expect(options.queryKey).toEqual([
      "creator-creator-groups",
      { query: "launch", include_archived: true },
    ]);
  });

  it("never loops automatic destination paging after a failed page", () => {
    expect(shouldAutoFetchCreatorGroupPage({
      selectedCount: 2,
      hasNextPage: true,
      isFetchingNextPage: false,
      hasError: true,
    })).toBe(false);
    expect(shouldAutoFetchCreatorGroupPage({
      selectedCount: 2,
      hasNextPage: true,
      isFetchingNextPage: false,
      hasError: false,
    })).toBe(true);
  });
});
