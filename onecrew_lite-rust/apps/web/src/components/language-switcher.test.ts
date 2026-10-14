import { describe, expect, it } from "vitest";

import SWITCHER_SOURCE from "./language-switcher.tsx?raw";
import SIDEBAR_SOURCE from "./AppSidebar.tsx?raw";
import { nextLocale } from "./language-switcher";

describe("sidebar language switcher", () => {
  it("toggles only between the two supported UI catalogs", () => {
    expect(nextLocale("zh-CN")).toBe("en-US");
    expect(nextLocale("en-US")).toBe("zh-CN");
  });

  it("uses the Access language glyph and the product rail primitive", () => {
    expect(SWITCHER_SOURCE).toContain('import { Languages } from "lucide-react"');
    expect(SWITCHER_SOURCE).toContain('variant="rail"');
    expect(SWITCHER_SOURCE).toContain('size="rail"');
    expect(SWITCHER_SOURCE).toContain("aria-label={switchLabel}");
    expect(SWITCHER_SOURCE).toContain("group-data-[collapsible=icon]:hidden");
  });

  it("mounts at the bottom of the application sidebar", () => {
    expect(SIDEBAR_SOURCE).toContain("<SidebarFooter");
    expect(SIDEBAR_SOURCE).toContain("<LanguageSwitcher />");
    expect(SIDEBAR_SOURCE).not.toContain("text-white");
  });
});
