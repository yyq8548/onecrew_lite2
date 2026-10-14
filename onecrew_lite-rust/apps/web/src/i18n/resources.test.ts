import { describe, expect, it } from "vitest";

import RESOURCES_SOURCE from "./resources.ts?raw";
import EN_ACCOUNTS_SOURCE from "./locales/en-US/accounts.ts?raw";
import EN_COMMON_SOURCE from "./locales/en-US/common.ts?raw";
import EN_CONNECT_SOURCE from "./locales/en-US/connect.ts?raw";
import EN_INBOX_SOURCE from "./locales/en-US/inbox.ts?raw";
import EN_LEADS_SOURCE from "./locales/en-US/leads.ts?raw";
import EN_MAIL_SOURCE from "./locales/en-US/mail.ts?raw";
import EN_PAIRING_SOURCE from "./locales/en-US/pairing.ts?raw";
import EN_SHELL_SOURCE from "./locales/en-US/shell.ts?raw";
import EN_SIDEBAR_SOURCE from "./locales/en-US/sidebar.ts?raw";
import ZH_ACCOUNTS_SOURCE from "./locales/zh-CN/accounts.ts?raw";
import ZH_COMMON_SOURCE from "./locales/zh-CN/common.ts?raw";
import ZH_CONNECT_SOURCE from "./locales/zh-CN/connect.ts?raw";
import ZH_INBOX_SOURCE from "./locales/zh-CN/inbox.ts?raw";
import ZH_LEADS_SOURCE from "./locales/zh-CN/leads.ts?raw";
import ZH_MAIL_SOURCE from "./locales/zh-CN/mail.ts?raw";
import ZH_PAIRING_SOURCE from "./locales/zh-CN/pairing.ts?raw";
import ZH_SHELL_SOURCE from "./locales/zh-CN/shell.ts?raw";
import ZH_SIDEBAR_SOURCE from "./locales/zh-CN/sidebar.ts?raw";
import { NAMESPACES, resources } from "./resources";

function catalogKeys(value: unknown, prefix = ""): string[] {
  if (typeof value !== "object" || value == null) return [prefix];
  return Object.entries(value).flatMap(([key, child]) =>
    catalogKeys(child, prefix ? `${prefix}.${key}` : key));
}

function catalogStrings(value: unknown, prefix = ""): Array<[string, string]> {
  if (typeof value === "string") return [[prefix, value]];
  if (typeof value !== "object" || value == null) return [];
  return Object.entries(value).flatMap(([key, child]) =>
    catalogStrings(child, prefix ? `${prefix}.${key}` : key));
}

function interpolationTokens(value: string): string[] {
  return Array.from(
    value.matchAll(/\{\{\s*([^},\s]+)(?:\s*,[^}]*)?\s*\}\}/g),
    (match) => match[1],
  ).sort();
}

describe("desktop translation catalogs", () => {
  it("keeps English and Chinese keys in parity within every namespace", () => {
    expect(Object.keys(resources["zh-CN"]).sort()).toEqual([...NAMESPACES].sort());
    expect(Object.keys(resources["en-US"]).sort()).toEqual([...NAMESPACES].sort());

    for (const namespace of NAMESPACES) {
      const chinese = catalogKeys(resources["zh-CN"][namespace]).sort();
      const english = catalogKeys(resources["en-US"][namespace]).sort();
      expect(english, `${namespace} namespace`).toEqual(chinese);
    }

    expect(catalogKeys(resources["zh-CN"].sidebar)).toContain("navigation.leads");
    expect(catalogKeys(resources["zh-CN"].sidebar)).toContain("language.switchToEnglish");
    expect(catalogKeys(resources["zh-CN"].shell)).toContain("views.connect.subtitle");
    expect(catalogKeys(resources["zh-CN"].connect)).toContain("codex.pluginStates.ready.title");
    expect(catalogKeys(resources["zh-CN"].connect)).toContain(
      "codex.pluginStates.unrelatedMarketplace.description",
    );
    expect(catalogKeys(resources["zh-CN"].pairing)).toContain("states.remoteUnavailable.title");
  });

  it("keeps interpolation tokens in parity for every nested catalog value", () => {
    for (const namespace of NAMESPACES) {
      const chinese = new Map(catalogStrings(resources["zh-CN"][namespace]));
      const english = new Map(catalogStrings(resources["en-US"][namespace]));

      for (const key of chinese.keys()) {
        expect(
          interpolationTokens(english.get(key) ?? ""),
          `${namespace}.${key} interpolation tokens`,
        ).toEqual(interpolationTokens(chinese.get(key) ?? ""));
      }
    }
  });

  it("keeps each locale and namespace in its own static module", () => {
    expect(RESOURCES_SOURCE).toContain('from "./locales/zh-CN"');
    expect(RESOURCES_SOURCE).toContain('from "./locales/en-US"');
    expect(RESOURCES_SOURCE).not.toContain("展开侧栏");
    expect(RESOURCES_SOURCE).not.toContain("Expand sidebar");

    for (const source of [
      ZH_ACCOUNTS_SOURCE,
      ZH_COMMON_SOURCE,
      ZH_CONNECT_SOURCE,
      ZH_INBOX_SOURCE,
      ZH_LEADS_SOURCE,
      ZH_MAIL_SOURCE,
      ZH_PAIRING_SOURCE,
      ZH_SIDEBAR_SOURCE,
      ZH_SHELL_SOURCE,
      EN_ACCOUNTS_SOURCE,
      EN_COMMON_SOURCE,
      EN_CONNECT_SOURCE,
      EN_INBOX_SOURCE,
      EN_LEADS_SOURCE,
      EN_MAIL_SOURCE,
      EN_PAIRING_SOURCE,
      EN_SIDEBAR_SOURCE,
      EN_SHELL_SOURCE,
    ]) {
      expect(source).toMatch(
        /export default (accounts|common|connect|inbox|leads|mail|pairing|sidebar|shell)/,
      );
    }
  });
});
