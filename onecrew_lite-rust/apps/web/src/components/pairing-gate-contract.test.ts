import { describe, expect, it } from "vitest";

import APP_SOURCE from "@/App.tsx?raw";
import GATE_SOURCE from "@/components/pairing-gate.tsx?raw";

describe("PairingGate source contract", () => {
  it("reuses the existing brand and shared UI primitives", () => {
    expect(GATE_SOURCE).toContain('from "@/components/brand"');
    expect(GATE_SOURCE).toContain('from "@/components/ui/alert"');
    expect(GATE_SOURCE).toContain('from "@/components/ui/button"');
    expect(GATE_SOURCE).toContain('useTranslation("pairing")');
    expect(GATE_SOURCE).toContain("<OneCrewMark");
    expect(GATE_SOURCE).toContain("<Alert");
    expect(GATE_SOURCE).toContain("<Button");
    expect(GATE_SOURCE).toContain("<Loader2");
  });

  it("is a full-window accessible narrow layout with reduced-motion progress", () => {
    expect(GATE_SOURCE).toContain("min-h-svh");
    expect(GATE_SOURCE).toContain("place-items-center");
    expect(GATE_SOURCE).toContain("max-w-md");
    expect(GATE_SOURCE).toContain('aria-labelledby="pairing-gate-title"');
    expect(GATE_SOURCE).toContain('role="status"');
    expect(GATE_SOURCE).toContain('aria-live="polite"');
    expect(GATE_SOURCE).toContain("motion-reduce:animate-none");
    expect(GATE_SOURCE).toContain('aria-hidden="true"');
    expect(GATE_SOURCE).toContain("bg-[var(--tone-progress-bg)]");
    expect(GATE_SOURCE).toContain("text-[var(--tone-progress-fg)]");
    expect(GATE_SOURCE).toContain("dark:bg-card/70");
    expect(GATE_SOURCE).not.toContain("data-tauri-drag-region");
  });

  it("keeps presentation disconnected from native and alternative account paths", () => {
    for (const forbidden of [
      "@tauri-apps",
      "invoke(",
      "openExternal",
      "plugin-opener",
      "http://",
      "https://",
      "mcp",
      "<Dialog",
      "<dialog",
      "<Input",
      "注册",
      "服务条款",
      "激活码",
      "手动",
    ]) {
      expect(GATE_SOURCE.toLowerCase()).not.toContain(forbidden.toLowerCase());
    }
    expect(APP_SOURCE).toContain("PairingGateController");
    expect(APP_SOURCE).not.toContain('from "@/components/pairing-gate"');
  });

  it("returns active children directly and exposes only controller actions", () => {
    expect(GATE_SOURCE).toContain("return <>{children}</>;");
    for (const action of ["onBegin", "onReopen", "onCancel", "onRestart"]) {
      expect(GATE_SOURCE).toContain(action);
    }
    expect(GATE_SOURCE).not.toContain("useState(");
    expect(GATE_SOURCE).not.toContain("useEffect(");
  });
});
