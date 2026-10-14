import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it, vi } from "vitest";

import { CardDetailFrame, CardDetailView, ResultTextDetailsProvider } from "@mcp-ui/features/result-card/components/text-details";
import { HostFrameProvider } from "@mcp-ui/app/host-frame";
import { ResultActions } from "@mcp-ui/features/result-card/components/result-actions";
import { ResultShell } from "@mcp-ui/features/result-card/components/shell";
import { EmailDetails } from "@mcp-ui/features/result-card/components/email-preview";
import { CreatorReviewList, CreatorRowDetail } from "@mcp-ui/features/result-card/components/creator-review-list";
import { ResultCardView } from "@mcp-ui/features/result-card/view";
import { emailPreview } from "@mcp-ui/features/result-card/email-preview";
import { selectableCreatorRoutes } from "@mcp-ui/features/result-card/creator-save-state";
import { displayLabel } from "@mcp-ui/lib/display-label";
import { AppLocaleProvider, translate } from "@mcp-ui/lib/i18n";
import { manualEmailSnapshot } from "@mcp-ui/test-support/email-fixture";
import { searchPlanSnapshot } from "@mcp-ui/test-support/search-plan-fixture";
import { CREATOR_SAVE_PREVIEW } from "@mcp-ui/test-support/creator-save-fixture";
import { SNAPSHOT } from "@mcp-ui/test-support/fixtures";

const SOURCES = import.meta.glob([
  "/src/features/result-card/components/*.tsx",
  "/src/features/result-card/view.tsx",
], { query: "?raw", import: "default", eager: true }) as Record<string, string>;

const handlers = { onConfirm: vi.fn(), onSelect: vi.fn(), onCancel: vi.fn(), onRequestEdit: vi.fn() };
const render = (snapshot: Parameters<typeof ResultCardView>[0]["snapshot"]) =>
  renderToStaticMarkup(createElement(ResultCardView, { snapshot, busy: false, ...handlers }));
const body = () => createElement("p", null, "BODY-CONTENT");
const frame = (detail: Parameters<typeof CardDetailFrame>[0]["detail"]) =>
  renderToStaticMarkup(createElement(CardDetailFrame, { detail, onPage: vi.fn(), onClose: vi.fn(), children: body() }));

/** Growing is the one move this Host punishes. It ignores
 * `ui/notifications/size-changed` and keeps the frame height it already chose,
 * so every disclosure that adds height pushes the content it revealed — and the
 * action bar beneath it — outside the frame. A disclosure therefore replaces the
 * body, and it bounds reading only: the panel carries its own way back, while
 * the card's one action bar keeps the card's own actions, so a swap never
 * stands between the reader and the decision. */
