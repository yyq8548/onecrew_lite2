import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import i18next from "i18next";
import { I18nextProvider } from "react-i18next";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import zh from "@/i18n/locales/zh-CN/shell";
import en from "@/i18n/locales/en-US/shell";
import { discoveryReviewKey } from "@/lib/discovery-review";
import { reviewFixture, reviewRequest, tiktokReviewFixture, xReviewFixture } from "@/lib/discovery-review.fixture";
import { buildReviewFindings, reviewCitation, reviewCriteria, ReviewMaterialContent, SavedReviewMaterial } from "./discovery-review";

describe("audited review input", () => {
  it.each(["zh-CN", "en-US"])("labels rounded native followers as approximate in %s", async locale => {
    const material = reviewFixture();
    material.native_follower_estimate = { display: "43.9万", nominal: 439000, lower: 438000, upper: 440000 };
    const i18n = i18next.createInstance();
    await i18n.init({ lng: locale, resources: { "zh-CN": { shell: zh }, "en-US": { shell: en } } });
    const html = renderToStaticMarkup(<I18nextProvider i18n={i18n}><ReviewMaterialContent material={material} /></I18nextProvider>);
    expect(html).toContain(locale === "zh-CN" ? "约43.9万" : "approximately 43.9万");
    expect(html).toContain("438,000");
    expect(html).toContain("440,000");
  });
  it("reviews required language, leaves unrestricted language alone, and requires speech transcripts", () => {
    const material = reviewFixture();
    material.criteria = { language: "spoken English", requires_ai_tool_demo: false, requires_public_tool_url: false };
    expect(reviewCriteria(material)).toEqual(["content_language"]);
    const draft = { outcome: "supported" as const, rationale: "English original speech",
      citation: "description", quote: material.video_description!, toolName: "", corroboration: "", corroborationQuote: "", countryCode: "" };
    expect(buildReviewFindings(material, { content_language: draft })).toBeNull();
    const cited = { ...draft, citation: "transcript:0", quote: material.transcript!.segments[0].text };
    expect(buildReviewFindings(material, { content_language: cited })?.[0].criterion).toBe("content_language");
    material.criteria = { language: "不限", requires_ai_tool_demo: false, requires_public_tool_url: false };
    expect(reviewCriteria(material)).toEqual([]);
    expect(buildReviewFindings(material, {})).toEqual([]);
  });

  it("requires a reason and author citation for optional fit and can leave fit unrated", () => {
    const material = reviewFixture();
    material.criteria = { requires_ai_tool_demo: false, requires_public_tool_url: false };
    const draft = { outcome: "supported" as const, rationale: "Direct relevant product evidence",
      citation: "description", quote: material.video_description!, toolName: "", corroboration: "", corroborationQuote: "", countryCode: "" };
    expect(buildReviewFindings(material, { match_fit: draft })).toBeNull();
    expect(buildReviewFindings(material, { match_fit: { ...draft, matchLevel: "good" } })?.[0]).toMatchObject({ criterion: "match_fit", match_level: "good" });
    expect(buildReviewFindings(material, {})).toEqual([]);
  });
  it("requires exact saved citation and does not review unrelated criteria", () => {
    const material = reviewFixture();
    material.criteria = { requires_ai_tool_demo: true, requires_public_tool_url: false };
    expect(reviewCriteria(material)).toEqual(["own_ai_tool_demo"]);
    expect(reviewCitation(material, "transcript:0", "<script>transcript</script>")).toEqual({
      kind: "transcript", segment_index: 0, quote: "<script>transcript</script>",
    });
    expect(reviewCitation(material, "transcript:0", "invented demo")).toBeNull();
    expect(reviewCitation(material, "page:0", "[unsafe]")).toEqual({
      kind: "public_page", requested_url: "https://example.com/tool", quote: "[unsafe]",
    });
    expect(buildReviewFindings(material, {
      own_ai_tool_demo: { outcome: "supported", rationale: "Observed first-hand steps", citation: "description",
        quote: "<script>description</script>", toolName: "", corroboration: "", corroborationQuote: "", countryCode: "" },
      public_tool_url: { outcome: "insufficient", rationale: "", citation: "", quote: "", toolName: "", corroboration: "", corroborationQuote: "", countryCode: "" },
      apparel_business_content: { outcome: "insufficient", rationale: "", citation: "", quote: "", toolName: "", corroboration: "", corroborationQuote: "", countryCode: "" },
      category_content: { outcome: "insufficient", rationale: "", citation: "", quote: "", toolName: "", corroboration: "", corroborationQuote: "", countryCode: "" },
      creator_residence_outside_mainland: { outcome: "insufficient", rationale: "", citation: "", quote: "", toolName: "", corroboration: "", corroborationQuote: "", countryCode: "" },
    })).toBeNull();
    expect(buildReviewFindings(material, {
      own_ai_tool_demo: { outcome: "insufficient", rationale: "No saved visual evidence", citation: "", quote: "", toolName: "", corroboration: "", corroborationQuote: "", countryCode: "" },
      public_tool_url: { outcome: "insufficient", rationale: "", citation: "", quote: "", toolName: "", corroboration: "", corroborationQuote: "", countryCode: "" },
      apparel_business_content: { outcome: "insufficient", rationale: "", citation: "", quote: "", toolName: "", corroboration: "", corroborationQuote: "", countryCode: "" },
      category_content: { outcome: "insufficient", rationale: "", citation: "", quote: "", toolName: "", corroboration: "", corroborationQuote: "", countryCode: "" },
      creator_residence_outside_mainland: { outcome: "insufficient", rationale: "", citation: "", quote: "", toolName: "", corroboration: "", corroborationQuote: "", countryCode: "" },
    })).toEqual([{ criterion: "own_ai_tool_demo", outcome: "insufficient", rationale: "No saved visual evidence", citations: [], tool: null }]);
  });

  it("requires first-person apparel evidence and separate residence corroboration", () => {
    const material = reviewFixture();
    material.criteria = { requires_ai_tool_demo: false, requires_public_tool_url: false, category: "服装", region: "中国大陆以外" };
    material.transcript!.segments[0].text = "I live in Toronto and run a clothing export business.";
    material.public_pages[0].markdown_excerpt = "The founder lives in Toronto, Canada.";
    expect(reviewCriteria(material)).toEqual(["apparel_business_content", "creator_residence_outside_mainland"]);
    const blank = { outcome: "insufficient" as const, rationale: "", citation: "", quote: "", toolName: "", corroboration: "", corroborationQuote: "", countryCode: "" };
    const drafts = {
      own_ai_tool_demo: blank, public_tool_url: blank, category_content: blank,
      apparel_business_content: { ...blank, outcome: "supported" as const, rationale: "Author's apparel export business",
        citation: "transcript:0", quote: "run a clothing export business" },
      creator_residence_outside_mainland: { ...blank, outcome: "supported" as const, rationale: "Current residence corroborated",
        citation: "transcript:0", quote: "I live in Toronto", corroboration: "page:0",
        corroborationQuote: "founder lives in Toronto", countryCode: "CA" },
    };
    expect(buildReviewFindings(material, { ...drafts, creator_residence_outside_mainland: { ...drafts.creator_residence_outside_mainland, corroboration: "" } })).toBeNull();
    expect(buildReviewFindings(material, drafts)?.[1]).toMatchObject({ criterion: "creator_residence_outside_mainland",
      country_code: "CA", citations: [{ kind: "transcript" }, { kind: "public_page" }] });
  });
  it("offers the frozen shoe category for review without inventing location requirements", () => {
    const material = reviewFixture();
    material.criteria = { min_subscribers: 10000, category: "鞋子", region: "", language: "",
      requires_ai_tool_demo: false, requires_public_tool_url: false };
    material.video_description = "I compare the fit and sole of these running shoes.";
    expect(reviewCriteria(material)).toEqual(["category_content"]);
    const blank = { outcome: "insufficient" as const, rationale: "", citation: "", quote: "",
      toolName: "", corroboration: "", corroborationQuote: "", countryCode: "" };
    const drafts = { own_ai_tool_demo: blank, public_tool_url: blank,
      apparel_business_content: blank, creator_residence_outside_mainland: blank,
      category_content: { ...blank, outcome: "supported" as const,
        rationale: "Author compares shoe fit and sole in their saved video.",
        citation: "description", quote: "compare the fit and sole of these running shoes" } };
    expect(buildReviewFindings(material, drafts)).toEqual([{ criterion: "category_content",
      outcome: "supported", rationale: drafts.category_content.rationale,
      citations: [{ kind: "description", quote: drafts.category_content.quote }], tool: null }]);
    expect(buildReviewFindings(material, { ...drafts,
      category_content: { ...drafts.category_content, quote: "invented shoe demonstration" } })).toBeNull();
  });
  it("uses an exact native X post quote for apparel, while residence still needs corroboration", () => {
    const material = xReviewFixture();
    const blank = { outcome: "insufficient" as const, rationale: "No independent residence evidence", citation: "",
      quote: "", toolName: "", corroboration: "", corroborationQuote: "", countryCode: "" };
    const drafts = { own_ai_tool_demo: blank, public_tool_url: blank, category_content: blank,
      apparel_business_content: { ...blank, outcome: "supported" as const, rationale: "Author's clothing business",
        citation: "native", quote: "clothing</script> brand" },
      creator_residence_outside_mainland: blank };
    expect(reviewCitation(material, "native", "invented brand")).toBeNull();
    expect(buildReviewFindings(material, drafts)?.[0]).toMatchObject({ criterion: "apparel_business_content",
      citations: [{ kind: "native_content", quote: "clothing</script> brand" }] });
    expect(buildReviewFindings(material, { ...drafts, creator_residence_outside_mainland: {
      ...blank, outcome: "supported", citation: "native", quote: "clothing", countryCode: "CA",
    } })).toBeNull();
  });
  it("accepts an exact TikTok search caption for apparel, never for residence", () => {
    const material = tiktokReviewFixture();
    const blank = { outcome: "insufficient" as const, rationale: "No independent residence evidence", citation: "",
      quote: "", toolName: "", corroboration: "", corroborationQuote: "", countryCode: "" };
    const drafts = { own_ai_tool_demo: blank, public_tool_url: blank, category_content: blank,
      apparel_business_content: { ...blank, outcome: "supported" as const, rationale: "Author describes clothing wholesale",
        citation: "caption", quote: "dresses</script> to international wholesale" },
      creator_residence_outside_mainland: blank };
    expect(reviewCitation(material, "caption", "invented caption")).toBeNull();
    expect(buildReviewFindings(material, drafts)?.[0]).toMatchObject({ criterion: "apparel_business_content",
      citations: [{ kind: "search_caption", quote: "dresses</script> to international wholesale" }] });
    expect(buildReviewFindings(material, { ...drafts, creator_residence_outside_mainland: {
      ...blank, outcome: "supported", citation: "caption", quote: "dresses", countryCode: "CA",
    } })).toBeNull();
  });
  it("accepts a source-bound TikTok shoe caption for the frozen category", () => {
    const material = tiktokReviewFixture();
    material.criteria = { min_subscribers: 10000, category: "鞋子", region: "", language: "",
      requires_ai_tool_demo: false, requires_public_tool_url: false };
    material.hit.title = "I compare these running shoes on foot.";
    const blank = { outcome: "insufficient" as const, rationale: "", citation: "", quote: "",
      toolName: "", corroboration: "", corroborationQuote: "", countryCode: "" };
    const drafts = { own_ai_tool_demo: blank, public_tool_url: blank,
      apparel_business_content: blank, creator_residence_outside_mainland: blank,
      category_content: { ...blank, outcome: "supported" as const,
        rationale: "Native author compares running shoes in the saved caption.",
        citation: "caption", quote: "compare these running shoes" } };
    expect(buildReviewFindings(material, drafts)?.[0]).toMatchObject({ criterion: "category_content",
      citations: [{ kind: "search_caption", quote: "compare these running shoes" }] });
    expect(buildReviewFindings(material, { ...drafts,
      category_content: { ...drafts.category_content, quote: "fabricated shoe review" } })).toBeNull();
  });
});

