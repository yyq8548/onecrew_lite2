import { describe, expect, it } from "vitest";

import APP_SOURCE from "@/App.tsx?raw";
import CONTROLLER_SOURCE from "@/components/pairing-gate-controller.tsx?raw";
import RUNTIME_SOURCE from "@/lib/pairing-gate-runtime.ts?raw";
import MAIN_SOURCE from "@/main.tsx?raw";

describe("PairingGateController source contract", () => {
  it("uses safe events first and keeps a five-minute pending-phase fallback", () => {
    const subscription = CONTROLLER_SOURCE.indexOf(
      "const subscription = subscribeCreatorSyncPairingStatus",
    );
    const initialRead = CONTROLLER_SOURCE.indexOf("if (active) void refresh()", subscription);
    expect(subscription).toBeGreaterThan(-1);
    expect(initialRead).toBeGreaterThan(subscription);
    expect(CONTROLLER_SOURCE).toContain("subscribeCreatorSyncPairingStatus");
    expect(CONTROLLER_SOURCE).toContain("Event delivery is the primary convergence path");
    expect(CONTROLLER_SOURCE).toContain("createPairingPollFallback");
    expect(RUNTIME_SOURCE).toContain('["waiting", "callback", "enrolling"]');
    expect(RUNTIME_SOURCE).toContain("PAIRING_POLL_INTERVAL_MS = 1_500");
    expect(RUNTIME_SOURCE).toContain("PAIRING_POLL_WINDOW_MS = 5 * 60 * 1_000");
    expect(RUNTIME_SOURCE).toContain("PAIRING_MAX_POLL_ATTEMPTS = Math.ceil");
  });

  it("keeps raw event payloads, URL opening, and native calls out of React", () => {
    for (const forbidden of [
      "@tauri-apps",
      "invoke(",
      "payload",
      "openExternal",
      "http://",
      "https://",
      "authorize_url",
      "pairing_code",
      "pairing_state",
    ]) {
      expect(CONTROLLER_SOURCE).not.toContain(forbidden);
    }
    expect(CONTROLLER_SOURCE).not.toMatch(/\blisten\(/);
  });

  it("mounts every normal provider only after pairing becomes active", () => {
    const localData = APP_SOURCE.indexOf("<LocalDataGate>");
    const pairing = APP_SOURCE.indexOf("<PairingGateController>");
    const activated = APP_SOURCE.indexOf("<ActivatedApp key={ownerKey}");
    expect(localData).toBeGreaterThan(-1);
    expect(localData).toBeLessThan(pairing);
    expect(pairing).toBeLessThan(activated);
    expect(APP_SOURCE).toContain("<RuntimeChangeBridge />");
    expect(APP_SOURCE).toContain("<WorkspaceApp />");
    expect(APP_SOURCE).not.toContain("McpStatusProvider");
    expect(MAIN_SOURCE).not.toContain("RuntimeChangeBridge");
  });

  it("clears old owner queries and wires only the four safe actions", () => {
    expect(CONTROLLER_SOURCE).toContain("queryClient.clear()");
    expect(CONTROLLER_SOURCE).toContain("pairingOwnerTransition");
    expect(CONTROLLER_SOURCE).toContain("ownerTransition.nextReadyOwner");
    expect(CONTROLLER_SOURCE).toContain("ownerTransition.renderOwner");
    expect(CONTROLLER_SOURCE).toContain("beginCreatorSyncPairing");
    expect(CONTROLLER_SOURCE).toContain("reopenCreatorSyncPairing");
    expect(CONTROLLER_SOURCE).toContain("cancelCreatorSyncPairing");
    expect(CONTROLLER_SOURCE).toContain("pairingGateActionHandlers");
    expect(CONTROLLER_SOURCE).not.toContain("await cancelCreatorSyncPairing()");
  });
});
