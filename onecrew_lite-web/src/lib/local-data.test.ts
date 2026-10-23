import { beforeEach, describe, expect, it, vi } from "vitest";

const { invokeMock } = vi.hoisted(() => ({ invokeMock: vi.fn() }));
vi.mock("@/lib/local-runtime", () => ({ invoke: invokeMock }));
import { inspectLocalData, failureMessage, type DataStartupStatus } from "./local-data";

const inspecting = { phase: "inspecting", preflight: null, postflight: null, upgrade: null, failure: null, diagnostic_code: null } as const;

const upgrading = {
  ...inspecting,
  phase: "upgrading",
  preflight: {
    state: "upgrade_required",
    blocked_store: null,
    failure_kind: null,
    message: null,
    primary_action: null,
    secondary_action: null,
    recoverable_backup: false,
    remote_data_deleted: false,
    support_code: null,
    title: null,
    targets: [{
      store: "business",
      from_version: 36,
      to_version: 37,
      impact: {
        cleared_audit_campaign_refs: 0,
        cleared_reply_triage_task_refs: 0,
        converted_campaign_community_sources: 0,
        converted_campaign_search_origins: 0,
        converted_campaign_search_snapshots: 0,
        deleted_task_attention: 0,
        deleted_task_idempotency_records: 0,
        deleted_task_runs: 0,
        deleted_tasks: 0,
        removed_campaign_account_bindings: 0,
        removed_campaign_delete_operations: 0,
        removed_campaign_idempotency_records: 0,
        removed_campaign_template_delete_operations: 0,
        removed_campaign_templates: 0,
        removed_campaigns: 0,
        removed_unattributable_campaign_origins: 0,
        removed_candidate_snapshots: 2,
        rewritten_contact_route_delete_impacts: 3,
        removed_tiktok_one_account_bindings: 4,
        removed_creator_save_confirmations: 5,
        removed_contact_confirmations: 6,
        unresolved_retired_outreach_actions: 7,
      },
    }],
  },
} satisfies DataStartupStatus;

describe("read-only data startup client", () => {
  beforeEach(() => invokeMock.mockReset());

  it("only reads the service-owned startup projection", async () => {
    invokeMock.mockResolvedValue(inspecting);
    expect(await inspectLocalData()).toEqual(inspecting);
    expect(invokeMock.mock.calls).toEqual([["local_data_status"]]);
  });

  it("rejects malformed or instruction-bearing startup projections", async () => {
    for (const value of [{ phase: "ready" }, { ...inspecting, run_upgrade: true }, { ...inspecting, phase: "upgrade_now" }]) {
      invokeMock.mockResolvedValue(value);
      await expect(inspectLocalData()).rejects.toThrow("invalid_data_startup_status");
    }
  });

  it("reads the owner-generated v37 impact without dropping any retirement counters", async () => {
    invokeMock.mockResolvedValue(upgrading);
    expect(await inspectLocalData()).toEqual(upgrading);
    expect(invokeMock.mock.calls).toEqual([["local_data_status"]]);
  });

  it.each([
    "removed_candidate_snapshots",
    "rewritten_contact_route_delete_impacts",
    "removed_tiktok_one_account_bindings",
    "removed_creator_save_confirmations",
    "removed_contact_confirmations",
    "unresolved_retired_outreach_actions",
  ])("rejects an incomplete owner impact missing %s", async (missingField) => {
    const target = upgrading.preflight.targets[0];
    const impact = Object.fromEntries(Object.entries(target.impact).filter(([key]) => key !== missingField));
    invokeMock.mockResolvedValue({
      ...upgrading,
      preflight: { ...upgrading.preflight, targets: [{ ...target, impact }] },
    });
    await expect(inspectLocalData()).rejects.toThrow("invalid_data_startup_status");
  });

  it("preserves the stronger recovery wording after a committed subset", () => {
    expect(failureMessage("initialization_failed", true)).not.toBe(failureMessage("initialization_failed", false));
  });
});
