import { describe, expect, it } from "vitest";

import STATE_SOURCE from "./pairing-gate-state.ts?raw";
import type { LoginProjection } from "@/lib/generated/creator-sync-contracts";
import {
  pairingGatePhase,
  pairingGateView,
  pairingOwnerKey,
  type PairingGatePhase,
} from "./pairing-gate-state";

function projection(
  state: LoginProjection["status"]["state"],
  overrides: Partial<LoginProjection> = {},
): LoginProjection {
  return {
    browser_opened: false,
    callback_received: false,
    issue: null,
    flow_id: state === "pairing_pending" ? "flow-1" : null,
    status: {
      action: null,
      credential_id: state === "active" ? "credential-1" : null,
      installation_id: state === "active" ? "installation-1" : null,
      issue: null,
      state,
    },
    ...overrides,
  };
}

describe("PairingGate presentation state", () => {
  it("presents one browser binding action while idle", () => {
    expect(pairingGateView("idle")).toMatchObject({
      copy: "idle",
      primaryAction: "begin",
      primaryLabel: "begin",
      canCancel: false,
      progress: null,
      alert: null,
      renderChildren: false,
    });
  });

  it("has one neutral checking state for initial local status reads", () => {
    expect(pairingGateView("checking")).toMatchObject({
      copy: "checking",
      primaryAction: null,
      progress: "checking",
    });
  });

  it("keeps waiting actionable without adding another account path", () => {
    expect(pairingGateView("waiting")).toMatchObject({
      copy: "waiting",
      primaryAction: "reopen",
      primaryLabel: "reopen",
      canCancel: true,
      progress: null,
    });
  });

  it.each([
    ["callback", "callback"],
    ["enrolling", "enrolling"],
  ] as const)("keeps %s in a stable progress-only state", (phase, progress) => {
    expect(pairingGateView(phase)).toMatchObject({
      copy: phase,
      primaryAction: null,
      canCancel: false,
      progress,
      alert: null,
    });
  });

  it.each(["expired", "canceled", "error"] as const)(
    "makes %s a fixed safe restart state",
    (phase) => {
      expect(pairingGateView(phase)).toMatchObject({
        primaryAction: "restart",
        primaryLabel: "restart",
        canCancel: false,
        progress: null,
        renderChildren: false,
      });
      expect(pairingGateView(phase).alert).not.toBeNull();
    },
  );

  it("presents remote credential loss once as an explicit rebind action", () => {
    expect(pairingGateView("invalidated")).toMatchObject({
      copy: "invalidated",
      primaryAction: "restart",
      primaryLabel: "rebind",
      canCancel: false,
      alert: {
        variant: "warning",
        copy: "invalidated",
      },
    });
  });

  it("presents an actionable native-network failure without calling it an authorization rejection", () => {
    expect(pairingGateView("remote_unavailable")).toMatchObject({
      copy: "remoteUnavailable",
      primaryAction: "restart",
      primaryLabel: "retryNetwork",
      alert: {
        variant: "warning",
        copy: "remoteUnavailable",
      },
    });
    expect(pairingGatePhase(projection("temporarily_unavailable", {
      status: {
        ...projection("temporarily_unavailable").status,
        issue: "remote_unavailable",
      },
    }))).toBe("remote_unavailable");
  });

  it("keeps a remote limit retryable without describing it as an incomplete binding", () => {
    expect(pairingGateView("rate_limited")).toMatchObject({
      copy: "rateLimited",
      primaryAction: "restart",
      primaryLabel: "retry",
      alert: {
        variant: "warning",
        copy: "rateLimited",
      },
    });
    expect(pairingGatePhase(projection("temporarily_unavailable", {
      status: {
        ...projection("temporarily_unavailable").status,
        issue: "remote_rate_limited",
      },
    }))).toBe("rate_limited");
  });

  it("renders active children directly without an intermediate success view", () => {
    expect(pairingGateView("active")).toEqual({
      copy: null,
      primaryAction: null,
      primaryLabel: null,
      canCancel: false,
      progress: null,
      alert: null,
      renderChildren: true,
    });
  });

  it("covers every closed phase", () => {
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
      "error",
      "active",
    ];
    expect(phases.map((phase) => pairingGateView(phase))).toHaveLength(12);
  });

  it("keeps state semantics independent from localized UI copy", () => {
    expect(STATE_SOURCE).not.toMatch(/Reacher|OneCrew|正在|绑定未完成|重新开始/);
    expect(STATE_SOURCE).toContain('copy: "remoteUnavailable"');
    expect(STATE_SOURCE).toContain('primaryLabel: "retryNetwork"');
  });

  it("distinguishes native name lookup and label validation failures before enrollment", () => {
    for (const issue of ["device_name_unavailable", "device_name_invalid"] as const) {
      const phase = pairingGatePhase(projection("pairing_required", { issue: issue }));
      expect(phase).toBe(issue);
      expect(pairingGateView(phase)).toMatchObject({ primaryAction: "restart", canCancel: false, renderChildren: false });
    }
    expect(pairingGateView("device_name_unavailable").copy).toBe("deviceNameUnavailable");
    expect(pairingGateView("device_name_invalid").copy).toBe("deviceNameInvalid");
  });

  it("maps safe runtime projections into presentation phases", () => {
    expect(pairingGatePhase(projection("pairing_required"))).toBe("idle");
    expect(pairingGatePhase(projection("pairing_pending"))).toBe("waiting");
    expect(pairingGatePhase(projection("pairing_pending", { callback_received: true }))).toBe("callback");
    expect(pairingGatePhase(projection("pairing_completing"))).toBe("enrolling");
    expect(pairingGatePhase(projection("validating"))).toBe("checking");
    expect(pairingGatePhase(projection("validation_required"))).toBe("error");
    expect(pairingGatePhase(projection("active"))).toBe("active");
    expect(pairingGatePhase(projection("pairing_required", {
      issue: "callback_expired",
    }))).toBe("expired");
    expect(pairingGatePhase(projection("pairing_required", {
      issue: "canceled",
    }))).toBe("canceled");
    expect(pairingGatePhase(projection("recovery_required"))).toBe("error");
    expect(pairingGatePhase(projection("revoked", {
      status: {
        ...projection("revoked").status,
        issue: "remote_revoked",
      },
    }))).toBe("invalidated");
    expect(pairingGatePhase(projection("unauthorized", {
      status: {
        ...projection("unauthorized").status,
        issue: "remote_unauthorized",
      },
    }))).toBe("invalidated");
    expect(pairingGatePhase(projection("revoked", {
      status: {
        ...projection("revoked").status,
        issue: "safe_cleanup_incomplete",
      },
    }))).toBe("error");
  });

  it("requires both active owner identifiers before exposing children", () => {
    const active = projection("active");
    expect(pairingOwnerKey(active)).toBe("creator-sync:installation-1:credential-1");
    expect(pairingGatePhase({
      ...active,
      status: { ...active.status, credential_id: null },
    })).toBe("error");
  });
});
