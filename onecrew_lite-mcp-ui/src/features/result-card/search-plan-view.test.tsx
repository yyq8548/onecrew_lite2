import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it, vi } from "vitest";
import { ResultCardView } from "@mcp-ui/features/result-card/view";
import { SearchPlanEditFields } from "@mcp-ui/features/result-card/components/search-plan-card";
import { SearchPlanTextDetails } from "@mcp-ui/features/result-card/components/search-plan-details";
import { scopeCriteria, searchScopeKind, searchScopePreviews } from "@mcp-ui/features/result-card/search-plan-presentation";
import { startSearchPlanEdit } from "@mcp-ui/features/result-card/search-plan-state";
import { resultClientFailureCopy } from "@mcp-ui/features/result-card/presentation";
import { TEXT_PAGE_LINES, textPages } from "@mcp-ui/features/result-card/text-pages";
import { PlatformMark } from "@mcp-ui/components/ui/platform-mark";
import { AppLocaleProvider } from "@mcp-ui/lib/i18n";
import { BINDING } from "@mcp-ui/test-support/fixtures";
import { INSTAGRAM_SCOPE, SEARCH_PLAN, X_SCOPE, X_TRANSCRIPT_PLAN, YOUTUBE_SCOPE, YOUTUBE_TRANSCRIPT_PLAN, searchPlanSnapshot } from "@mcp-ui/test-support/search-plan-fixture";

const handlers = { onConfirm: vi.fn(), onCancel: vi.fn(), onRequestEdit: vi.fn(), onSelect: vi.fn(), onReviseSearchPlan: vi.fn(async () => true) };
const THREE_SCOPES = { ...SEARCH_PLAN, scopes: [INSTAGRAM_SCOPE, YOUTUBE_SCOPE, X_SCOPE] };
const OPERATION_IDS = ["x.posts.search.advanced", "youtube.channels.search", "youtube.videos.search", "instagram.users.search", "telegram.communities.search"];
const UUID = /[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}/i;
const visibleText = (html: string) => html.replace(/<[^>]+>/g, " ").replace(/&amp;/g, "&");

// No operation id, connection identifier, schema revision, or raw argument key reaches the card.
function expectBusinessLanguage(html: string) {
  for (const id of OPERATION_IDS) expect(html).not.toContain(id);
  expect(html).not.toMatch(UUID);
  expect(html).not.toContain("sha256:");
  const text = visibleText(html);
  for (const key of ["all_words", "any_words", "exclude_words", "result_mode", "operator_account", "service_credential", "credential_id", "query:", "count:", "[]", "{}", "null"]) expect(text).not.toContain(key);
}

