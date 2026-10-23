import { describe, expect, it } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import { createInstance } from "i18next";
import { I18nextProvider } from "react-i18next";
import { DiscoveryManualContacts } from "./discovery-manual-contacts";
import type { DiscoveryCandidate } from "@/lib/opencli-discovery";

describe("associated social accounts", () => {
  it("shows sources and uncertainty for pending creators, excluding unsafe links", async () => {
    const i18n = createInstance();
    await i18n.init({ lng: "en-US", resources: {} });
    const account = (url: string) => ({ kind: "social_profile" as const, normalized_value: url,
      source_url: "https://creator.example/contact", captured_at: 1, native_author_id: "creator",
      attribution: "unconfirmed" as const });
    const candidate: DiscoveryCandidate = { id: "candidate", platform: "youtube",
      platform_account_id: "creator", display_name: "Creator", profile_url: "https://youtube.com/@creator",
      verdict: "pending", evidence: { source_urls: [], contact_status: { has_contact: false, manual_email_required: true }, associated_social_accounts: [
        account("https://instagram.com/creator"), account("https://x.com/creator"),
        account("https://x.com/creator"),
        { ...account("https://x.com/wrongauthor"), native_author_id: "other" },
        account("https://user:password@x.com/unsafe"),
      ] } };
    const html = renderToStaticMarkup(<I18nextProvider i18n={i18n}>
      <DiscoveryManualContacts candidates={[candidate]} />
    </I18nextProvider>);
    expect(html).toContain("https://instagram.com/creator");
    expect(html).toContain("https://x.com/creator");
    expect(html).toContain("https://creator.example/contact");
    expect(html).toContain("socialAccountsUnconfirmed");
    expect(html).toContain("manualContactsPending");
    expect(html.match(/<table\b/g)).toHaveLength(1);
    expect(html.match(/href="https:\/\/x.com\/creator"/g)).toHaveLength(1);
    expect(html).not.toContain("wrongauthor");
    expect(html).not.toContain("unsafe");
    expect(html).not.toContain("password");
  });
  it("hides the section when no associated account has been observed", async () => {
    const i18n = createInstance();
    await i18n.init({ lng: "en-US", resources: {} });
    expect(renderToStaticMarkup(<I18nextProvider i18n={i18n}>
      <DiscoveryManualContacts candidates={[]} />
    </I18nextProvider>)).toBe("");
  });
});
