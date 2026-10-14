import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it, vi } from "vitest";

import { ResultCardView } from "@mcp-ui/features/result-card/view";
import { creatorBatchSnapshot } from "@mcp-ui/test-support/creator-batch-fixture";
import { manualEmailSnapshot } from "@mcp-ui/test-support/email-fixture";

const THEME_CSS = readFileSync(fileURLToPath(new URL("../../styles/theme.css", import.meta.url)), "utf8");
const SHELL_SOURCE = readFileSync(fileURLToPath(new URL("./components/shell.tsx", import.meta.url)), "utf8");
const CARD_SOURCES = import.meta.glob([
  "/src/features/result-card/components/*.tsx",
  "/src/features/result-card/view.tsx",
], { query: "?raw", import: "default", eager: true }) as Record<string, string>;

const handlers = { onConfirm: vi.fn(), onSelect: vi.fn(), onCancel: vi.fn(), onRequestEdit: vi.fn() };
const render = (snapshot: Parameters<typeof ResultCardView>[0]["snapshot"]) =>
  renderToStaticMarkup(createElement(ResultCardView, { snapshot, busy: false, ...handlers }));

/** An inline card fits its content. It does not scroll inside itself and it
 * does not page: both hid the very thing the card existed to show — a scroll
 * region put a prepared message above the fold, and a pager turned one short
 * list into "page 1 of 6" that the reader had to scroll to operate. Nor is a
 * scroll region a way to survive being too tall: a card with more to show asks
 * for fullscreen, the one mode where it owns a scrollport. Fitting the frame is
 * a design obligation and not a licence to clip, so what the Host's frame does
 * not show stays reachable by scrolling the document itself. */
describe("a card that fits its content", () => {
  it("scopes the card's only scrollport to fullscreen and keeps inline free of one", () => {
    const scrolling = [...THEME_CSS.matchAll(/([^{}]*\.result-shell-content)\s*\{[^}]*overflow-y:\s*auto/g)];
    for (const [, selector] of scrolling) expect(selector, selector).toContain('[data-display-mode="fullscreen"]');
    expect(SHELL_SOURCE).not.toContain("maxHeight");
    expect(SHELL_SOURCE).not.toContain("useCardCeiling");
    // A Host that stated no display mode gets the inline card, never a
    // fullscreen layout this App assumed on its behalf.
    const html = render(manualEmailSnapshot());
    expect(html).not.toContain("max-height");
    expect(html).not.toContain("data-display-mode");
  });

  it("keeps what the frame cannot show reachable by scrolling instead of clipped away", () => {
    // This Host keeps its own frame height whatever height the App reports, so
    // the viewport has to stay scrollable or everything past the frame's edge
    // is unreachable. `overflow` on the root propagates to the viewport, so the
    // root rule must not suppress it; vertical scroll lives on `body`.
    const rootRule = THEME_CSS.match(/html,\s*body,\s*#root\s*\{([^}]*)\}/)?.[1] ?? "";
    expect(rootRule).toContain("scrollbar-width: none");
    expect(rootRule).not.toMatch(/overflow(-y)?:\s*(hidden|clip)/);
    const bodyRule = THEME_CSS.match(/^body\s*\{([^}]*)\}/m)?.[1] ?? "";
    expect(bodyRule).toMatch(/overflow-y:\s*auto/);
    expect(bodyRule).toMatch(/overflow-x:\s*hidden/);
    // Reachable, yet without a page-level bar drawn beside the Host's own.
    expect(THEME_CSS).toMatch(/body::-webkit-scrollbar[^{]*\{\s*display:\s*none/);
    // A viewport-height root under Host auto-resize grows without bound, so
    // 100vh belongs to the granted fullscreen mode and to no other.
    const viewportHeights = [...THEME_CSS.matchAll(/([^{}]*)\{[^}]*100vh/g)];
    expect(viewportHeights.length).toBeGreaterThan(0);
    for (const [, selector] of viewportHeights) expect(selector, selector).toContain('[data-display-mode="fullscreen"]');
  });

  it("keeps the paging control and action bar structurally separate", () => {
    // Static rendering has no Host geometry, so page count is covered by the
    // real-browser lane. The action bar remains independent of that reading
    // bound and can act on the full selection from any page.
    const html = renderToStaticMarkup(createElement(ResultCardView, {
      snapshot: creatorBatchSnapshot(40), busy: false, ...handlers, onConfirmCreatorSaveBatch: vi.fn(),
    }));
    expect(html.match(/data-result-actions="true"/g)).toHaveLength(1);
    expect(html).toContain("保存或更新所选达人");
    // The search plan keeps its own editor pagers; no result view may bring
    // back the removed paging infrastructure to page its content.
    for (const [path, source] of Object.entries(CARD_SOURCES)) {
      if (path.includes("search-plan")) continue;
      expect(source, path).not.toContain("usePagedRegion");
      expect(source, path).not.toContain("listPage");
    }
  });

  it("keeps a single action bar and chooses its layout from its own width", () => {
    const html = render(manualEmailSnapshot());
    expect(html.match(/data-result-actions="true"/g)).toHaveLength(1);
    expect(SHELL_SOURCE).toContain("useCompactWidth()");
    // A media query inside an iframe reports the outer view, so it would stay
    // wide while this card is rendered into a narrow column.
    expect(SHELL_SOURCE).not.toContain("matchMedia");
  });

  it("lets the creator table pan sideways without creating a second inline vertical scrollport", () => {
    expect(THEME_CSS).toMatch(/\[data-creator-list-scroll="true"\]\s*\{[^}]*overflow-x:\s*auto[^}]*overflow-y:\s*clip/);
    const source = CARD_SOURCES["/src/features/result-card/components/creator-review-list.tsx"]!;
    expect(source).not.toContain("overflow-y-auto");
  });
});