describe("review evidence drawer", () => {
  for (const [locale, catalog] of [["zh-CN", zh], ["en-US", en]] as const) {
    const localize = async () => {
      const i18n = i18next.createInstance();
      await i18n.init({ lng: locale, resources: { [locale]: { shell: catalog } } });
      return i18n;
    };
    it(`starts closed with no query or visible cached evidence in ${locale}`, async () => {
      const client = new QueryClient();
      const key = discoveryReviewKey(reviewRequest);
      client.setQueryData(key, reviewFixture());
      const html = renderToStaticMarkup(<I18nextProvider i18n={await localize()}><QueryClientProvider client={client}>
        <SavedReviewMaterial request={reviewRequest} title="Observed demo" />
      </QueryClientProvider></I18nextProvider>);
      expect(html).toContain("Observed demo");
      expect(html).not.toContain("&lt;script&gt;");
      expect(client.getQueryState(key)?.fetchStatus).toBe("idle");
      client.clear();
    });
    for (const status of ["reading", "completed", "unavailable", "failed"] as const) {
      it(`shows ${status} without interpreting source markup in ${locale}`, async () => {
        const packet = reviewFixture();
        packet.transcript!.status = status;
        packet.transcript!.stop_code = status === "failed" ? "opencli_timeout" : null;
        const html = renderToStaticMarkup(<I18nextProvider i18n={await localize()}><ReviewMaterialContent material={packet} /></I18nextProvider>);
        expect(html).toContain(catalog.opencliPilot.reviewStatus[status]);
        expect(html).toContain(catalog.opencliPilot.reviewLimit);
        expect(html).toContain("&lt;script&gt;description&lt;/script&gt;");
        expect(html).not.toContain("<script>");
        expect(html).not.toContain("href=");
        expect(html.includes("&lt;script&gt;transcript&lt;/script&gt;")).toBe(status === "completed");
        expect(html.includes(catalog.opencliPilot.reviewTruncated)).toBe(status === "completed");
        if (status === "failed") expect(html).toContain("opencli_timeout");
      });
    }
    it(`does not fabricate missing observations in ${locale}`, async () => {
      const packet = reviewFixture();
      packet.transcript = null; packet.public_pages = []; packet.video_description = null;
      const html = renderToStaticMarkup(<I18nextProvider i18n={await localize()}><ReviewMaterialContent material={packet} /></I18nextProvider>);
      expect(html.split(catalog.opencliPilot.reviewMissing)).toHaveLength(4);
      expect(html).not.toContain(catalog.opencliPilot.reviewStatus.completed);
    });
    it(`renders native X content as text in ${locale}`, async () => {
      const html = renderToStaticMarkup(<I18nextProvider i18n={await localize()}>
        <ReviewMaterialContent material={xReviewFixture()} />
      </I18nextProvider>);
      expect(html).toContain(catalog.opencliPilot.reviewNativeContent);
      expect(html).toContain(catalog.opencliPilot.reviewNativeProfileLimit);
      expect(html).toContain("New York, NY");
      expect(html).toContain("15000");
      expect(html).toContain("&lt;script&gt;clothing&lt;/script&gt;");
      expect(html).not.toContain("<script>");
    });
    it(`renders native artwork text and exact citations without inventing a transcript in ${locale}`, async () => {
      const packet = { ...reviewFixture(), format_version: 2, video_description: null, transcript: null, public_pages: [],
        hit: { ...reviewFixture().hit, platform: "pixiv" },
        native_content: { claim: { campaign_id: "campaign", expected_revision: 1, search_hit_id: "hit", platform: "pixiv" as const,
          profile_ref: "dedicated", author_id: "123", source_url: "https://www.pixiv.net/artworks/456", call_number: 2 },
          status: "completed" as const, stop_code: null, captured_at: 1, native_content: { post_id: "456", author_id: "123",
            source_url: "https://www.pixiv.net/artworks/456", text: "原文 日本語 <script>caption</script>", text_truncated: false } } };
      const html = renderToStaticMarkup(<I18nextProvider i18n={await localize()}>
        <ReviewMaterialContent material={packet} />
      </I18nextProvider>);
      expect(html).toContain("原文 日本語 &lt;script&gt;caption&lt;/script&gt;");
      expect(html).not.toContain("<script>");
      expect(html).not.toContain(catalog.opencliPilot.reviewTranscript);
      expect(reviewCitation(packet, "native", "日本語")).toEqual({ kind: "native_content", quote: "日本語" });
      expect(reviewCitation(packet, "native", "fabricated text")).toBeNull();
    });
    it(`labels TikTok search caption limits in ${locale}`, async () => {
      const html = renderToStaticMarkup(<I18nextProvider i18n={await localize()}>
        <ReviewMaterialContent material={tiktokReviewFixture()} />
      </I18nextProvider>);
      expect(html).toContain(catalog.opencliPilot.reviewSearchCaptionLimit);
      expect(html).toContain("&lt;script&gt;dresses&lt;/script&gt;");
      expect(html).not.toContain("<script>");
    });
  }
});
