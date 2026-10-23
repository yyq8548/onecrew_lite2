import { describe, expect, it } from "vitest";
import { openCliSearchAccess, openCliSearchNeedsLoginCheck, openCliAccountNeedsLoginCheck } from "./opencli-search-access";
import { excludedSearchPlatforms } from "./opencli-auth";
import { openCliPlatformIsGreen, orderOpenCliPlatforms } from "./opencli-platform-presentation";

describe("reviewed search login requirements", () => {
  it("does not equate Chrome or cookies with account login", () => {
    for (const platform of ["google", "yahoo", "youtube"]) {
      expect(openCliSearchAccess(platform, true)).toBe("public_browser");
      expect(openCliSearchNeedsLoginCheck(platform, true)).toBe(false);
    }
    expect(openCliSearchAccess("twitter", true)).toBe("login_required");
    expect(openCliAccountNeedsLoginCheck("youtube", true)).toBe(true);
    expect(openCliSearchAccess("bilibili", true)).toBe("login_required");
    expect(openCliSearchNeedsLoginCheck("bilibili", true)).toBe(true);
    expect(openCliSearchAccess("linkedin-salesnav", true)).toBe("extra_entitlement");
    expect(openCliSearchAccess("tiktok", true)).toBe("conditional");
    expect(openCliSearchAccess("substack", false)).toBe("public_read");
  });
  it("keeps browser-free sources last and does not claim browser readiness without a connection", () => {
    const make = (platform: string, browser_required = true) => ({ platform, browser_required, registered: true, command: "search" });
    expect(orderOpenCliPlatforms([make("google"), make("substack", false), make("twitter")]).map(row => row.platform))
      .toEqual(["twitter", "google", "substack"]);
    expect(openCliPlatformIsGreen(make("youtube"), null, undefined, false)).toBe(false);
    expect(openCliPlatformIsGreen(make("youtube"), "selected", undefined, false)).toBe(false);
    expect(openCliPlatformIsGreen(make("google"), "selected", undefined, false)).toBe(true);
    expect(openCliPlatformIsGreen({ ...make("youtube"), registered: false }, "selected", undefined, false)).toBe(false);
  });
  it("preserves default public YouTube search when no platform is signed in", () => {
    expect(excludedSearchPlatforms({}, "selected", [{ platform: "youtube", browser_required: true }])).toEqual([]);
    expect(excludedSearchPlatforms({}, "selected", [{ platform: "youtube", browser_required: true }, { platform: "instagram", browser_required: true }])).toEqual(["instagram"]);
  });
});
