import { readFileSync } from "node:fs";
import { fileURLToPath, URL } from "node:url";
import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { CreatorReviewList, CreatorScore } from "./components/creator-review-list";
import { creatorAssessmentText, creatorScoreTooltipText } from "./components/creator-score";
import { CREATOR_SAVE_PREVIEW } from "@mcp-ui/test-support/creator-save-fixture";
import { AppLocaleProvider, translate } from "@mcp-ui/lib/i18n";
import { pageCapacity, pageForAnchor } from "./creator-review-layout";
import { HostFrameProvider } from "@mcp-ui/app/host-frame";

const LIST_SOURCE = readFileSync(fileURLToPath(new URL("./components/creator-review-list.tsx", import.meta.url)), "utf8");

describe("Creator list review", () => {
  it("budgets one safe-area deduction and keeps an anchor on a resized page", () => {
    expect(pageCapacity({ availableHeight: 600, listTop: 100, actionHeight: 80, rowHeight: 50, pagerHeight: 30, listToPagerGap: 12, contentBottomPadding: 18, safeArea: { top: 10, right: 0, bottom: 20, left: 0 } })).toBe(6);
    expect(pageForAnchor(["r0", "r1", "r2", "r3", "r4", "r5", "r6"], "r4", 3)).toBe(1);
  });

  it("offers the fullscreen escape even when one page fits inline", () => {
    const rows = [{ row_id: "row-0", preview: structuredClone(CREATOR_SAVE_PREVIEW) }];
    const html = renderToStaticMarkup(createElement(AppLocaleProvider, { locale: "en-US", children: createElement(HostFrameProvider, {
      frame: { displayMode: "inline", availableDisplayModes: ["inline", "fullscreen"], platform: "desktop", touch: false, safeArea: null, availableHeight: 720 },
      onRequestDisplayMode() {},
      children: createElement(CreatorReviewList, { rows, selectedIds: [], optionalIds: {}, onSelect() {}, onRoute() {}, disabled: false }),
    }) }));
    expect(html).toContain(translate("en-US", "listReviewFullscreen", { total: 1 }));
  });

  it("disables row selection for every owner-ineligible status", () => {
    const preview = { ...structuredClone(CREATOR_SAVE_PREVIEW), status: "existing_skipped" as const, reason: "existing_creator" as const };
    const html = renderToStaticMarkup(createElement(AppLocaleProvider, { locale: "en-US", children: createElement(CreatorReviewList, {
      rows: [{ row_id: "row-0", preview }], selectedIds: [], optionalIds: {}, onSelect() {}, onRoute() {}, disabled: false,
    }) }));
    expect(html.match(/role="checkbox"[^>]*disabled/g)).toHaveLength(1);
  });

  it("renders the owner's status and safe reason in the selected locale", () => {
    const rows = ([
      ["eligible_new", null],
      ["eligible_update", null],
      ["existing_skipped", "existing_creator"],
      ["duplicate_skipped", "duplicate_selection"],
      ["identity_pending", "identity_ambiguous"],
      ["contact_conflict", "required_contact_route_conflict"],
      ["failed", "preparation_failed"],
    ] as const).map(([status, reason], index) => ({
      row_id: `row-${index}`,
      preview: { ...structuredClone(CREATOR_SAVE_PREVIEW), display_name: `Creator ${index}`, status, reason },
    }));
    const html = renderToStaticMarkup(createElement(AppLocaleProvider, { locale: "zh-CN", children: createElement(CreatorReviewList, {
      rows, selectedIds: [], optionalIds: {}, onSelect() {}, onRoute() {}, disabled: false,
    }) }));
    expect(html).toContain(translate("zh-CN", "creatorStatusEligibleUpdate"));
    expect(html).toContain(translate("zh-CN", "creatorSaveReasonExistingCreator"));
    expect(html).toContain(translate("zh-CN", "creatorSaveReasonPreparationFailed"));
    // No local identity/collision inference should leak a different warning.
    expect(html).not.toContain(translate("zh-CN", "creatorIdentityDecision"));
  });
  // Static rendering has no DOM geometry and is deliberately not evidence for
  // page capacity. The browser lane supplies the Host budget and real boxes;
  // this seam only verifies that an unstated frame does not invent a page size
  // or disable the action-independent row controls.
  it("does not invent a page size when static rendering has no Host geometry", () => {
    const rows = Array.from({ length: 20 }, (_, index) => ({ row_id: `row-${index}`, preview: { ...structuredClone(CREATOR_SAVE_PREVIEW), display_name: `Creator ${index}` } }));
    const html = renderToStaticMarkup(createElement(AppLocaleProvider, { locale: "en-US", children: createElement(CreatorReviewList, {
      rows, selectedIds: rows.map(row => row.row_id), optionalIds: {}, disabled: true,
      onSelect() {}, onRoute() {},
    }) }));
    const buttons = [...html.matchAll(/<button\b[^>]*>[\s\S]*?<\/button>/g)].map(match => match[0]);
    expect(html.match(/data-creator-row=/g)).toHaveLength(20);
    expect(html).not.toContain('data-creator-list-pages="true"');
    // The Host offered no fullscreen here, so the control that would ask for it
    // is absent rather than present and inert.
    expect(html).not.toContain(translate("en-US", "listReviewFullscreen", { total: 20 }));
    const checkboxes = buttons.filter(button => button.includes('role="checkbox"'));
    expect(checkboxes).toHaveLength(20);
    expect(checkboxes.every(button => button.includes("disabled="))).toBe(true);
  });
  it("renders five creators in one Host-sized list with individual checkboxes", () => {
    const rows = Array.from({ length: 5 }, (_, index) => ({ row_id: `row-${index}`, preview: { ...structuredClone(CREATOR_SAVE_PREVIEW), display_name: `Creator ${index}` } }));
    const html = renderToStaticMarkup(createElement(CreatorReviewList, { rows, selectedIds: rows.map(row => row.row_id), optionalIds: {}, onSelect() {}, onRoute() {}, disabled: false }));
    expect(html.match(/data-creator-row=/g)).toHaveLength(5);
    expect(html.match(/role="checkbox"/g)).toHaveLength(5);
    expect(html).not.toContain('max-height:');
    expect(html).not.toContain('overflow-y-auto');
    expect(html).toContain('data-creator-list-scroll="true"');
    expect(html).not.toContain("candidate-0");
    expect(html).not.toContain("<img");
  });
  it("keeps full assessment text and uses explicit fallback for null or blank descriptions", () => {
    for (const value of [null, undefined, "", "  \n "]) expect(creatorAssessmentText(value, "暂无评分描述")).toBe("暂无评分描述");
    const full = "  核对内容主题。\n保留公开渠道来源。  ";
    expect(creatorAssessmentText(full, "暂无评分描述")).toBe(full);
  });
  it("renders each row's score as a plain focusable grade whose hover text is that Creator's assessment", () => {
    const rows = ([[87, "符合土耳其语与 RWA 要求；合作邮箱待核实。"], [0, "未满足语言要求。"], [null, null]] as const).map(([score, assessment], index) => ({
      row_id: `row-${index}`, preview: { ...structuredClone(CREATOR_SAVE_PREVIEW), display_name: `Creator ${index}`, score, assessment },
    }));
    const html = renderToStaticMarkup(createElement(AppLocaleProvider, { locale: "zh-CN", children: createElement(CreatorReviewList, { rows, selectedIds: [], optionalIds: {}, onSelect() {}, onRoute() {}, disabled: false }) }));
    const buttons = (label: string) => html.match(new RegExp(`<button[^>]*aria-label="${label}"[^>]*>([^<]*)</button>`, "g")) ?? [];
    for (const [label, text] of [["评分等级：87 · 良好", "87 · 良好"], ["评分等级：0 · 较低", "0 · 较低"], ["评分等级：未评级", "未评级"]]) {
      const [button] = buttons(label);
      expect(button, label).toBeDefined();
      expect(button).toContain(`>${text}</button>`);
      expect(button).not.toContain("underline decoration-dotted");
    }
    // The header keeps the scale explanation on its dotted label.
    expect(html).toContain(`aria-label="评分等级" aria-haspopup="dialog">评分等级</button>`);
    const t = (key: "creatorNoAssessment" | "scoreScaleHelp") => translate("zh-CN", key);
    expect(creatorScoreTooltipText(true, rows[0]!.preview.assessment, t)).toBe("符合土耳其语与 RWA 要求；合作邮箱待核实。");
    expect(creatorScoreTooltipText(true, rows[2]!.preview.assessment, t)).toBe("暂无评分描述");
    expect(creatorScoreTooltipText(true, "  ", t)).toBe("暂无评分描述");
    expect(creatorScoreTooltipText(false, rows[0]!.preview.assessment, t)).toBe(translate("zh-CN", "scoreScaleHelp"));
  });
  it("preserves zero score and distinguishes missing score", () => {
    expect(renderToStaticMarkup(createElement(CreatorScore, { score: 0 }))).toContain(">0 · Low<");
    expect(renderToStaticMarkup(createElement(CreatorScore, { score: null }))).toContain(">Unrated<");
  });

  it("keeps detail return focus scoped to the mounted card opener", () => {
    expect(LIST_SOURCE).not.toContain("document.querySelectorAll");
    expect(LIST_SOURCE).toContain("detailOpener");
  });

  it("leaves enough measured horizontal room for localized grade labels", () => {
    expect(LIST_SOURCE).toContain("grid-cols-[20px_minmax(0,1fr)_88px_76px_76px_16px]");
  });

  it("does not turn userinfo profile URLs into candidate or failure anchors", () => {
    const candidate = {
      row_id: "candidate-userinfo",
      preview: { ...structuredClone(CREATOR_SAVE_PREVIEW), profile_url: "https://user:pass@example.test/candidate", assessment: null },
    };
    const failure = {
      row_id: "failure-userinfo",
      display_name: "Blocked candidate",
      platform: "x" as const,
      handle: "blocked",
      profile_url: "http://user:pass@example.test/failure",
      reason: "preparation_failed" as const,
    };
    const html = renderToStaticMarkup(createElement(AppLocaleProvider, { locale: "en-US", children: createElement(CreatorReviewList, {
      rows: [candidate], failures: [failure], selectedIds: [], optionalIds: {}, onSelect() {}, onRoute() {}, disabled: false,
    }) }));
    expect(html).not.toContain('href="https://user:pass@example.test/candidate"');
    expect(html).not.toContain('href="http://user:pass@example.test/failure"');
    expect(html).toContain("Blocked candidate");
  });
});
