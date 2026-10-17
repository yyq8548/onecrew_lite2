import { describe, expect, it } from "vitest";
import { excludedSearchPlatforms, type OpenCliAuthObservation } from "./opencli-auth";

const capabilities = ["instagram", "linkedin-salesnav", "nowcoder", "youtube", "substack"].map(platform =>
  ({ platform, browser_required: platform !== "substack" }));
const observation = (platform: string, status: OpenCliAuthObservation["status"], profile_ref = "chosen", issue: OpenCliAuthObservation["issue"] = null): OpenCliAuthObservation =>
  ({ platform, status, profile_ref, issue, checked_at: 100, method: "full" });

describe("search eligibility", () => {
  it("skips unchecked login/conditional sources but keeps public sources", () => {
    expect(excludedSearchPlatforms({}, "chosen", capabilities, 100)).toEqual(["instagram", "linkedin-salesnav", "nowcoder", "youtube"]);
  });
  it("requires the selected profile's verified login without entitlement issues", () => {
    for (const status of ["not_logged_in", "unknown", "error", "session_detected"] as const)
      expect(excludedSearchPlatforms({ instagram: observation("instagram", status) }, "chosen", capabilities, 100)).toContain("instagram");
    expect(excludedSearchPlatforms({ instagram: observation("instagram", "logged_in", "other") }, "chosen", capabilities, 100)).toContain("instagram");
    expect(excludedSearchPlatforms({ instagram: observation("instagram", "logged_in") }, "chosen", capabilities, 100)).not.toContain("instagram");
    expect(excludedSearchPlatforms({ "linkedin-salesnav": observation("linkedin-salesnav", "logged_in", "chosen", "entitlement_unverified") }, "chosen", capabilities, 100)).toContain("linkedin-salesnav");
  });
});

 it("Bilibili full flow requires a fresh verified self-account, not a public search or cookie", () => {
   const cap = [{platform: "bilibili", browser_required: true}];
   expect(excludedSearchPlatforms({}, "chosen", cap, 100)).toEqual(["bilibili"]);
   expect(excludedSearchPlatforms({bilibili: observation("bilibili", "logged_in")}, "chosen", cap, 100)).toEqual([]);
   expect(excludedSearchPlatforms({bilibili: {...observation("bilibili", "logged_in"), method: "quick"}}, "chosen", cap, 100)).toEqual(["bilibili"]);
   expect(excludedSearchPlatforms({bilibili: observation("bilibili", "logged_in")}, "chosen", cap, 2000)).toEqual(["bilibili"]);
 });
