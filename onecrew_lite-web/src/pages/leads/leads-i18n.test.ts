import i18next from "i18next";
import { describe, expect, it } from "vitest";

import enUSLeads from "@/i18n/locales/en-US/leads";
import zhCNLeads from "@/i18n/locales/zh-CN/leads";
import CONTACT_ROUTES_SOURCE from "@/components/contact-routes.tsx?raw";
import LEADS_SOURCE from "@/pages/Leads.tsx?raw";
import BULK_MOVE_SOURCE from "./creator-bulk-move-bar.tsx?raw";
import CONTACT_ROUTE_FIELDS_SOURCE from "./contact-route-fields.tsx?raw";
import CONTACT_ROUTE_MUTATIONS_SOURCE from "./contact-route-mutation-dialogs.tsx?raw";
import GROUP_DIALOGS_SOURCE from "./creator-group-dialogs.tsx?raw";
import GROUPS_VIEW_SOURCE from "./creator-groups-view.tsx?raw";
import MANUAL_CONTACT_SOURCE from "./manual-contact-route-dialog.tsx?raw";
import MANUAL_CREATOR_SOURCE from "./manual-creator-dialog.tsx?raw";
import TRANSFER_SOURCE from "./creator-transfer-menu.tsx?raw";

function catalogKeys(value: unknown, prefix = ""): string[] {
  if (typeof value !== "object" || value == null) return [prefix];
  return Object.entries(value).flatMap(([key, child]) =>
    catalogKeys(child, prefix ? `${prefix}.${key}` : key));
}

function withoutComments(source: string) {
  return source
    .replace(/\/\*[\s\S]*?\*\//g, "")
    .replace(/\/\/.*$/gm, "");
}

const UI_SOURCES = [
  CONTACT_ROUTES_SOURCE,
  LEADS_SOURCE,
  BULK_MOVE_SOURCE,
  CONTACT_ROUTE_FIELDS_SOURCE,
  CONTACT_ROUTE_MUTATIONS_SOURCE,
  GROUP_DIALOGS_SOURCE,
  GROUPS_VIEW_SOURCE,
  MANUAL_CONTACT_SOURCE,
  MANUAL_CREATOR_SOURCE,
  TRANSFER_SOURCE,
];

describe("Leads translations", () => {
  it("keeps the complete English and Chinese catalog shapes in parity", () => {
    expect(catalogKeys(enUSLeads).sort()).toEqual(catalogKeys(zhCNLeads).sort());
  });

  it("switches fixed copy while preserving dynamic Creator and provider data byte-for-byte", async () => {
    const instance = i18next.createInstance();
    await instance.init({
      fallbackLng: "zh-CN",
      interpolation: { escapeValue: false },
      lng: "zh-CN",
      ns: ["leads"],
      defaultNS: "leads",
      resources: {
        "zh-CN": { leads: zhCNLeads },
        "en-US": { leads: enUSLeads },
      },
    });

    const creatorName = "Ada 陈 <creator@example.com>";
    const providerError = "Resend 401 / request_id=req_7f";
    expect(instance.t("archiveDialog.description", { name: creatorName })).toContain(creatorName);
    expect(instance.t("table.readError", { error: providerError })).toBe(`读取 Creator 失败：${providerError}`);

    await instance.changeLanguage("en-US");

    expect(instance.t("table.empty")).toBe("No Creators match the current filters");
    expect(instance.t("archiveDialog.description", { name: creatorName })).toContain(creatorName);
    expect(instance.t("table.readError", { error: providerError })).toBe(`Could not load Creators: ${providerError}`);
  });

  it("binds every copy-bearing Leads component to the leads namespace", () => {
    for (const source of UI_SOURCES) {
      expect(source).toContain('useTranslation("leads")');
      expect(withoutComments(source)).not.toMatch(/[一-龥]/);
    }
  });

  it("keeps provider errors and user reply text outside translation catalogs", () => {
    expect(LEADS_SOURCE).toContain("description: String(error)");
    expect(BULK_MOVE_SOURCE).toContain("description: String(cause)");
    expect(TRANSFER_SOURCE).toContain("description: String(cause)");
    expect(JSON.stringify({ zhCNLeads, enUSLeads })).not.toContain("creator@example.com");
    expect(JSON.stringify({ zhCNLeads, enUSLeads })).not.toContain("request_id");
  });
});
