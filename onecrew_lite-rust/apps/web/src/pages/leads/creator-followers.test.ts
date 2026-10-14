import { describe, expect, it } from "vitest";
import { primaryFollowerEntry } from "./creator-followers";

describe("Creator displayed follower count", () => {
  it("prefers the primary platform, including a known zero", () => {
    expect(primaryFollowerEntry({
      primary_platform: "x", platforms: ["instagram", "x"],
      follower_counts: { instagram: 100, x: 0 },
    })).toEqual(["x", 0]);
  });

  it("uses first-capture platform order before the alphabetic fallback", () => {
    expect(primaryFollowerEntry({
      primary_platform: "youtube", platforms: ["youtube", "x", "instagram"],
      follower_counts: { instagram: 100, x: 7 },
    })).toEqual(["x", 7]);
  });

  it("uses alphabetic fallback for unlinked counts regardless of object insertion order", () => {
    for (const follower_counts of [{ instagram: 100, x: 7 }, { x: 7, instagram: 100 }]) {
      expect(primaryFollowerEntry({ platforms: [], follower_counts })).toEqual(["instagram", 100]);
      expect(primaryFollowerEntry({
        primary_platform: "youtube", platforms: ["youtube"], follower_counts,
      })).toEqual(["instagram", 100]);
    }
  });

  it("returns no displayed value when no platform has a known count", () => {
    expect(primaryFollowerEntry({ primary_platform: "x", platforms: ["x"], follower_counts: {} })).toBeNull();
  });
});
