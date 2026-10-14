import { createInstance } from "i18next";
import { describe, expect, it } from "vitest";

import MESSAGE_WORKSPACE_SOURCE from "@/components/message-workspace.tsx?raw";
import INBOX_SOURCE from "@/pages/Inbox.tsx?raw";
import INTENT_TAG_SOURCE from "@/components/reply-intent-tag.tsx?raw";
import enUSInbox from "@/i18n/locales/en-US/inbox";
import zhCNInbox from "@/i18n/locales/zh-CN/inbox";

function catalogKeys(value: unknown, prefix = ""): string[] {
  if (typeof value !== "object" || value == null) return [prefix];
  return Object.entries(value).flatMap(([key, child]) =>
    catalogKeys(child, prefix ? `${prefix}.${key}` : key));
}

function catalogEntries(value: unknown, prefix = ""): Array<[string, string]> {
  if (typeof value === "string") return [[prefix, value]];
  if (typeof value !== "object" || value == null) return [];
  return Object.entries(value).flatMap(([key, child]) =>
    catalogEntries(child, prefix ? `${prefix}.${key}` : key));
}

function interpolationTokens(value: string): string[] {
  return [...value.matchAll(/{{\s*([^},\s]+)[^}]*}}/g)]
    .map((match) => match[1]!)
    .sort();
}

describe("Inbox and message-workspace translations", () => {
  it("keeps the inbox locale catalogs in strict key parity", () => {
    expect(catalogKeys(enUSInbox).sort()).toEqual(catalogKeys(zhCNInbox).sort());
    expect(catalogKeys(zhCNInbox)).toEqual(expect.arrayContaining([
      "conversations.filters.searchPlaceholder",
      "intent.labels.interested",
      "outreach.telegram.identifyFailed",
      "outreach.message.warningDescription",
    ]));

    const english = new Map(catalogEntries(enUSInbox));
    for (const [key, chineseValue] of catalogEntries(zhCNInbox)) {
      expect(interpolationTokens(english.get(key) ?? ""), key).toEqual(
        interpolationTokens(chineseValue),
      );
    }
  });

  it("uses the inbox namespace across every React surface in scope", () => {
    for (const source of [
      INBOX_SOURCE,
      MESSAGE_WORKSPACE_SOURCE,
      INTENT_TAG_SOURCE,
    ]) {
      expect(source).toContain('useTranslation("inbox")');
    }
  });

  it("routes fixed Toast titles through translations and preserves dynamic errors", () => {
    expect(MESSAGE_WORKSPACE_SOURCE).not.toMatch(/toast\.(?:error|success)\((?:"|'|`)/);
    expect(MESSAGE_WORKSPACE_SOURCE).toContain("description: String(error)");
  });

  it("renders English interpolation without altering dynamic values", async () => {
    const i18n = createInstance();
    await i18n.init({
      fallbackLng: "zh-CN",
      interpolation: { escapeValue: false },
      lng: "en-US",
      resources: {
        "zh-CN": { inbox: zhCNInbox },
        "en-US": { inbox: enUSInbox },
      },
    });

    expect(i18n.t("outreach.message.sent", {
      ns: "inbox",
      handle: "creator_raw",
    })).toBe("Direct message sent to @creator_raw");
    expect(i18n.t("conversations.listLoadFailed", {
      ns: "inbox",
      error: "RAW_PROVIDER_ERROR",
    })).toBe("Could not load conversations: RAW_PROVIDER_ERROR");
  });
});
