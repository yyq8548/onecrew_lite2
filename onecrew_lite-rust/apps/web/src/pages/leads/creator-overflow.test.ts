import { describe, expect, it } from "vitest";

import type { CreatorSummary } from "@/lib/creator";
import {
  hiddenChannels,
  hiddenFollowerCounts,
  hiddenPlatformLabels,
} from "./creator-overflow";

function summary(overrides: Partial<CreatorSummary>): CreatorSummary {
  return {
    id: "c1",
    display_name: "Creator",
    stage: "prospect",
    primary_platform: null,
    primary_handle: null,
    avatar_url: null,
    platforms: [],
    channels: [],
    has_manual_origin: false,
    score: null,
    assessment: null,
    follower_counts: {},
    last_touch_at: null,
    pinned_at: null,
    platform_account_count: 0,
    contact_route_count: 0,
    archived_at: null,
    group: null,
    created_at: 0,
    updated_at: 0,
    ...overrides,
  } as CreatorSummary;
}

describe("hiddenPlatformLabels", () => {
  it("lists everything the +N chip stands for", () => {
    expect(
      hiddenPlatformLabels(summary({ platforms: ["x", "telegram", "instagram"] })),
    ).toEqual(["Telegram", "Instagram"]);
  });

  it("is empty when nothing is hidden", () => {
    expect(hiddenPlatformLabels(summary({ platforms: ["x"] }))).toEqual([]);
    expect(hiddenPlatformLabels(summary({ platforms: [] }))).toEqual([]);
  });
});

describe("hiddenChannels", () => {
  it("lists only what the … chip swallowed", () => {
    expect(hiddenChannels(["email", "x", "telegram", "website"], 3)).toEqual(["website"]);
  });

  it("is empty when everything fits", () => {
    expect(hiddenChannels(["email", "x"], 3)).toEqual([]);
  });
});

describe("hiddenFollowerCounts", () => {
  it("excludes the platform actually on screen, not the first map key", () => {
    // follower_counts 的键序是字典序,不是收录顺序:instagram 排在 x 前面,
    // 但显示的是 x。按键序切会把 x 报成「隐藏的」,同时漏掉 instagram。
    const record = summary({
      primary_platform: "x",
      platforms: ["x", "instagram"],
      follower_counts: { instagram: 1200, x: 340 } as CreatorSummary["follower_counts"],
    });
    expect(hiddenFollowerCounts(record, "x", "zh-CN")).toEqual(["Instagram 1200"]);
    expect(hiddenFollowerCounts(record, "x", "en-US")).toEqual(["Instagram 1.2K"]);
  });

  it("is empty when only one platform has a count", () => {
    const record = summary({
      primary_platform: "x",
      follower_counts: { x: 340 } as CreatorSummary["follower_counts"],
    });
    expect(hiddenFollowerCounts(record, "x")).toEqual([]);
  });
});
