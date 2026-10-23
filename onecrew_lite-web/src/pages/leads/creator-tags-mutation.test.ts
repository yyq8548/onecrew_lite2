import { QueryClient } from "@tanstack/react-query";
import { afterEach, describe, expect, it, vi } from "vitest";
import { invoke } from "@/lib/local-runtime";
import type { CreatorRecord } from "@/lib/creator";
import { creatorDetailQueryKey, creatorsInfiniteQueryOptions } from "@/lib/runtime-query";
import { queryKeysForRuntimeChangeScopes } from "@/lib/runtime-change";
import { creatorWriteErrorDetail, saveCreatorTags } from "./creator-tags-mutation";

vi.mock("@/lib/local-runtime", () => ({ invoke: vi.fn(), isRuntimeAvailable: () => true }));
afterEach(() => vi.clearAllMocks());

const record: CreatorRecord = {
  creator: { id: "creator-1", display_name: "Ada", tags: ["Art"], follower_counts: {}, created_at: 1, updated_at: 2 },
  group: null,
  relationship: { creator_id: "creator-1", stage: "prospect", created_at: 1, updated_at: 1 },
  platform_accounts: [], account_links: [], account_observations: [], observation_sources: [], contact_routes: [], origins: [],
};

describe("Creator tags saved through the canonical update", () => {
  it("sends an explicit replacement, uses full-record readback, and invalidates list/details", async () => {
    const client = new QueryClient();
    const listKey = creatorsInfiniteQueryOptions(false).queryKey;
    client.setQueryData(listKey, { pages: [], pageParams: [] });
    client.setQueryData(creatorDetailQueryKey("creator-1"), record);
    const readback = { ...record, creator: { ...record.creator, tags: [], updated_at: 3 } };
    vi.mocked(invoke).mockResolvedValueOnce(readback);
    expect(await saveCreatorTags(client, "creator-1", [])).toEqual(readback);
    expect(invoke).toHaveBeenCalledWith("onecrew_creator_update", { input: { creator_id: "creator-1", tags: [] } });
    expect(client.getQueryData(creatorDetailQueryKey("creator-1"))).toEqual(readback);
    expect(client.getQueryState(listKey)?.isInvalidated).toBe(true);
    expect(client.getQueryState(creatorDetailQueryKey("creator-1"))?.isInvalidated).toBe(true);
    client.clear();
  });

  it("does not replace persisted readback or invalidate caches after a rejected write", async () => {
    const client = new QueryClient();
    client.setQueryData(creatorDetailQueryKey("creator-1"), record);
    const failure = { code: "validation", message: "Creator tags must be unique" };
    vi.mocked(invoke).mockRejectedValueOnce(failure);
    await expect(saveCreatorTags(client, "creator-1", ["Art", "Art"])).rejects.toEqual(failure);
    expect(client.getQueryData(creatorDetailQueryKey("creator-1"))).toEqual(record);
    expect(client.getQueryState(creatorDetailQueryKey("creator-1"))?.isInvalidated).toBe(false);
    expect(creatorWriteErrorDetail(failure)).toBe("Creator tags must be unique");
    expect(creatorWriteErrorDetail(new Error("offline"))).toBe("offline");
    expect(creatorWriteErrorDetail({ code: "unknown" })).toBeNull();
    client.clear();
  });

  it("keeps MCP Creator writes on the existing list and detail invalidation family", async () => {
    const client = new QueryClient();
    const listKey = creatorsInfiniteQueryOptions(false, { stages: ["engaged"] }).queryKey;
    client.setQueryData(listKey, { pages: [], pageParams: [] });
    client.setQueryData(creatorDetailQueryKey("creator-1"), record);
    await Promise.all(queryKeysForRuntimeChangeScopes(["creators"]).map((queryKey) => client.invalidateQueries({ queryKey })));
    expect(client.getQueryState(listKey)?.isInvalidated).toBe(true);
    expect(client.getQueryState(creatorDetailQueryKey("creator-1"))?.isInvalidated).toBe(true);
    expect(invoke).not.toHaveBeenCalled();
    client.clear();
  });
});