describe("a disclosure that replaces the body instead of growing it", () => {
  it("shows the detail instead of the body, never alongside it, and offers the way back", () => {
    const closed = frame(null);
    expect(closed).toContain("BODY-CONTENT");
    expect(closed).not.toContain("hidden=");

    const open = frame({ kind: "view", title: "DETAIL-TITLE", content: createElement("p", null, "DETAIL-CONTENT") });
    expect(open).toContain("DETAIL-CONTENT");
    expect(open).toContain("DETAIL-TITLE");
    // The body it replaced is kept mounted so a draft in it survives the trip,
    // but it is never rendered visibly beside the detail.
    expect(open).toContain('<div hidden=""><p>BODY-CONTENT</p></div>');
    expect(open.indexOf("DETAIL-CONTENT")).toBeLessThan(open.indexOf("BODY-CONTENT"));
    // Reading is not a mode with actions of its own, so the way back belongs to
    // the panel and the card's action bar is left to the card's decision.
    expect(open).toContain(translate("en-US", "back"));
    expect(open).toContain("data-card-detail-back");
  });

  it("keeps the card's own decision in the bar while a detail is read", () => {
    const pending = SNAPSHOT.pending_action;
    const confirmLabel = pending?.kind === "confirm" ? pending.label : "";
    expect(confirmLabel).not.toBe("");
    // A static render cannot click a card into its detail state, so the detail
    // is composed into the body the way a body region composes one — the shape
    // CreatorRowDetail and the plan's panels already use, and the shape the
    // shared frame now produces for every other detail.
    const html = renderToStaticMarkup(createElement(AppLocaleProvider, { locale: "zh-CN", children: createElement(ResultShell, {
      snapshot: SNAPSHOT,
      footer: createElement(ResultActions, {
        cancel: { label: translate("zh-CN", "cancel"), onClick: vi.fn() },
        primary: { label: confirmLabel, onClick: vi.fn() },
      }),
      children: createElement(CardDetailView, { title: "DETAIL-TITLE", onClose: vi.fn(), children: createElement("p", null, "DETAIL-CONTENT") }),
    }) }));
    expect(html.match(/data-result-actions="true"/g)).toHaveLength(1);
    const split = html.indexOf("data-result-actions");
    const [cardBody, bar] = [html.slice(0, split), html.slice(split)];
    // Reading bounds reading. The decision is still in front of the reader,
    // acting on the whole selection rather than on what is displayed.
    expect(bar).toContain(confirmLabel);
    expect(bar).toContain(translate("zh-CN", "cancel"));
    expect(bar).not.toContain(translate("zh-CN", "back"));
    // The way back is in the panel, where a reader leaves reading behind.
    expect(cardBody).toContain("data-card-detail-back");
    expect(cardBody).toContain(translate("zh-CN", "back"));
    // The bar cannot depend on what the body is showing: the shell never reads
    // the detail state, so no detail can take the card's actions off it.
    const shell = SOURCES["/src/features/result-card/components/shell.tsx"]!;
    expect(shell).not.toContain("useCardDetails");
    expect(shell).not.toContain("t(\"back\")");
  });

  it("keeps the paged reading of long text on the same frame", () => {
    const open = frame({ kind: "text", title: "LONG", text: "x".repeat(400), page: 0 });
    expect(open).toContain(translate("en-US", "page", { current: 1, total: 3 }));
    expect(open).toContain(translate("en-US", "nextPage"));
    expect(open).toContain('<div hidden=""><p>BODY-CONTENT</p></div>');
  });

  it("bounds the detail from the room the Host stated, and falls back to a named constant", () => {
    const source = SOURCES["/src/features/result-card/components/text-details.tsx"]!;
    // The bound is the Host's stated frame minus what the card already spends,
    // never a hand-picked pixel height.
    expect(source).toContain("useHostFrame");
    expect(source).toContain("availableHeight");
    expect(source).toContain("[data-result-actions]");
    expect(source).toContain("UNSTATED_DETAIL_HEIGHT");
    expect(source).toContain("conservative constant");
    // Inline owns no nested vertical scrollport and therefore does not invent a
    // max-height. The document remains the scroll owner until fullscreen is
    // actually granted.
    const html = renderToStaticMarkup(createElement(CardDetailView, { title: "T", onClose: vi.fn(), children: createElement("p", null, "C") }));
    expect(html).not.toContain("max-height");
    const fullscreen = renderToStaticMarkup(createElement(HostFrameProvider, {
      frame: { displayMode: "fullscreen", availableDisplayModes: ["inline", "fullscreen"], platform: "desktop", touch: false, safeArea: null, availableHeight: null },
      children: createElement(CardDetailView, { title: "T", onClose: vi.fn(), children: createElement("p", null, "C") }),
    }));
    expect(fullscreen).toContain("max-height:320px");
  });

  it("sends the prepared message's internals to the detail view and keeps the decision reachable", () => {
    const html = render(manualEmailSnapshot());
    expect(html).not.toContain("mail-details");
    expect(html.match(/data-result-actions="true"/g)).toHaveLength(1);

    const model = emailPreview(manualEmailSnapshot().presentation)!;
    const details = renderToStaticMarkup(createElement(AppLocaleProvider, { locale: "zh-CN", children: createElement(EmailDetails, { model }) }));
    expect(details).toContain("mail-details");
    // Values reach the view as text, so an address in angle brackets arrives
    // escaped rather than as markup.
    const escaped = (value: string) => value.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
    for (const detail of model.details) expect(details).toContain(escaped(detail.value));
  });

  it("opens the plan's assist panel as a swap, shipping none of its height while closed", () => {
    const html = render({ ...searchPlanSnapshot(), locale: "zh-CN" });
    expect(html).toContain(translate("zh-CN", "planAssistLabel"));
    expect(html).not.toContain("<details");
    expect(html).not.toContain("<textarea");
    expect(html).not.toContain("agent-edit-instruction");
    expect(html.match(/data-result-actions="true"/g)).toHaveLength(1);
  });

  it("reads a long row value in the detail view rather than expanding it under the row", () => {
    const snapshot = structuredClone(SNAPSHOT);
    snapshot.presentation.sections = [{ kind: "fields", heading: null, fields: [{ label: "Notes", value: "N".repeat(400) }] }];
    const html = render(snapshot);
    const control = html.match(/<button[^>]*>查看全文<\/button>/)?.[0];
    expect(control).toBeDefined();
    // A way to a view that replaces the body reports no expanded state, and the
    // in-place disclosure's collapse control is gone with it.
    expect(control).not.toContain("aria-expanded");
    expect(control).not.toContain("aria-controls");
    expect(html).not.toContain(translate("zh-CN", "collapseText"));
    expect(html.match(/data-result-actions="true"/g)).toHaveLength(1);
  });

  it("shows a creator row's routes and remaining followers instead of the list, not inside it", () => {
    const preview = structuredClone(CREATOR_SAVE_PREVIEW);
    preview.follower_counts = { ...preview.follower_counts, [preview.platform]: 1_000, youtube: 4_321 };
    const rows = [{ row_id: "row-0", preview }];
    const list = renderToStaticMarkup(createElement(AppLocaleProvider, { locale: "en-US", children: createElement(CreatorReviewList, {
      rows, selectedIds: ["row-0"], optionalIds: {}, disabled: false, onSelect() {}, onRoute() {},
    }) }));
    // Nothing of the row's detail is mounted beside the list.
    expect(list).toContain('data-creator-row="row-0"');
    expect(list).not.toContain("creator-supplement");

    const detail = renderToStaticMarkup(createElement(AppLocaleProvider, { locale: "en-US", children: createElement(ResultTextDetailsProvider, { revision: 1, children: createElement(CreatorRowDetail, {
      preview, selected: true, optional: [], disabled: false, onRoute() {}, onClose() {},
    }) }) }));
    expect(detail).toContain("creator-supplement");
    expect(detail).not.toContain("data-creator-row=");
    for (const route of selectableCreatorRoutes(preview)) expect(detail).toContain(route.display_value);
    expect(detail).toContain(displayLabel("youtube", "en-US"));
    expect(detail).toContain("4,321");
    // The list opens this one out of its own state, so that the routes it
    // toggles stay live. It carries its own way back, the same as every detail
    // the card's shared frame opens.
    expect(detail).toContain(translate("en-US", "back"));
  });

  it("makes the Agent edit panel the body's alternative while the wizard draft survives behind it", () => {
    const view = SOURCES["/src/features/result-card/view.tsx"]!;
    expect(view).toMatch(/\{editing && canAct \? \(/);
    expect(view).not.toContain("{editing && canAct && (");
    // The wizard stays mounted but hidden, exactly as before, so a trip through
    // the edit panel does not discard the answers already given.
    expect(view).toContain("<div hidden={editing}>");
  });

  it("leaves no in-place disclosure behind on the swapped surfaces, and leaves Choice alone", () => {
    for (const name of ["email-preview", "search-plan-card", "creator-review-list", "projection-sections"]) {
      const source = SOURCES[`/src/features/result-card/components/${name}.tsx`]!;
      expect(source, name).not.toContain("<details");
      expect(source, name).not.toContain("<summary");
    }
    // Choice is out of scope and keeps the shared in-place expander it had.
    expect(SOURCES["/src/features/result-card/components/choice-action.tsx"]!).toContain("SharedExpandableText");
  });
});
