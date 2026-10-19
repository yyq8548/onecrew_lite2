import i18next from "i18next";
import { describe, expect, it } from "vitest";

import RECIPIENT_SOURCE from "./recipient-picker.tsx?raw";
import WORKSPACE_SOURCE from "./mail-workspace.tsx?raw";
import { EmailTemplatePaginationLimitError } from "./queries";
import { emailTemplateLoadErrorDescription } from "./template-error-presentation";
import MAIL_PAGE_SOURCE from "../../pages/Mail.tsx?raw";
import enUSMail from "../../i18n/locales/en-US/mail";
import zhCNMail from "../../i18n/locales/zh-CN/mail";

function catalogKeys(value: unknown, prefix = ""): string[] {
  if (!value || typeof value !== "object") return prefix ? [prefix] : [];
  return Object.entries(value).flatMap(([key, child]) => (
    catalogKeys(child, prefix ? `${prefix}.${key}` : key)
  ));
}

function catalogStrings(value: unknown): string[] {
  if (typeof value === "string") return [value];
  if (!value || typeof value !== "object") return [];
  return Object.values(value).flatMap(catalogStrings);
}

describe("Mail i18n contract", () => {
  it("keeps the zh-CN and en-US catalogs structurally identical", () => {
    expect(catalogKeys(enUSMail).sort()).toEqual(catalogKeys(zhCNMail).sort());
    expect(catalogStrings(enUSMail).join("\n")).not.toMatch(/[\u3400-\u9fff]/u);
  });

  it("routes every Mail toast title through the mail catalog", () => {
    const source = [
      MAIL_PAGE_SOURCE,
      WORKSPACE_SOURCE,
      RECIPIENT_SOURCE,
    ].join("\n");

    expect(source).not.toMatch(/toast\.(?:success|error|info|warning)\(\s*["'`]/u);
    expect(source).toContain("description: String(error)");
  });

  it("uses the mail namespace in every stateful Mail surface", () => {
    for (const source of [
      MAIL_PAGE_SOURCE,
      WORKSPACE_SOURCE,
      RECIPIENT_SOURCE,
    ]) {
      expect(source).toContain('useTranslation("mail")');
    }
  });

  it("resolves English plurals and preserves template variable syntax", async () => {
    const instance = i18next.createInstance();
    await instance.init({
      lng: "en-US",
      resources: { "en-US": { mail: enUSMail } },
    });
    const t = instance.getFixedT("en-US", "mail");

    expect(t("folders.itemAria", { label: "Inbox", count: 1 })).toBe("Inbox, 1 item");
    expect(t("folders.itemAria", { label: "Inbox", count: 2 })).toBe("Inbox, 2 items");
    expect(t("templates.editor.subjectPlaceholder", { first_name: "{{first_name}}" }))
      .toBe("Hello {{first_name}}");

    expect(emailTemplateLoadErrorDescription(new EmailTemplatePaginationLimitError(), t))
      .toBe("The template library exceeds the safe read limit. Reduce the number of templates and try again.");
    const providerError = new Error("provider detail must stay raw");
    expect(emailTemplateLoadErrorDescription(providerError, t)).toBe(String(providerError));
  });
});
