import { describe, expect, it } from "vitest";

import CONNECT_ACTIVE_SOURCE from "@/assets/nav-icons/connect-active.svg?raw";
import CONTACTS_ACTIVE_SOURCE from "@/assets/nav-icons/contacts-active.svg?raw";
import CONVERSATIONS_ACTIVE_SOURCE from "@/assets/nav-icons/conversations-active.svg?raw";
import CREATORS_ACTIVE_SOURCE from "@/assets/nav-icons/creators-active.svg?raw";
import INBOX_ACTIVE_SOURCE from "@/assets/nav-icons/inbox-active.svg?raw";
import BRAND_SOURCE from "@/components/brand.tsx?raw";
import APP_SIDEBAR_SOURCE from "@/components/AppSidebar.tsx?raw";
import PLATFORM_SOURCE from "@/components/platform.tsx?raw";
import SIDEBAR_PRIMITIVE_SOURCE from "@/components/ui/sidebar.tsx?raw";

const ACTIVE_NAV_SOURCES = [
  CONNECT_ACTIVE_SOURCE,
  CONTACTS_ACTIVE_SOURCE,
  CONVERSATIONS_ACTIVE_SOURCE,
  CREATORS_ACTIVE_SOURCE,
  INBOX_ACTIVE_SOURCE,
];

describe("OneCrew brand contract", () => {
  it("uses the canonical OneCrew mark instead of the retired dot lattice", () => {
    expect(BRAND_SOURCE).toContain('src="/onecrew-ai-mark.png"');
    expect(BRAND_SOURCE).toContain("export function OneCrewMark");
    expect(BRAND_SOURCE).not.toContain("<circle");

    expect(BRAND_SOURCE).not.toContain("onecrew-favicon.svg");
  });

  it("draws every platform glyph in the ink it inherits", () => {
    expect(PLATFORM_SOURCE).toContain('fill="currentColor"');
    expect(PLATFORM_SOURCE).not.toContain('fill="white"');
  });

  it("uses OneCrew brand ink for every selected navigation glyph", () => {
    expect(ACTIVE_NAV_SOURCES).toHaveLength(5);
    for (const source of ACTIVE_NAV_SOURCES) {
      expect(source).toContain("#7A5A00");
      expect(source).not.toContain("#C33B2E");
    }
    expect(APP_SIDEBAR_SOURCE).toContain("active ? glyph.selected : glyph.rest");
  });

  it("uses the complete soft-yellow surface for selected navigation rows", () => {
    expect(SIDEBAR_PRIMITIVE_SOURCE).toContain("data-[active=true]:bg-sidebar-accent");
    expect(SIDEBAR_PRIMITIVE_SOURCE).toContain("data-[active=true]:text-sidebar-accent-foreground");
    expect(SIDEBAR_PRIMITIVE_SOURCE).toContain("dark:data-[active=true]:[&_img]:brightness-0");
    expect(SIDEBAR_PRIMITIVE_SOURCE).toContain("dark:data-[active=true]:[&_img]:invert");
    expect(SIDEBAR_PRIMITIVE_SOURCE).not.toContain("data-[active=true]:bg-[var(--brand-accent)]/10");
  });
});
