import { beforeEach, describe, expect, it, vi } from "vitest";
import { invoke } from "./local-runtime";
import { authorGroupsKey, authorGroupsRefreshInterval, authorProfileLink, getDiscoveryAuthorGroups } from "./discovery-author-groups";

vi.mock("./local-runtime", () => ({ invoke: vi.fn() }));
const request = { campaignId: "campaign-a", revision: 2 };
const emptySnapshot = () => ({
  campaign_id: "campaign-a", criteria_revision: 2, snapshot_hash: "a".repeat(64),
  groups: [], reviews: [], counts: { native_accounts: 0, qualified_native_accounts: 0,
    identity_groups: 0, qualified_identity_groups: 0, qualified_native_only_groups: 0,
    unbound_source_hits: 0, stale_identity_reviews: 0 },
});

describe("saved author groups", () => {
  beforeEach(() => vi.resetAllMocks());
  it("reads only the typed local snapshot for the selected campaign revision", async () => {
    vi.mocked(invoke).mockResolvedValue(emptySnapshot());
    expect(await getDiscoveryAuthorGroups(request)).toEqual(emptySnapshot());
    expect(invoke).toHaveBeenCalledExactlyOnceWith("onecrew_discovery_identity_groups_get", {
      input: { campaign_id: "campaign-a", expected_revision: 2 },
    });
  });
  it.each([
    { campaign_id: "campaign-b" }, { criteria_revision: 1 }, { counts: {} }, { groups: [{}] }, { snapshot_hash: "invalid" },
  ])("rejects another campaign, stale revision, or malformed groups: %j", async patch => {
    vi.mocked(invoke).mockResolvedValue({ ...emptySnapshot(), ...patch });
    await expect(getDiscoveryAuthorGroups(request)).rejects.toThrow("discovery_author_groups_invalid");
  });
  it("does not turn a failed read into an empty successful result", async () => {
    vi.mocked(invoke).mockRejectedValue(new Error("session required"));
    await expect(getDiscoveryAuthorGroups(request)).rejects.toThrow("session required");
  });
  it("isolates query caches and bounds automatic local refresh", () => {
    expect(authorGroupsKey(request)).not.toEqual(authorGroupsKey({ ...request, revision: 3 }));
    expect(authorGroupsKey(request)).not.toEqual(authorGroupsKey({ ...request, campaignId: "b" }));
    expect(authorGroupsRefreshInterval(11, false)).toBe(5000);
    expect(authorGroupsRefreshInterval(12, false)).toBe(false);
    expect(authorGroupsRefreshInterval(0, true)).toBe(false);
  });
  it("allows HTTPS profile links and rejects executable or credential-bearing links", () => {
    expect(authorProfileLink("https://www.youtube.com/@creator")).toBe("https://www.youtube.com/@creator");
    for (const url of ["javascript:alert(1)", "data:text/html,test", "http://example.org", "https://user:secret@example.org", "bad-url"])
      expect(authorProfileLink(url)).toBeNull();
  });
});
