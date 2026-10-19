import { describe, expect, it } from "vitest";

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

});
