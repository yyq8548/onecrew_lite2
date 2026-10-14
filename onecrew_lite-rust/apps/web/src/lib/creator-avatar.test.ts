import { describe, expect, it } from "vitest";
import { creatorRecordAvatarUrl, type CreatorPlatform, type CreatorRecord } from "@/lib/creator";

function record(platform: CreatorPlatform, publicData: Record<string, unknown>): CreatorRecord {
  return {
    creator: { id: "creator", display_name: "Creator", tags: [], follower_counts: {}, created_at: 1, updated_at: 1 },
    group: null,
    relationship: { creator_id: "creator", stage: "prospect", created_at: 1, updated_at: 1 },
    platform_accounts: [{ id: "account", platform, target_kind: "user", external_id: "123", created_at: 1, updated_at: 1 }],
    account_links: [],
    account_observations: [{ id: "observation", platform_account_id: "account", source_id: "source", public_data: publicData, observed_at: 2 }],
    observation_sources: [], origins: [], contact_routes: [],
  };
}

const historicalKeys: [CreatorPlatform, string][] = [
  ["x", "profile_image_url"], ["instagram", "profile_pic_url"], ["instagram", "profile_pic_url_hd"],
  ["telegram", "photo_url"], ["telegram", "profile_photo_url"], ["youtube", "thumbnail_url"], ["tiktok", "profile_image"],
];

describe("Creator observation avatars", () => {
  it.each(historicalKeys)("reads canonical avatar_url before historical %s.%s", (platform, key) => {
    expect(creatorRecordAvatarUrl(record(platform, { avatar_url: "https://cdn.example/new.png", [key]: "https://cdn.example/old.png" }))).toBe("https://cdn.example/new.png");
  });
  it.each(historicalKeys)("retains existing v36 %s.%s observations", (platform, key) => {
    expect(creatorRecordAvatarUrl(record(platform, { [key]: "https://cdn.example/old.png" }))).toBe("https://cdn.example/old.png");
  });
  it("uses the latest observation and does not revive an older avatar", () => {
    const value = record("x", { avatar_url: "https://cdn.example/old.png" });
    value.account_observations.push({ ...value.account_observations[0], id: "newer", observed_at: 3, public_data: { description: "New profile" } });
    expect(creatorRecordAvatarUrl(value)).toBeNull();
  });
});
