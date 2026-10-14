import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import i18next from "i18next";
import { I18nextProvider } from "react-i18next";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import zh from "@/i18n/locales/zh-CN/shell";
import en from "@/i18n/locales/en-US/shell";
import { discoveryPublicPageKey, publicPageRefreshInterval, type DiscoveryPublicPageRead } from "@/lib/opencli-discovery";
import { PublicPageReceipt, SavedPublicPageEvidence } from "./discovery-public-page";

const receipt: DiscoveryPublicPageRead = {
  candidate_id: "candidate", requested_url: "https://example.com/tool",
  search_hit_id: "hit", source_url: "https://www.youtube.com/watch?v=abcdef12345",
  source_excerpt: "Creator description", profile_ref: "dedicated", platform_account_id: "channel",
  call_number: 4, status: "completed", document_url: "https://example.com/actual",
  title: "Observed title", markdown_excerpt: "<script>untrusted</script> [link](javascript:alert(1))",
  truncated: true, stop_code: null, captured_at: 1,
};

describe("saved public page evidence", () => {
  it("scopes local cache by Campaign, candidate and exact URL", () => {
    const key = discoveryPublicPageKey("campaign", "candidate", receipt.requested_url);
    const client = new QueryClient();
    client.setQueryData(key, receipt);
    expect(client.getQueryData(key)).toEqual(receipt);
    for (const other of [
      discoveryPublicPageKey("other", "candidate", receipt.requested_url),
      discoveryPublicPageKey("campaign", "other", receipt.requested_url),
      discoveryPublicPageKey("campaign", "candidate", receipt.requested_url + "/"),
    ]) expect(client.getQueryData(other)).toBeUndefined();
    client.clear();
  });

  it("bounds polling and never automatically retries missing or failed receipts", () => {
    expect(publicPageRefreshInterval("reading", 1, false)).toBe(5000);
    expect(publicPageRefreshInterval("reading", 11, false)).toBe(5000);
    expect(publicPageRefreshInterval("reading", 12, false)).toBe(false);
    expect(publicPageRefreshInterval("reading", 2, true)).toBe(false);
    for (const status of [undefined, "completed", "failed"] as const)
      expect(publicPageRefreshInterval(status, 0, false)).toBe(false);
  });

  for (const [locale, catalog] of [["zh-CN", zh], ["en-US", en]] as const) {
    it(`starts closed without querying or exposing cached evidence in ${locale}`, async () => {
      const i18n = i18next.createInstance();
      await i18n.init({ lng: locale, resources: { [locale]: { shell: catalog } } });
      const client = new QueryClient();
      const key = discoveryPublicPageKey("campaign", "candidate", receipt.requested_url);
      client.setQueryData(key, receipt);
      const html = renderToStaticMarkup(<I18nextProvider i18n={i18n}><QueryClientProvider client={client}>
        <SavedPublicPageEvidence campaignId="campaign" candidateId="candidate" requestedUrl={receipt.requested_url} />
      </QueryClientProvider></I18nextProvider>);
      expect(html).toContain(catalog.opencliPilot.savedPublicPage);
      expect(html).not.toContain(receipt.title);
      expect(client.getQueryState(key)?.fetchStatus).toBe("idle");
      client.clear();
    });

    for (const status of ["reading", "completed", "failed"] as const) {
      it(`renders ${status} faithfully as untrusted evidence in ${locale}`, async () => {
        const i18n = i18next.createInstance();
        await i18n.init({ lng: locale, resources: { [locale]: { shell: catalog } } });
        const read = status === "completed" ? receipt : {
          ...receipt, status, document_url: null, title: "", markdown_excerpt: "", truncated: false,
          stop_code: status === "failed" ? "opencli_invalid_output" : null,
        };
        const html = renderToStaticMarkup(<I18nextProvider i18n={i18n}><PublicPageReceipt read={read} /></I18nextProvider>);
        expect(html).toContain(catalog.opencliPilot.publicPageStatus[status]);
        expect(html).toContain(catalog.opencliPilot.publicPageLimit);
        expect(html).toContain(receipt.source_url);
        expect(html).toContain(receipt.requested_url);
        expect(html).not.toContain("<script>");
        expect(html).not.toContain("href=");
        if (status === "completed") {
          expect(html).toContain("&lt;script&gt;untrusted&lt;/script&gt;");
          expect(html).toContain(catalog.opencliPilot.publicPageTruncated);
          expect(html).toContain(receipt.document_url);
        } else {
          expect(html).not.toContain(receipt.title);
          expect(html).not.toContain(catalog.opencliPilot.publicPageTruncated);
        }
        if (status === "failed") expect(html).toContain("opencli_invalid_output");
      });
    }
  }
});
