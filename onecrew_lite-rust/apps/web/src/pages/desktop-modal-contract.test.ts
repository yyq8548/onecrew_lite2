import { describe, expect, it } from "vitest";

import inboxSource from "./Inbox.tsx?raw";
import creatorSource from "./Leads.tsx?raw";
import toolkitSource from "./accounts/toolkit-catalog.tsx?raw";

describe("Desktop modal composition contract", () => {
  it("keeps Toolkit and Creator detail sheets modal and canvas-scoped", () => {
    for (const content of [toolkitSource, creatorSource]) {
      expect(content).toContain("container={canvas}");
      expect(content).toContain("onCloseAutoFocus=");
      expect(content).toContain("<SheetClose asChild>");
      expect(content).not.toContain("modal={false}");
      expect(content).not.toContain("onInteractOutside=");
    }

    expect(toolkitSource).not.toContain("HoverCard");
    expect(toolkitSource).toContain("w-[min(560px,calc(100%-1rem))]");
    expect(creatorSource).toContain("w-[min(560px,calc(100%-1rem))]");
  });

  it("uses the canvas modal and Command keyboard model for new conversations", () => {
    expect(inboxSource).toContain("container={canvas}");
    expect(inboxSource).toContain("sm:max-w-lg");
    expect(inboxSource).toContain("w-[min(32rem,calc(100%-2rem))]");
    expect(inboxSource).toContain("min-w-0");
    expect(inboxSource).toContain("<Command shouldFilter={false}");
    expect(inboxSource).toContain("<CommandItem");
    expect(inboxSource).toContain("grid-cols-[auto_minmax(0,1fr)_auto]");
    expect(inboxSource).toContain("overflow-x-hidden overflow-y-auto");
    expect(inboxSource).toContain('aria-label={t("conversations.newDialog.searchAria")}');
    expect(inboxSource).toContain('aria-label={t("conversations.newDialog.resultAria"');
    expect(inboxSource).toContain("if (!target?.isConnected) return");
    expect(inboxSource).toContain("target.focus()");
    expect(inboxSource).toContain("creators.error");
    expect(inboxSource).toContain("detail.error");
    expect(inboxSource).toContain("accounts.error");
    expect(inboxSource).toContain('role="alert"');
    expect(inboxSource).toContain("restoreFocusRef={composeButtonRef}");
  });
});
