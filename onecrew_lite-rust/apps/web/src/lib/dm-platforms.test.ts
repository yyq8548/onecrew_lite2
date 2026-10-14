import { describe, expect, it } from "vitest";

import { DM_PLATFORMS, isDmPlatform } from "@/lib/dm-platforms";
import type { ContactChannel } from "@/lib/creator";
import type { Platform } from "@/types";

/** Every channel a saved contact route can name, written out so a new one
 * cannot be added on the wire without deciding whether it carries a DM. */
const EVERY_CONTACT_CHANNEL: ContactChannel[] = [
  "email",
  "x",
  "instagram",
  "telegram",
  "youtube",
  "tiktok",
  "website",
  "link_hub",
];

describe("the platforms with a direct-message inbox", () => {
  it("is the operator-account set: X, Instagram and Telegram", () => {
    expect([...DM_PLATFORMS]).toEqual(["x", "instagram", "telegram"]);
    // The list is the type: an entry the union does not allow fails to compile.
    const asPlatforms: readonly Platform[] = DM_PLATFORMS;
    expect(new Set(asPlatforms).size).toBe(DM_PLATFORMS.length);
  });

  it("answers for every contact channel, and only the listed ones carry a DM", () => {
    const carrying = EVERY_CONTACT_CHANNEL.filter(isDmPlatform);
    expect(carrying).toEqual([...DM_PLATFORMS]);
    // YouTube has a service credential and TikTok no local connection at all;
    // neither is an operator account, so the Creator runtime refuses a direct
    // message for both.
    expect(isDmPlatform("youtube")).toBe(false);
    expect(isDmPlatform("tiktok")).toBe(false);
    expect(isDmPlatform("email")).toBe(false);
  });

  it("rejects anything that is not a channel at all", () => {
    expect(isDmPlatform("")).toBe(false);
    expect(isDmPlatform("X")).toBe(false);
    expect(isDmPlatform("telegram_bot")).toBe(false);
  });
});
