import { describe, expect, it } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import { createInstance } from "i18next";
import { I18nextProvider } from "react-i18next";
import { DiscoveryManualContacts } from "./discovery-manual-contacts";
import type { DiscoveryCandidate } from "@/lib/opencli-discovery";
import zh from "@/i18n/locales/zh-CN/shell";
import en from "@/i18n/locales/en-US/shell";

describe("manual contacts", () => {
  it("shows source-bound WeChat as manual text, excludes another author", async () => {
    const i18n = createInstance();
    await i18n.init({ lng: "zh-CN", resources: { "zh-CN": { shell: zh } } });
    const candidate: DiscoveryCandidate = { id: "wechat", platform: "zhihu", platform_account_id: "native",
      display_name: "Author", profile_url: "https://www.zhihu.com/people/author", verdict: "pending",
      evidence: {source_urls: [], manual_wechat_accounts: [
        {kind: "wechat", value: "public123", source_url: "https://www.zhihu.com/people/author", native_author_id: "native"},
        {kind: "wechat_official", value: "公众号名称", source_url: "https://www.zhihu.com/people/author", native_author_id: "native"},
        {kind: "wechat", value: "wrong123", source_url: "https://www.zhihu.com/people/other", native_author_id: "other"},
      ]} };
    const html = renderToStaticMarkup(<I18nextProvider i18n={i18n}><DiscoveryManualContacts candidates={[candidate]} /></I18nextProvider>);
    expect(html).toContain("公开微信"); expect(html).toContain("public123");
    expect(html).toContain("公开公众号"); expect(html).toContain("公众号名称");
    expect(html).not.toContain("wrong123"); expect(html).not.toContain("mailto:");
    expect(html).not.toContain('href="public123"');
  });
  it.each(["zh-CN", "en-US"])("shows actionable real %s copy without send controls", async locale => {
    const i18n = createInstance();
    await i18n.init({ lng: locale, resources: { "zh-CN": { shell: zh }, "en-US": { shell: en } } });
    const html = renderToStaticMarkup(<I18nextProvider i18n={i18n}>
      <DiscoveryManualContacts candidates={[socialCandidate()]} />
    </I18nextProvider>);
    const copy = locale === "zh-CN" ? zh : en;
    expect(html).toContain(copy.opencliPilot.manualContactsTitle);
    expect(html).toContain(copy.opencliPilot.manualContactsSteps);
    expect(html).toContain(copy.opencliPilot.manualContactsHelp);
    expect(html).not.toContain("opencliPilot.");
    expect(html).not.toMatch(/<button|<form|mailto:|onclick=/);
  });
  const socialCandidate = (): DiscoveryCandidate => ({
    id: "creator", platform: "youtube", platform_account_id: "native-author",
    display_name: "Creator", profile_url: "https://www.youtube.com/@creator",
    verdict: "pending", evidence: {
      source_urls: [], contact_status: { has_contact: false, manual_email_required: true },
      associated_social_accounts: [{
        kind: "social_profile", normalized_value: "https://www.instagram.com/creator/",
        source_url: "https://www.youtube.com/@creator/about", captured_at: 1,
        native_author_id: "native-author", attribution: "author_published",
      }],
    },
  });
  const render = async (candidate: DiscoveryCandidate) => {
    const i18n = createInstance();
    await i18n.init({ lng: "en-US", resources: {} });
    return renderToStaticMarkup(<I18nextProvider i18n={i18n}>
      <DiscoveryManualContacts candidates={[candidate]} />
    </I18nextProvider>);
  };

  it("combines hidden email and associated accounts in one table and deduplicates a source", async () => {
    const candidate = socialCandidate();
    const account = candidate.evidence.associated_social_accounts![0];
    candidate.evidence.associated_social_accounts!.push({ ...account });
    const html = await render(candidate);
    expect(html.match(/<table\b/g)).toHaveLength(1);
    expect(html.match(/<tr\b/g)).toHaveLength(3); // Header, hidden entry, associated account.
    expect(html).toContain("manualContactsHidden");
    expect(html).toContain("socialAccountsPublished");
    expect(html.match(/href="https:\/\/www.instagram.com\/creator\/"/g)).toHaveLength(1);
  });

  it("includes owner-approved LinkedIn profiles with or without email alongside YouTube", async () => {
    const i18n = createInstance();
    await i18n.init({ lng: "en-US", resources: {} });
    const linkedin: DiscoveryCandidate = {
      id: "linkedin", platform: "linkedin", platform_account_id: "ACoAAA123456789abcdefghijk",
      display_name: "LinkedIn Author", profile_url: "https://www.linkedin.com/in/native-author/",
      verdict: "qualified", evidence: { source_urls: [], manual_profile_contact: true,
        contact_status: { has_contact: false, manual_email_required: false } },
    };
    const html = renderToStaticMarkup(<I18nextProvider i18n={i18n}>
      <DiscoveryManualContacts candidates={[socialCandidate(), linkedin]} />
    </I18nextProvider>);
    expect(html.match(/<table\b/g)).toHaveLength(1);
    expect(html).toContain("LinkedIn Author");
    expect(html).toContain("manualContactsLinkedInSteps");
    expect(html).not.toMatch(/<button|<form|mailto:/);
    linkedin.evidence.contact_status!.has_contact = true;
    expect(await render(linkedin)).toContain("LinkedIn Author");
    linkedin.evidence.manual_profile_contact = false;
    expect(await render(linkedin)).toBe("");
    delete linkedin.evidence.manual_profile_contact;
    expect(await render(linkedin)).toBe("");
  });

  it("rejects foreign authors and unsafe links without hiding an unconfirmed matching account", async () => {
    const candidate = socialCandidate();
    candidate.evidence.contact_status!.manual_email_required = false;
    const account = candidate.evidence.associated_social_accounts![0];
    candidate.evidence.associated_social_accounts = [
      { ...account, native_author_id: "other-author", normalized_value: "https://www.instagram.com/foreign/" },
      { ...account, normalized_value: "javascript:alert(1)" },
      { ...account, source_url: "https://user:password@example.com/" },
      { ...account, attribution: "unconfirmed" },
    ];
    const html = await render(candidate);
    expect(html.match(/<tr\b/g)).toHaveLength(2);
    expect(html).toContain("socialAccountsUnconfirmed");
    expect(html).not.toContain("socialAccountsPublished");
    expect(html).not.toContain("foreign");
    expect(html).not.toContain("javascript:");
    expect(html).not.toContain("password");
  });

  it("shows only verified unresolved email entries, with a native profile link", async () => {
    const i18n = createInstance();
    await i18n.init({ lng: "en-US", resources: {} });
    const candidate = (name: string, required: boolean): DiscoveryCandidate => ({
      id: name, platform: "youtube", platform_account_id: name,
      display_name: name, profile_url: `https://www.youtube.com/@${name}`,
      verdict: "qualified", evidence: { source_urls: [], contact_status: { has_contact: !required, manual_email_required: required } },
    });
    const html = renderToStaticMarkup(<I18nextProvider i18n={i18n}><DiscoveryManualContacts
      candidates={[candidate("HiddenEntry", true), candidate("ActualEmail", false)]}
    /></I18nextProvider>);
    expect(html).toContain("https://www.youtube.com/@HiddenEntry");
    expect(html).not.toContain("ActualEmail");
    expect(html).toContain("manualContactsSteps");
  });
});
