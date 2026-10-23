import { describe, expect, it } from "vitest";

import GATE_SOURCE from "./pairing-gate.tsx?raw";
import { resources } from "@/i18n/resources";
import {
  pairingGateView,
  type PairingGatePhase,
} from "@/lib/pairing-gate-state";

const phases: PairingGatePhase[] = [
  "idle",
  "checking",
  "waiting",
  "callback",
  "enrolling",
  "expired",
  "canceled",
  "invalidated",
  "remote_unavailable",
  "rate_limited",
  "device_name_unavailable",
  "device_name_invalid",
  "error",
  "active",
];

describe("PairingGate translations", () => {
  it("provides both locale catalogs for every presentation semantic", () => {
    for (const locale of ["zh-CN", "en-US"] as const) {
      const catalog = resources[locale].pairing;
      for (const phase of phases) {
        const presentation = pairingGateView(phase);
        if (presentation.copy) {
          expect(catalog.states[presentation.copy].title).toBeTruthy();
          expect(catalog.states[presentation.copy].description).toBeTruthy();
        }
        if (presentation.primaryLabel) {
          expect(catalog.actions[presentation.primaryLabel]).toBeTruthy();
        }
        if (presentation.progress) {
          expect(catalog.progress[presentation.progress]).toBeTruthy();
        }
        if (presentation.alert) {
          expect(catalog.alerts[presentation.alert.copy].title).toBeTruthy();
          expect(catalog.alerts[presentation.alert.copy].description).toBeTruthy();
        }
      }
    }
  });

  it("contains no retired product names in fixed PairingGate copy", () => {
    const fixedCopy = JSON.stringify({
      chinese: resources["zh-CN"].pairing,
      english: resources["en-US"].pairing,
    });

    expect(fixedCopy).toContain("onecrew_lite");
    expect(fixedCopy).not.toMatch(/Reacher|Agent Platform/);
    expect(GATE_SOURCE).not.toMatch(/Reacher|Agent Platform/);
  });

  it("renders translations while leaving ownership and provider data outside the gate", () => {
    expect(GATE_SOURCE).toContain('useTranslation("pairing")');
    expect(GATE_SOURCE).toContain('t("actions.cancel")');
    expect(GATE_SOURCE).not.toMatch(/owner|identifier|provider|issue|status\.issue/);
  });
});
