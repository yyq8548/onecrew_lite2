import { describe, expect, it } from "vitest";

import enUSAccounts from "@/i18n/locales/en-US/accounts";
import zhCNAccounts from "@/i18n/locales/zh-CN/accounts";

import BROWSER_ALERT_SOURCE from "./browser-cookie-import-alert.tsx?raw";
import BROWSER_STATUS_SOURCE from "./browser-environment-status.tsx?raw";
import COLUMNS_SOURCE from "./columns.tsx?raw";
import CREDENTIAL_COLUMNS_SOURCE from "./credential-columns.tsx?raw";
import CONNECT_DIALOG_SOURCE from "./connect-dialog.tsx?raw";
import CONNECT_WIZARD_SOURCE from "./connect-wizard.tsx?raw";
import PROXY_SOURCE from "./proxy.tsx?raw";
import RELOGIN_SOURCE from "./relogin-dialog.tsx?raw";
import RESEND_SOURCE from "./resend-credential-dialog.tsx?raw";
import SHARED_SOURCE from "./shared.tsx?raw";
import TELEGRAM_SOURCE from "./tg-login.tsx?raw";
import TOOLKIT_SOURCE from "./toolkit-catalog.tsx?raw";
import X_CHAT_SOURCE from "./x-chat-passcode-dialog.tsx?raw";
import YOUTUBE_SOURCE from "./youtube-key-dialog.tsx?raw";

const UI_SOURCES = [
  BROWSER_ALERT_SOURCE,
  BROWSER_STATUS_SOURCE,
  COLUMNS_SOURCE,
  CREDENTIAL_COLUMNS_SOURCE,
  CONNECT_DIALOG_SOURCE,
  CONNECT_WIZARD_SOURCE,
  PROXY_SOURCE,
  RELOGIN_SOURCE,
  RESEND_SOURCE,
  SHARED_SOURCE,
  TELEGRAM_SOURCE,
  TOOLKIT_SOURCE,
  X_CHAT_SOURCE,
  YOUTUBE_SOURCE,
] as const;

function catalogKeys(value: unknown, prefix = ""): string[] {
  if (typeof value !== "object" || value == null) return [prefix];
  return Object.entries(value).flatMap(([key, child]) =>
    catalogKeys(child, prefix ? `${prefix}.${key}` : key));
}

function catalogStrings(value: unknown): string[] {
  if (typeof value === "string") return [value];
  if (typeof value !== "object" || value == null) return [];
  return Object.values(value).flatMap(catalogStrings);
}

function withoutComments(source: string) {
  return source
    .replace(/\/\*[\s\S]*?\*\//g, "")
    .replace(/\/\/.*$/gm, "");
}

describe("Accounts translations", () => {
  it("keeps complete, non-empty zh-CN and en-US catalogs in parity", () => {
    expect(catalogKeys(enUSAccounts).sort()).toEqual(catalogKeys(zhCNAccounts).sort());
    expect(catalogStrings(enUSAccounts).every(Boolean)).toBe(true);
    expect(catalogStrings(zhCNAccounts).every(Boolean)).toBe(true);
  });

  it("routes every Accounts React surface through the accounts namespace", () => {
    for (const source of UI_SOURCES) {
      expect(source).toContain('useTranslation("accounts")');
      expect(withoutComments(source)).not.toMatch(/[\p{Script=Han}]/u);
    }
  });

  it("keeps fixed Toast titles in the catalog and dynamic details untouched", () => {
    const source = UI_SOURCES.join("\n");
    expect(source).not.toMatch(/toast(?:\.(?:success|error|warning|info))?\(\s*["'`]/);
    expect(source).toContain("description: String(error)");
    expect(source).toContain("toast.success(result.health.message");
  });

  it("uses the onecrew_lite product name in fixed Accounts copy", () => {
    const fixedCopy = JSON.stringify({ enUSAccounts, zhCNAccounts });
    expect(fixedCopy).toContain("onecrew_lite");
    expect(fixedCopy).not.toMatch(/Reacher|Agent Platform/);
    expect(UI_SOURCES.join("\n")).not.toMatch(/Reacher|Agent Platform/);
  });
});
