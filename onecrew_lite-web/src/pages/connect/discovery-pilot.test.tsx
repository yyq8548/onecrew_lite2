import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import i18next from "i18next";
import { I18nextProvider } from "react-i18next";
import { renderToStaticMarkup } from "react-dom/server";
import { afterEach, describe, expect, it, vi } from "vitest";
import zh from "@/i18n/locales/zh-CN/shell";
import en from "@/i18n/locales/en-US/shell";
import { DiscoveryPilot, registeredSearchPlatforms, SEARCH_PLATFORM_GROUPS, SavedContentEvidence } from "./discovery-pilot";
import { DiscoveryResultsTable } from "./discovery-results-table";
import { DiscoveryTargetIncrease } from "./discovery-target-increase";
import { saveRunCheckpoint } from "@/lib/discovery-run-checkpoint";
import type { DiscoveryCampaign } from "@/lib/opencli-discovery";

const capabilities = [
  { platform: "youtube", command: "youtube/search", browser_required: true, registered: true },
  { platform: "bilibili", command: "onecrew-bilibili/search", browser_required: true, registered: true },
  { platform: "twitter", command: "twitter/search", browser_required: true, registered: true },
  { platform: "substack", command: "substack/search", browser_required: false, registered: true },
];

describe("Campaign paginated result presentation", () => {
  afterEach(() => { vi.unstubAllGlobals(); });
  it("keeps backend candidates out of the retired manual review queue while retaining recommendation counts", async () => {
    vi.stubGlobal("window", { localStorage: { getItem: () => "graded", setItem: () => {} } });
    const client = new QueryClient({ defaultOptions: { queries: { retry: false, staleTime: Infinity } } });
    client.setQueryData(["opencli-campaign", "graded"], { id: "graded", phase: "active", revision: 1,
      brief: "卖音响", opencli_version: "1.8.8", criteria: { min_subscribers: 8000, category: "音响", language: "中文" },
      target_count: 100, max_calls: 400, used_calls: 20, scopes: [], platform_runs: [] });
    const candidate = (name: string, verdict: string, level: string, reason: string) => ({ id: name, platform: "youtube",
      platform_account_id: name, display_name: name, profile_url: `https://www.youtube.com/@${name}`, verdict,
      evidence: { source_urls: ["https://www.youtube.com/watch?v=LR04bU_yV5k"],
        assessment: { reasons: [], subscribers_normalized: 10000,
          match_rating: { level, reason, source_url: "https://www.youtube.com/watch?v=LR04bU_yV5k" } } } });
    client.setQueryData(["opencli-campaign-candidates", "graded"], { pages: [[
      candidate("GoodAuthor", "qualified", "good", "Clear relevant reviews"),
      candidate("UnverifiedAuthor", "pending", "excellent", "Unverified rating must stay hidden"),
      candidate("ExcellentAuthor", "qualified", "excellent", "Direct product tests and a relevant advantage"),
    ]], pageParams: [null] });
    client.setQueryData(["opencli-campaign-candidate-counts", "graded"], { qualified: 2, pending: 1, rejected: 0 });
    const i18n = i18next.createInstance();
    await i18n.init({ lng: "zh-CN", resources: { "zh-CN": { shell: zh } }, defaultNS: "shell" });
    const markup = renderToStaticMarkup(<I18nextProvider i18n={i18n}><QueryClientProvider client={client}>
      <DiscoveryPilot profileRef="scope" version="1.8.8" capabilities={capabilities} />
    </QueryClientProvider></I18nextProvider>);
    expect(markup).not.toContain(">ExcellentAuthor<");
    expect(markup).not.toContain(">GoodAuthor<");
    expect(markup).not.toContain(">UnverifiedAuthor<");
    expect(markup).toContain("2/100");
    expect(markup).not.toContain(zh.opencliPilot.manualApprove);
    expect(markup).not.toContain("Unverified rating must stay hidden");
    client.clear();
  });
  it("restores the phone plan's stop reason after refresh without showing an old water plan", async () => {
    const values = new Map<string, string>([["onecrew-opencli-pilot-campaign", "phone"]]);
    const storage = { getItem: (key: string) => values.get(key) ?? null, setItem: (key: string, value: string) => { values.set(key, value); } };
    vi.stubGlobal("window", { localStorage: storage });
    const campaign = { id: "phone", phase: "active", revision: 1, brief: "卖手机架", opencli_version: "1.8.8",
      criteria: { min_subscribers: 8000 }, target_count: 30, max_calls: 200, used_calls: 11,
      scopes: [{ platform: "youtube", profile_ref: "scope", keywords: ["phone stand"] }],
      platform_runs: [{ platform: "youtube", profile_ref: "scope", status: "completed", calls_used: 1, stop_code: null }] } as DiscoveryCampaign;
    saveRunCheckpoint(storage, { ...campaign, id: "water" }, "user_paused", 1000);
    saveRunCheckpoint(storage, campaign, "search_page_left", 2000);
    const client = new QueryClient({ defaultOptions: { queries: { retry: false, staleTime: Infinity } } });
    client.setQueryData(["opencli-campaign", "phone"], campaign);
    const i18n = i18next.createInstance();
    await i18n.init({ lng: "zh-CN", resources: { "zh-CN": { shell: zh } }, defaultNS: "shell" });
    const markup = renderToStaticMarkup(<I18nextProvider i18n={i18n}><QueryClientProvider client={client}>
      <DiscoveryPilot profileRef="scope" version="1.8.8" capabilities={capabilities} />
    </QueryClientProvider></I18nextProvider>);
    expect(markup).toContain("离开搜索页后暂停");
    expect(markup).toContain("卖手机架");
    expect(markup).toContain("11");
    expect(markup).not.toContain("11/200");
    expect(markup).not.toContain("水杯");
    client.setQueryData(["opencli-campaign-candidate-counts", "phone"], { qualified: 30, pending: 0, rejected: 0 });
    const completed = renderToStaticMarkup(<I18nextProvider i18n={i18n}><QueryClientProvider client={client}>
      <DiscoveryPilot profileRef="scope" version="1.8.8" capabilities={capabilities} />
    </QueryClientProvider></I18nextProvider>);
    expect(completed).toContain("本轮搜索结束");
    client.clear();
  });
  it("shows target increases with accumulated reads and explicit identity-count limits in both locales", async () => {
    for (const [locale, catalog] of [["en-US", en], ["zh-CN", zh]] as const) {
      const i18n = i18next.createInstance();
      await i18n.init({ lng: locale, ns: ["shell"], defaultNS: "shell", resources: { [locale]: { shell: catalog } } });
      const client = new QueryClient();
      const campaign = { id: "shampoo", phase: "active", revision: 1, brief: "我要卖洗发水", opencli_version: "1.8.8",
        criteria: { category: "洗发水", min_subscribers: 10000 }, target_count: 30, max_calls: 400, used_calls: 227,
        scopes: [], platform_runs: [] };
      const markup = renderToStaticMarkup(<I18nextProvider i18n={i18n}><QueryClientProvider client={client}>
        <DiscoveryTargetIncrease campaign={campaign} busy={false} />
      </QueryClientProvider></I18nextProvider>);
      expect(markup).toContain('id="discovery-target-new"');
      expect(markup).toContain('min="1"');
      expect(markup).toContain('max="10000"');
      expect(markup).toContain("227");
      expect(markup).not.toContain("227/400");
      expect(markup).toContain(catalog.opencliPilot.targetDecisionTitle.replaceAll("'", "&#x27;"));
      expect(markup).toContain('disabled=""');
      client.clear();
    }
  });

  it("places the source-bound review action inside its result row", async () => {
    const i18n = i18next.createInstance();
    await i18n.init({ lng: "en-US", ns: ["shell"], defaultNS: "shell", resources: { "en-US": { shell: en } } });
    const client = new QueryClient();
    const markup = renderToStaticMarkup(<I18nextProvider i18n={i18n}><QueryClientProvider client={client}>
      <DiscoveryResultsTable hits={[{
        id: "tiktok-hit", platform: "tiktok", source_url: "https://www.tiktok.com/@creator/video/123",
        title: "Original apparel post", author_label: "creator", author_id: "123", author_url: "https://www.tiktok.com/@creator",
        author_observed_at: 1, search_keyword: "apparel", captured_at: 1, profile_ref: "browser",
      }]} verifiedSourceUrls={new Set()} canVerify={false} busy={false} onVerify={() => {}}
      reviewContext={{ campaignId: "campaign", revision: 1 }} />
    </QueryClientProvider></I18nextProvider>);
    expect(markup).toMatch(/<tr[^>]*>[\s\S]*?Original apparel post[\s\S]*?Review evidence[\s\S]*?<\/tr>/);
    expect(markup).toContain('aria-label="View evidence: Original apparel post"');
    expect(markup).not.toContain('class="flex flex-wrap gap-2">View evidence:');
    client.clear();
  });

  it("hides the retired evidence sections", async () => {
    vi.stubGlobal("window", { localStorage: { getItem: () => "evidence-campaign" } });
    const client = new QueryClient({ defaultOptions: { queries: { retry: false, staleTime: Infinity } } });
    client.setQueryData(["opencli-campaign", "evidence-campaign"], {
      id: "evidence-campaign", phase: "active", revision: 1, brief: "Shoes",
      criteria: { min_subscribers: 10000, region: null, language: null, category: "shoes", requires_ai_tool_demo: false, requires_public_tool_url: false },
      opencli_version: "1.8.8", target_count: 30, max_calls: 200, used_calls: 1,
      scopes: [{ platform: "bilibili", profile_ref: "scope", keywords: ["shoes"] }],
      platform_runs: [{ platform: "bilibili", profile_ref: "scope", status: "completed", calls_used: 1, stop_code: null }],
    });
    client.setQueryData(["opencli-campaign-hits", "evidence-campaign"], { pages: [[{
      id: "hit-1", platform: "bilibili", source_url: "https://www.bilibili.com/video/BV123",
      title: "Saved shoe video", author_label: "creator", author_id: "123", author_url: "https://space.bilibili.com/123",
      author_observed_at: 1, search_keyword: "shoes", captured_at: 1, profile_ref: "scope",
    }]], pageParams: [null] });
    const i18n = i18next.createInstance();
    await i18n.init({ lng: "en-US", ns: ["shell"], defaultNS: "shell", resources: { "en-US": { shell: en } } });
    const markup = renderToStaticMarkup(<I18nextProvider i18n={i18n}><QueryClientProvider client={client}>
      <DiscoveryPilot profileRef="scope" version="1.8.8" capabilities={capabilities} />
    </QueryClientProvider></I18nextProvider>);
    expect(markup).not.toContain("Saved evidence (1)");
    expect(markup).not.toMatch(/<details[^>]*\bopen(?:=|\s|>)/);
    client.clear();
  });

  it("selects registered sources but leaves unavailable sources disabled", async () => {
    vi.stubGlobal("window", { localStorage: { getItem: () => null } });
    const client = new QueryClient();
    const i18n = i18next.createInstance();
    await i18n.init({ lng: "en-US", ns: ["shell"], defaultNS: "shell", resources: { "en-US": { shell: en } } });
    const markup = renderToStaticMarkup(<I18nextProvider i18n={i18n}>
      <QueryClientProvider client={client}><DiscoveryPilot profileRef="scope" version="1.8.8" capabilities={capabilities} /></QueryClientProvider>
    </I18nextProvider>);
    expect(registeredSearchPlatforms(capabilities).sort()).toEqual(capabilities.map(item => item.platform).sort());
    expect(registeredSearchPlatforms([{ ...capabilities[0], registered: false }])).toEqual([]);
    expect(markup).toContain(en.opencliPilot.advancedSettings);
    expect(markup).not.toContain('id="discovery-keywords"');
    client.clear();
  });

  it("selects the remaining catalog without retired sources when every entry is registered", async () => {
    vi.stubGlobal("window", { localStorage: { getItem: () => null } });
    const all = SEARCH_PLATFORM_GROUPS.flatMap(group => group.platforms);
    expect(all).toHaveLength(44);
    expect(all).not.toContain("reddit");
    expect(all).not.toContain("pinterest");
    expect(all).not.toContain("zhihu");
    expect(all).not.toContain("tieba");
    expect(all).not.toContain("pixiv");
    const client = new QueryClient();
    const i18n = i18next.createInstance();
    await i18n.init({ lng: "en-US", ns: ["shell"], defaultNS: "shell", resources: { "en-US": { shell: en } } });
    const markup = renderToStaticMarkup(<I18nextProvider i18n={i18n}><QueryClientProvider client={client}>
      <DiscoveryPilot profileRef="scope" version="1.8.8"
        capabilities={all.map(platform => ({ platform, command: `${platform}/search`, browser_required: true, registered: true }))} />
    </QueryClientProvider></I18nextProvider>);
    expect(registeredSearchPlatforms(all.map(platform => ({ platform, command: `${platform}/search`, browser_required: true, registered: true })))).toEqual(all.filter(platform => !["google", "yahoo"].includes(platform)));
    expect(markup).toContain(en.opencliPilot.aiPrepare);
    expect(markup).not.toContain('id="discovery-followers"');
    expect(markup).not.toContain('id="discovery-require-demo"');
    client.clear();
  });

  for (const [locale, catalog] of [["zh-CN", zh], ["en-US", en]] as const) {
    it(`keeps saved Campaign recovery collapsed before creating another in ${locale}`, async () => {
      vi.stubGlobal("window", { localStorage: { getItem: () => null } });
      const client = new QueryClient();
      const i18n = i18next.createInstance();
      await i18n.init({ lng: locale, ns: ["shell"], defaultNS: "shell", resources: { [locale]: { shell: catalog } } });
      const markup = renderToStaticMarkup(<I18nextProvider i18n={i18n}>
        <QueryClientProvider client={client}><DiscoveryPilot profileRef="scope" version="1.8.8" capabilities={capabilities} /></QueryClientProvider>
      </I18nextProvider>);
      expect(markup).toContain(catalog.opencliPilot.restoreTitle);
      expect(markup).toContain('aria-expanded="false"');
      expect(markup).not.toContain('id="discovery-restore-id"');
      expect(markup).not.toContain(catalog.opencliPilot.restoreHelp.replaceAll("'", "&#x27;"));
      client.clear();
    });

    it(`offers task editing without a budget control while showing frozen criteria in ${locale}`, async () => {
      vi.stubGlobal("window", { localStorage: { getItem: () => "budget-campaign" } });
      const client = new QueryClient({ defaultOptions: { queries: { retry: false, staleTime: Infinity } } });
      client.setQueryData(["opencli-campaign", "budget-campaign"], {
        id: "budget-campaign", phase: "active", revision: 1, brief: "Overseas apparel",
        criteria: { min_subscribers: 10000, region: "outside mainland China", language: "any", category: "clothing", requires_ai_tool_demo: false, requires_public_tool_url: false },
        opencli_version: "1.8.8", target_count: 30, max_calls: 200, used_calls: 175,
        scopes: [{ platform: "youtube", profile_ref: "scope", keywords: ["apparel"] }],
        platform_runs: [{ platform: "youtube", profile_ref: "scope", status: "completed", calls_used: 175, stop_code: null }],
      });
      const i18n = i18next.createInstance();
      await i18n.init({ lng: locale, ns: ["shell"], defaultNS: "shell", resources: { [locale]: { shell: catalog } } });
      const markup = renderToStaticMarkup(<I18nextProvider i18n={i18n}>
        <QueryClientProvider client={client}><DiscoveryPilot profileRef="scope" version="1.8.8" capabilities={capabilities} /></QueryClientProvider>
      </I18nextProvider>);
      expect(markup).toContain(catalog.opencliPilot.adjustPlan);
      expect(markup).toContain('aria-expanded="false"');
      expect(markup).not.toContain('id="discovery-budget-new"');
      expect(markup).not.toContain('id="discovery-max-calls"');
      expect(markup).not.toContain("175/200");
      expect(markup).toContain(catalog.opencliPilot.editPlan);
      expect(markup).toContain("outside mainland China");
      expect(markup).toContain("clothing");
      expect(markup).toContain("10000");
      expect(markup).toContain(catalog.opencliPilot.automaticStart);
      expect(markup).not.toContain(catalog.opencliPilot.platformRunsDetails.replaceAll("'", "&#x27;"));
      expect(markup).not.toContain('id="discovery-active-platform"');
      expect(markup).not.toContain(catalog.opencliPilot.search);
      client.clear();
    });

    it(`shows native content as evidence, not qualification, in ${locale}`, async () => {
      const i18n = i18next.createInstance();
      await i18n.init({ lng: locale, ns: ["shell"], defaultNS: "shell", resources: { [locale]: { shell: catalog } } });
      const markup = renderToStaticMarkup(<I18nextProvider i18n={i18n}><SavedContentEvidence read={{
        claim: { campaign_id: "native", search_hit_id: "hit", expected_revision: 1, platform: "twitter",
          profile_ref: "scope", author_id: "creator", source_url: "https://x.com/i/status/123", call_number: 4 },
        status: "completed", captured_at: 1, stop_code: null,
        native_content: { adapter_version: "1.0.0", identity: { adapter_version: "1.0.0", post_id: "123",
          account_id: "987654321", handle: "creator", profile_url: "https://x.com/creator",
          source_url: "https://x.com/creator/status/123", identity_kind: "native_post_author_id", evidence_source: "x_tweet_detail" },
          text: "<script>untrusted post</script>", text_truncated: true, text_source: "note_tweet",
          created_at: "public date", has_media: true, quotes_another_post: false, expanded_urls: ["https://example.com/tool"] },
      }} /></I18nextProvider>);
      expect(markup).toContain("987654321");
      expect(markup).toContain(catalog.opencliPilot.xNativeContentLimit);
      expect(markup).toContain(catalog.opencliPilot.xContentTruncated);
      expect(markup).toContain("&lt;script&gt;untrusted post&lt;/script&gt;");
      expect(markup).not.toContain('<script>');
      expect(markup).not.toContain('href="https://example.com/tool"'); // Unverified links are text only.
      expect(markup).toContain('href="https://x.com/creator/status/123"');
    });
    it(`hides the old identity evidence section without auto-reading in ${locale}`, async () => {
      vi.stubGlobal("window", { localStorage: { getItem: () => "identity-campaign" } });
      const client = new QueryClient({ defaultOptions: { queries: { retry: false, staleTime: Infinity } } });
      client.setQueryData(["opencli-campaign", "identity-campaign"], {
        id: "identity-campaign", phase: "active", revision: 1, brief: "AI tools", criteria: {},
        opencli_version: "1.8.8", target_count: 30, max_calls: 3, used_calls: 1,
        scopes: [{ platform: "twitter", profile_ref: "scope", keywords: ["AI tools"] }],
        platform_runs: [{ platform: "twitter", profile_ref: "scope", status: "completed", calls_used: 1, stop_code: null }],
      });
      client.setQueryData(["opencli-campaign-hits", "identity-campaign"], { pages: [[{
        id: "x-hit", platform: "twitter", source_url: "https://x.com/i/status/123", title: "Demo", author_label: "Creator",
        author_id: "creator", author_url: "https://x.com/creator", author_observed_at: 1, search_keyword: "AI tools", captured_at: 1, profile_ref: "scope",
      }]], pageParams: [null] });
      const i18n = i18next.createInstance();
      await i18n.init({ lng: locale, ns: ["shell"], defaultNS: "shell", resources: { [locale]: { shell: catalog } } });
      const markup = renderToStaticMarkup(<I18nextProvider i18n={i18n}><QueryClientProvider client={client}>
        <DiscoveryPilot profileRef="scope" version="1.8.8" capabilities={capabilities} />
      </QueryClientProvider></I18nextProvider>);
      expect(markup).not.toContain(catalog.opencliPilot.savedIdentity);
      expect(markup).not.toContain(catalog.opencliPilot.identityStatus.completed);
      expect(client.getQueryState(["opencli-campaign-identity", "identity-campaign", "x-hit"])).toBeUndefined();
      client.clear();
    });

    it(`asks only for a product and keeps manual configuration collapsed in ${locale}`, async () => {
      vi.stubGlobal("window", { localStorage: { getItem: () => null } });
      const client = new QueryClient();
      const i18n = i18next.createInstance();
      await i18n.init({ lng: locale, ns: ["shell"], defaultNS: "shell", resources: { [locale]: { shell: catalog } }, interpolation: { escapeValue: false } });
      const markup = renderToStaticMarkup(<I18nextProvider i18n={i18n}>
        <QueryClientProvider client={client}><DiscoveryPilot profileRef="scope" version="1.8.8" capabilities={capabilities} /></QueryClientProvider>
      </I18nextProvider>);
      expect(markup).toContain(catalog.opencliPilot.productBrief);
      expect(markup).toContain(catalog.opencliPilot.aiPrepare);
      expect(markup).toContain('aria-expanded="false"');
      expect(markup).not.toContain('<fieldset');
      expect(markup).not.toMatch(new RegExp(`>${catalog.opencliPilot.create}</button>`));
      expect(client.getQueryState(["opencli-codex-draft", undefined, "scope"])).toBeUndefined();
      expect(markup).not.toContain("复制需求");
      expect(markup).not.toContain("Copy request");
      client.clear();
    });

    it(`keeps paginated pending candidates in the backend without displaying a manual queue in ${locale}`, async () => {
      vi.stubGlobal("window", { localStorage: { getItem: () => "campaign-1" } });
      const client = new QueryClient({ defaultOptions: { queries: { retry: false, staleTime: Infinity } } });
      client.setQueryData(["opencli-campaign", "campaign-1"], {
        id: "campaign-1", phase: "active", revision: 1, brief: "AI tools",
        criteria: {language: "中文"}, opencli_version: "1.8.8", target_count: 30, max_calls: 50, used_calls: 5,
        scopes: [{ platform: "youtube", profile_ref: "scope", keywords: ["AI tools"] }],
        platform_runs: [{ platform: "youtube", profile_ref: "scope", status: "completed", calls_used: 1, stop_code: null }],
      });
      const hits = Array.from({ length: 101 }, (_, i) => ({
        id: `hit-${i}`, platform: "youtube", source_url: `https://www.youtube.com/watch?v=video${i}`,
        title: `Video ${i + 1}`, author_label: "Creator", search_keyword: "AI tools", captured_at: 1,
      }));
      client.setQueryData(["opencli-campaign-hits", "campaign-1"], {
        pages: [hits.slice(0, 100), hits.slice(100)], pageParams: [null, "hit-99"],
      });
      const candidates = Array.from({ length: 100 }, (_, i) => ({
        id: `candidate-${i}`, platform: "youtube", platform_account_id: `channel${i}`,
        display_name: `Creator ${i + 1}`, profile_url: `https://www.youtube.com/channel/channel${i}`,
        verdict: "pending", evidence: { source_urls: [hits[i].source_url] },
      }));
      client.setQueryData(["opencli-campaign-candidates", "campaign-1"], {
        pages: [candidates], pageParams: [null],
      });
      client.setQueryData(["opencli-campaign-candidate-counts", "campaign-1"], {
        qualified: 0, pending: 100, rejected: 0,
      });
      const i18n = i18next.createInstance();
      await i18n.init({ lng: locale, ns: ["shell"], defaultNS: "shell", resources: { [locale]: { shell: catalog } }, interpolation: { escapeValue: false } });
      const markup = renderToStaticMarkup(<I18nextProvider i18n={i18n}>
        <QueryClientProvider client={client}><DiscoveryPilot profileRef="scope" version="1.8.8" capabilities={capabilities} /></QueryClientProvider>
      </I18nextProvider>);
      expect(markup).not.toContain("<table");
      expect(markup).not.toContain(catalog.opencliPilot.tableAuthor);
      expect(markup).not.toContain(catalog.opencliPilot.tableUnverified);
      expect(markup).not.toContain(catalog.opencliPilot.tableCandidate);
      expect(markup).toContain(catalog.opencliPilot.resyncLibrary);
      expect(markup).not.toContain("Video 101");
      expect(markup).not.toContain("Creator 100");
      expect(markup).not.toContain(catalog.opencliPilot.moreCandidates);
      expect(markup).not.toContain(catalog.opencliPilot.manualApprove);
      expect(client.getQueryData(["opencli-campaign-candidate-counts", "campaign-1"])).toEqual({ qualified: 0, pending: 100, rejected: 0 });
      expect(markup).not.toMatch(/<details[^>]*\bopen(?:=|\s|>)/);
      expect(markup).not.toContain(catalog.opencliPilot.moreHits);
      client.clear();
    });

    it(`keeps historical non-priority Campaign scopes available in ${locale}`, async () => {
      vi.stubGlobal("window", { localStorage: { getItem: () => "old-campaign" } });
      const client = new QueryClient({ defaultOptions: { queries: { retry: false, staleTime: Infinity } } });
      client.setQueryData(["opencli-campaign", "old-campaign"], {
        id: "old-campaign", phase: "active", revision: 1, brief: "Historical scope",
        criteria: {}, opencli_version: "1.8.8", target_count: 30, max_calls: 50, used_calls: 0,
        scopes: [{ platform: "nowcoder", profile_ref: "scope", keywords: ["AI tools"] }],
        platform_runs: [{ platform: "nowcoder", profile_ref: "scope", status: "unsearched", calls_used: 0, stop_code: null }],
      });
      const i18n = i18next.createInstance();
      await i18n.init({ lng: locale, ns: ["shell"], defaultNS: "shell", resources: { [locale]: { shell: catalog } }, interpolation: { escapeValue: false } });
      const markup = renderToStaticMarkup(<I18nextProvider i18n={i18n}>
        <QueryClientProvider client={client}><DiscoveryPilot profileRef="scope" version="1.8.8" capabilities={capabilities} /></QueryClientProvider>
      </I18nextProvider>);
      expect(markup).toContain("nowcoder: AI tools");
      expect(markup).not.toContain('id="discovery-active-platform"');
      expect(markup).not.toContain(catalog.opencliPilot.platformRunsDetails.replaceAll("'", "&#x27;"));
      expect(markup).not.toContain('<fieldset');
      client.clear();
    });

    it(`keeps platform stops out of the overview while details are collapsed in ${locale}`, async () => {
      vi.stubGlobal("window", { localStorage: { getItem: () => "auth-campaign" } });
      const client = new QueryClient({ defaultOptions: { queries: { retry: false, staleTime: Infinity } } });
      client.setQueryData(["opencli-campaign", "auth-campaign"], {
        id: "auth-campaign", phase: "active", revision: 1, brief: "AI creators",
        criteria: {}, opencli_version: "1.8.8", target_count: 20, max_calls: 5, used_calls: 1,
        scopes: [{ platform: "youtube", profile_ref: "scope", keywords: ["AI demo"] }],
        platform_runs: [{ platform: "youtube", profile_ref: "scope", status: "unauthorized", calls_used: 1, stop_code: "opencli_auth_required" }],
      });
      const i18n = i18next.createInstance();
      await i18n.init({ lng: locale, ns: ["shell"], defaultNS: "shell", resources: { [locale]: { shell: catalog } }, interpolation: { escapeValue: false } });
      const markup = renderToStaticMarkup(<I18nextProvider i18n={i18n}>
        <QueryClientProvider client={client}><DiscoveryPilot profileRef="scope" version="1.8.8" capabilities={capabilities} /></QueryClientProvider>
      </I18nextProvider>);
      expect(markup).not.toContain(catalog.opencliPilot.platformRunsDetails.replaceAll("'", "&#x27;"));
      expect(markup).not.toContain(catalog.opencliPilot.accountRequired.replace("{{platform}}", "youtube"));
      expect(markup).not.toContain(catalog.opencliPilot.runStages.unauthorized);
      expect(markup).not.toContain(catalog.opencliPilot.handleIssues);
      expect(markup).toContain(catalog.opencliPilot.attemptedSources);
      expect(markup).toContain("1/1");
      expect(markup).not.toContain("opencli_auth_required");
      client.clear();
    });
  }
});
