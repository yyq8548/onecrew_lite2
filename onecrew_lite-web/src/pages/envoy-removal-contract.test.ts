import { describe, expect, it } from "vitest";

import LEADS_SOURCE from "@/pages/Leads.tsx?raw";
import APP_SIDEBAR_SOURCE from "@/components/AppSidebar.tsx?raw";

describe("Phase B frontend Envoy removal", () => {
  it("removes Envoy CTAs from the Creator surface", () => {
    expect(LEADS_SOURCE).not.toMatch(/Envoy|envoy|prefillChat|handoff-prompt/);
    expect(LEADS_SOURCE).toContain('t("table.archive")');
  });

  it("removes the retired Envoy artwork from the sidebar", () => {
    expect(APP_SIDEBAR_SOURCE).not.toContain("EnvoyLauncher");
    expect(APP_SIDEBAR_SOURCE).not.toContain("ReacherRippleIcon");
    expect(APP_SIDEBAR_SOURCE).not.toContain("reacher-ripple-hover-ink.svg");
  });
});