describe("search plan preview", () => {
  it("uses canonical platform glyphs with accessible names", () => {
    for (const [platform, label] of [["x", "X"], ["youtube", "YouTube"], ["instagram", "Instagram"], ["telegram", "Telegram"], ["tiktok", "TikTok"]] as const) {
      const html = renderToStaticMarkup(createElement(PlatformMark, { platform }));
      expect(html).toContain(`aria-label="${label}"`);
      expect(html).toContain("<svg");
      expect(html).toContain("<path");
    }
  });
  it.each(["en-US", "zh-CN"] as const)("explains the explicit plan capacity failure in %s", (locale) => {
    const failure = { kind: "server" as const, failure: { code: "search_plan_payload_too_large" as const } };
    const copy = resultClientFailureCopy(failure, locale);
    const html = renderToStaticMarkup(createElement(ResultCardView, { ...handlers, snapshot: { ...searchPlanSnapshot(), locale }, busy: false, revisionFailure: failure }));
    expect(html).toContain(copy.detail);
    expect(copy.detail).toContain(locale === "zh-CN" ? "计划超过容量限制" : "plan exceeds the size limit");
  });

  it("renders the full pending draft even when the projection contains only a keyword count", () => {
    const snapshot = searchPlanSnapshot();
    snapshot.presentation.sections = [{ kind: "fields", heading: null, fields: [{ label: "Keywords", value: "1 keyword" }] }];
    const html = renderToStaticMarkup(createElement(ResultCardView, { ...handlers, snapshot, busy: false }));
    expect(html).toContain("cooking");
    expect(html).toContain(SEARCH_PLAN.description);
    expect(html).toContain("用户搜索");
  });
  it.each(["en-US", "zh-CN"] as const)("shows bounded preview and distinct plan confirmation in %s", (locale) => {
    const snapshot = searchPlanSnapshot({ ...SEARCH_PLAN, description: "验收标准 ".repeat(1000), keywords: Array.from({ length: 100 }, (_, i) => `Keyword ${i}`) });
    const html = renderToStaticMarkup(createElement(ResultCardView, { ...handlers, snapshot: { ...snapshot, locale }, busy: false }));
    expect(html).toContain(locale === "zh-CN" ? "确认计划" : "Confirm plan");
    expect(html).toContain(locale === "zh-CN" ? "搜索计划" : "Search plan");
    expect(html).toContain(locale === "zh-CN" ? "搜索范围" : "Search scope");
    expect(html).toContain(locale === "zh-CN" ? "达人要求" : "Creator criteria");
    expect(html).toContain(locale === "zh-CN" ? "关键词" : "Keywords");
    expect(html).toContain(locale === "zh-CN" ? "搜索项（1）" : "Searches (1)");
    expect(html).toContain(locale === "zh-CN" ? "查看全部 100 项" : "View all 100");
    expect(html).toContain(locale === "zh-CN" ? "修改搜索范围" : "Modify search scope");
    expect(html.match(/data-slot="badge"/g)).toHaveLength(4);
    expect(html).not.toContain("Keyword 99");
    // The assist panel replaces the preview when it is opened, so the card
    // ships neither its textarea nor the height that unfolding one would add.
    expect(html).toContain(locale === "zh-CN" ? "补充或调整需求" : "Add or adjust requirements");
    expect(html).not.toContain("<textarea");
    expect(html).not.toContain("agent-edit-instruction");
    expect(html).not.toContain('id="search-plan-description"');
    expect(html).not.toContain("确认创建");
  });

  it.each(["en-US", "zh-CN"] as const)("explains the confirmation boundary in the default body in %s", (locale) => {
    const html = renderToStaticMarkup(createElement(ResultCardView, { ...handlers, snapshot: { ...searchPlanSnapshot(), locale }, busy: false }));
    expect(html).toContain(locale === "zh-CN"
      ? "一次确认覆盖此卡列出的只读搜索及其后续分页；授权在本次 onecrew_lite 运行中最长有效 60 分钟，搜索范围变化时需重新确认。"
      : "One confirmation covers the read-only searches listed on this card and their subsequent pages; authorization is valid only for this onecrew_lite run for up to 60 minutes, and a changed search scope requires confirmation again.");
  });

  it("renders the live X transcript scope in business language only", () => {
    const html = renderToStaticMarkup(createElement(ResultCardView, { ...handlers, snapshot: searchPlanSnapshot(X_TRANSCRIPT_PLAN), busy: false }));
    expectBusinessLanguage(html);
    // Icon-only platform glyph, then the localized search kind and one line of the main criteria.
    expect(html).toContain('role="img" aria-label="X"');
    expect(html).not.toContain('<span class="min-w-0 text-sm [overflow-wrap:anywhere]">X</span>');
    expect(html).toContain("帖子搜索（按作者发现）");
    expect(html).toContain('<span class="line-clamp-1 basis-full text-muted-foreground">包含全部词：crypto · 包含任一词：RWA、tokenization、real-world assets · 语言：土耳其语 · 起始日期：2026年8月1日</span>');
    expect(html).toContain(">筛选条件</button>");
    expect(html).not.toContain("账号: ");
    expect(html).not.toContain("操作");
  });

  it("humanizes the transcript filters, skips empty values, and pages them within the height rule", () => {
    const [scope] = searchScopePreviews(X_TRANSCRIPT_PLAN, "zh-CN");
    expect(scope?.criteria.map(({ label, value }) => `${label}：${value}`)).toEqual([
      "包含全部词：crypto", "包含任一词：RWA、tokenization、real-world assets", "排除词：giveaway、airdrop", "语言：土耳其语",
      "起始日期：2026年8月1日", "回复帖：排除", "链接：不限", "排序：最新", "每页结果数：100",
    ]);
    expect(searchScopePreviews(X_TRANSCRIPT_PLAN, "en-US")[0]?.details.split("\n")).toEqual([
      "All of these words: crypto", "Any of these words: RWA, tokenization, real-world assets", "None of these words: giveaway, airdrop", "Language: Turkish",
      "From date: Aug 1, 2026", "Reply posts: Exclude", "Links: Any", "Sort: Latest", "Results per page: 100",
    ]);
    // Six leading-6 lines per page (144px) keep the details under 400px / 50vh.
    const pages = textPages(scope!.details);
    expect(pages).toHaveLength(2);
    for (const page of pages) expect(page.match(/\n/g)?.length ?? 0).toBeLessThanOrEqual(TEXT_PAGE_LINES);
    const html = renderToStaticMarkup(createElement(AppLocaleProvider, { locale: "zh-CN", children: createElement(SearchPlanTextDetails, { title: `筛选条件 · ${scope!.kind}`, text: scope!.details, page: 0, onPage: vi.fn(), onClose: vi.fn() }) }));
    expectBusinessLanguage(html);
    expect(html).toContain("筛选条件 · 帖子搜索（按作者发现）");
    expect(html).toContain("语言：土耳其语");
    expect(html).not.toContain("排序：最新");
    expect(html).toContain("第 1 / 2 页");
    expect(html).toContain(">下一页</button>");
  });

  it("labels unknown operations and keys generically and hides execution controls and identifiers", () => {
    expect(searchScopeKind("zh-CN", "future.platform.search")).toBe("搜索");
    expect(searchScopeKind("en-US", "future.platform.search")).toBe("Search");
    expect(scopeCriteria("en-US", {
      empty_text: " ", none: null, empty_list: [], empty_object: {}, search_plan_policy: "require_confirmation", confirmed_plan_ref: null, cursor: null,
      request_id: "00000000-0000-4000-8000-000000000002",
      request: { connection_id: "connection-demo", account_id: "account-demo", country_codes: ["US"], min_followers: 1000, max_followers: 50000, sort_field: "FOLLOWERS", sort_order: "DESC", follower_gender_ratio: "FEMALE_60", content_label_ids: ["101", "102"], page: 1, page_size: 20 },
      custom_filter: "x", nested_extra: { inner_key: 3 },
    }).map(({ label, value }) => `${label}: ${value}`)).toEqual([
      "Creator regions: United States", "Followers: ≥ 1,000", "Followers: ≤ 50,000", "Content labels: 2 selected", "Follower gender: Female ≥ 60%",
      "Sort by: Followers", "Sort order: Descending", "Results per page: 20", "custom filter: x", "nested extra · inner key: 3",
    ]);
  });

  it.each(["en-US", "zh-CN"] as const)("lists each bound scope read-only by platform glyph and search kind in %s", (locale) => {
    const html = renderToStaticMarkup(createElement(ResultCardView, { ...handlers, snapshot: { ...searchPlanSnapshot(YOUTUBE_TRANSCRIPT_PLAN), locale }, busy: false }));
    expectBusinessLanguage(html);
    expect(html.match(/role="img" aria-label="YouTube"/g)).toHaveLength(2);
    expect(html).toContain(locale === "zh-CN" ? "频道搜索" : "Channel search");
    expect(html).toContain(locale === "zh-CN" ? "视频搜索" : "Video search");
    expect(html.match(locale === "zh-CN" ? /搜索词：Turkey Türkiye Turkish Türkçe RWA crypto real world assets tokenization/g : /Search terms: Turkey Türkiye Turkish Türkçe RWA crypto real world assets tokenization/g)).toHaveLength(2);
    expect(html.match(locale === "zh-CN" ? />筛选条件<\/button>/g : />Filters<\/button>/g)).toHaveLength(2);
    expect(html).toContain(locale === "zh-CN" ? "搜索项（2）" : "Searches (2)");
    expect(html).not.toContain("<input");
    expect(html).not.toContain("<select");
  });

  it("collapses more than three scopes behind a paged scope entry", () => {
    const snapshot = searchPlanSnapshot({ ...THREE_SCOPES, scopes: [...THREE_SCOPES.scopes, { ...YOUTUBE_SCOPE, operation_id: "youtube.videos.search" }] });
    const html = renderToStaticMarkup(createElement(ResultCardView, { ...handlers, snapshot: { ...snapshot, locale: "en-US" }, busy: false }));
    expect(html).toContain(">Searches (4)</button>");
    expect(html).not.toContain("Video search");
  });

  it.each(["en-US", "zh-CN"] as const)("edits only description and keywords while the bound scopes stay read-only in %s", (locale) => {
    const draft = { ...SEARCH_PLAN, scopes: [INSTAGRAM_SCOPE, YOUTUBE_SCOPE] };
    const edit = startSearchPlanEdit(draft, BINDING, "digest");
    const html = renderToStaticMarkup(createElement(AppLocaleProvider, { locale, children: createElement(SearchPlanEditFields, { edit, draft, localError: null, pendingKeyword: false, busy: false, onChange: vi.fn(), onPendingKeyword: vi.fn() }) }));
    expect(html).toContain('id="search-plan-description"');
    expect(html.match(/<textarea/g)).toHaveLength(1);
    expect(html).toContain(locale === "zh-CN" ? "用户搜索" : "User search");
    expect(html).toContain(locale === "zh-CN" ? "频道搜索" : "Channel search");
    expect(html).not.toContain("instagram.users.search");
    expect(html).not.toContain("operator-demo");
    expect(html).toContain(locale === "zh-CN" ? "已绑定的搜索操作不能在此修改" : "Bound search operations cannot be edited here");
    expect(html).not.toContain(locale === "zh-CN"
      ? "一次确认覆盖此卡列出的只读搜索及其后续分页"
      : "One confirmation covers the read-only searches listed on this card");
    expect(html).not.toContain("<select");
    expect(html).not.toContain('inputmode="numeric"');
    expect(html).not.toContain(locale === "zh-CN" ? "最低粉丝数" : "Minimum followers");
  });

  it("renders local description and keyword errors from the validate_typed rules", () => {
    const edit = startSearchPlanEdit(SEARCH_PLAN, BINDING, "digest");
    const render = (fields: { description: string; keywords: string[] }, localError: "description" | "keywords") => renderToStaticMarkup(createElement(AppLocaleProvider, { locale: "en-US", children: createElement(SearchPlanEditFields, { edit: { ...edit, fields }, draft: SEARCH_PLAN, localError, pendingKeyword: false, busy: false, onChange: vi.fn(), onPendingKeyword: vi.fn() }) }));
    expect(render({ description: " ", keywords: [] }, "description")).toContain("Enter valid creator requirements");
    expect(render({ description: "criteria", keywords: ["cooking", "cooking"] }, "keywords")).toContain("Check for empty or duplicate keywords");
    expect(render({ description: "criteria", keywords: Array.from({ length: 51 }, (_, i) => `k${i}`) }, "keywords")).toContain("Use at most 50 keywords.");
  });

  it("renders confirmed/cancelled results without search or edit authority", () => {
    for (const code of ["search_plan_confirmed", "user_cancelled"]) {
      const snapshot = { ...searchPlanSnapshot(), pending_action: null, state: { kind: "resolved" as const, code, summary: "Closed" } };
      const html = renderToStaticMarkup(createElement(ResultCardView, { ...handlers, snapshot, busy: false }));
      expect(html).not.toContain(">确认计划</button>");
      expect(html).not.toContain(">修改搜索范围</button>");
      if (code === "search_plan_confirmed") expect(html).toContain("等待开始搜索");
    }
  });

  it("keeps the model-facing confirmation summary and plan reference off the confirmed card", () => {
    const summary = "Search plan confirmed: plan-ref-demo-123. Use this reference for the approved searches and evaluate results against the final acceptance criteria.";
    const base = searchPlanSnapshot(X_TRANSCRIPT_PLAN);
    const snapshot = { ...base, pending_action: null, state: { kind: "resolved" as const, code: "search_plan_confirmed", summary }, presentation: { ...base.presentation, summary } };
    const html = renderToStaticMarkup(createElement(ResultCardView, { ...handlers, snapshot, busy: false }));
    expectBusinessLanguage(html);
    expect(html).toContain("计划已确认");
    expect(html).toContain("等待开始搜索。");
    // The terminal card echoes the server projection: localized labels and the acceptance criteria only.
    expect(html).toContain("验收标准");
    expect(html).toContain(X_TRANSCRIPT_PLAN.description.slice(0, 40));
    expect(visibleText(html)).not.toMatch(/keywords|editable list|Search plan|Acceptance criteria/i);
    expect(html).not.toContain("plan-ref-demo-123");
    expect(html).not.toContain("Search plan confirmed");
  });
});
