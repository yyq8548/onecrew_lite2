import { describe, expect, it } from "vitest";
import i18next from "i18next";

import { resources } from "@/i18n/resources";

import {
  browserConnectionGate,
  browserCookieImportCopy,
  browserEnvironmentView,
  browserOperationErrorMessage,
} from "./browser-environment";

const testI18n = i18next.createInstance();
void testI18n.init({
  initAsync: false,
  lng: "zh-CN",
  resources,
});
const t = testI18n.getFixedT("zh-CN", "accounts");

const status = {
  installed: true,
  version: "128",
  path: "/Applications/Google Chrome.app",
  os: "macos" as const,
  state: "ready" as const,
  cookie_provider: "macos_safe_storage" as const,
  limitations: [],
};

describe("browserEnvironmentView", () => {
  it("keeps the last ready result while revalidating", () => {
    expect(
      browserEnvironmentView(
        status,
        true,
        t,
      ),
    ).toEqual({
      ready: true,
      loading: false,
      badgeLabel: "已就绪",
      description: "Chrome 128 · /Applications/Google Chrome.app",
      diagnostic: null,
    });
  });

  it("limits a missing browser warning to browser account connections", () => {
    const view = browserEnvironmentView(
      { ...status, installed: false, version: null, path: null, state: "browser_not_found" },
      false,
      t,
    );
    expect(view.badgeLabel).toBe("未检测到");
    expect(view.description).toContain("X / Instagram");
    expect(view.description).not.toContain("MCP");
  });

  it("gates browser flows on runtime state instead of legacy installed", () => {
    const view = browserEnvironmentView({
      ...status,
      state: "cookie_import_unavailable",
      limitations: ["WINDOWS_COOKIE_DECRYPTION_UNAVAILABLE"],
    }, false, t);
    expect(view.ready).toBe(false);
    expect(view.description).toContain("X / Instagram");
    expect(view.diagnostic).toBeNull();
  });

  it("uses provider-aware credential copy", () => {
    expect(browserCookieImportCopy(status, t).title).toContain("macOS");
    expect(browserCookieImportCopy({
      ...status,
      os: "windows",
      path: "C:\\Program Files\\Google\\Chrome\\Application\\chrome.exe",
      cookie_provider: "windows_legacy_dpapi",
    }, t).description).toContain("DPAPI");
  });

  it("models loading, error, blocked, and ready as distinct gate states", () => {
    expect(browserConnectionGate(null, true, t).state).toBe("loading");
    expect(browserConnectionGate(null, false, t, "browser_not_installed: missing")).toMatchObject({
      state: "error",
      diagnostic: "browser_not_installed",
    });
    expect(browserConnectionGate({
      ...status,
      state: "cookie_import_unavailable",
      error_code: "browser_cookie_extraction_unsupported",
    }, false, t).state).toBe("blocked");
    expect(browserConnectionGate(status, false, t).state).toBe("ready");
  });

  it("maps stable runtime codes to localized actionable copy", () => {
    expect(browserOperationErrorMessage("browser_cookie_store_locked: C:/profile/Cookies", t))
      .toContain("仍被占用");
    expect(browserOperationErrorMessage(new Error("unknown implementation detail"), t, "请重试"))
      .toBe("请重试");
  });
});
