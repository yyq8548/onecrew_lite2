import { webResultBinding, type OwnerReviewSnapshot } from "@/lib/owner-review";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { creatorInvoke } from "@/lib/creator";
import { confirmTelegramAdministratorSave, prepareTelegramAdministratorSave, readCreatorSaveResult, creatorSaveReviewBlocked, creatorSaveReviewRows, creatorSaveSelections, savedTelegramCreatorId, telegramAdministratorCandidateInput,
  readTelegramCommunityAdministrators, type TelegramAdministrator } from "@/lib/telegram-candidate";

vi.mock("@/lib/creator", () => ({ creatorInvoke: vi.fn() }));
beforeEach(() => vi.mocked(creatorInvoke).mockReset());

const admin: TelegramAdministrator = { role: "owner", user: { id: "123", username: "admin", first_name: "Admin", last_name: null, is_bot: false, verified: true }, eligibility: { status: "ready" } };
const source = { source_creator_id: "source", source_contact_route_id: "route", operator_account_ref_id: "operator" };
const community = { external_id: "456", title: "Community", username: null, kind: "supergroup" as const };

it("rejects missing or unknown owner eligibility instead of treating a row as ready", async () => {
  for (const eligibility of [undefined, null, { status: "ready", reason: "ignored" }, { status: "blocked", reason: "unknown_reason" }]) {
    vi.mocked(creatorInvoke).mockResolvedValueOnce({ community, page_limit: 200,
      administrators: [{ ...admin, eligibility }],
    });
    await expect(readTelegramCommunityAdministrators(source)).rejects.toThrow("telegram_administrator_response_invalid");
  }
});

it("uses refreshed owner status while mapping only identity and provenance into preparation", async () => {
  const blocked: TelegramAdministrator = { ...admin, eligibility: { status: "blocked", reason: "missing_public_handle" } };
  for (const administrator of [blocked, admin]) {
    vi.mocked(creatorInvoke).mockResolvedValueOnce({ community, page_limit: 200, administrators: [administrator] });
    const page = await readTelegramCommunityAdministrators(source);
    expect(page.administrators[0].eligibility).toEqual(administrator.eligibility);
    const input = telegramAdministratorCandidateInput(page.administrators[0], page.community, source);
    expect(input).not.toHaveProperty("eligibility");
    expect(input.target.user_id).toBe("123");
  }
});

function snapshot(): OwnerReviewSnapshot {
  return {
    binding: { result_ref: "rr_123", view: { identity: "view", kind: "confirmation" }, fingerprint: "fingerprint", revision: 1, origin: "trusted-browser" },
    state: { kind: "available" },
    completed_sections_count: 0,
    pending_action: { kind: "creator_save_batch", review: { rows: [{ row_id: "row", preview: {
      display_name: "Admin", platform: "telegram", handle: "admin", profile_url: null,
      avatar_url: null, identity_status: "new", identity_creator_ids: [], notes: null, score: null,
      assessment: null, follower_counts: {}, expires_at: 1900000000,
      routes: [
        { route_id: "required", channel: "telegram", kind: "handle", display_value: "@admin", normalized_value: "admin", sources: [], required: true, collision: { status: "none", creator_ids: [] } },
        { route_id: "optional", channel: "website", kind: "url", display_value: "https://example.test", normalized_value: "https://example.test", sources: [], required: false, collision: { status: "none", creator_ids: [] } },
      ],
    } }] } },
    presentation: { title: "Review", summary: null, sections: [] },
  };
}

describe("Telegram administrator selection and confirmed save", () => {
  it("prepares direct candidate input and requires a separate confirm request", async () => {
    const prepared = snapshot();
    vi.mocked(creatorInvoke).mockResolvedValueOnce(prepared);
    expect(await prepareTelegramAdministratorSave(admin, community, source)).toBe(prepared);
    expect(vi.mocked(creatorInvoke).mock.calls.map(([name]) => name)).toEqual(["onecrew_creator_save_prepare"]);
    expect(creatorInvoke).toHaveBeenLastCalledWith("onecrew_creator_save_prepare", { input: { candidates: [telegramAdministratorCandidateInput(admin, community, source)] } });
    vi.mocked(creatorInvoke).mockResolvedValueOnce(prepared);
    await confirmTelegramAdministratorSave(prepared, [{ row_id: "row", confirmed_route_ids: ["required"] }]);
    expect(creatorInvoke).toHaveBeenLastCalledWith("onecrew_creator_save_confirm", { input: { binding: webResultBinding(prepared.binding), selections: [{ row_id: "row", confirmed_route_ids: ["required"] }] } });
    vi.mocked(creatorInvoke).mockResolvedValueOnce(prepared);
    await readCreatorSaveResult(prepared);
    expect(creatorInvoke).toHaveBeenLastCalledWith("onecrew_creator_save_result_get", { input: webResultBinding(prepared.binding) });
  });
  it("keeps stable community provenance and selected user without copying provider payload", () => {
    const enriched = { ...admin, user: { ...admin.user, phone: "PRIVATE_PHONE", access_hash: "PRIVATE_HASH" } };
    const input = telegramAdministratorCandidateInput(enriched, community, source);
    expect(input.target).toEqual({ platform: "telegram_administrator", user_id: "123", community: { kind: "saved_community_route", creator_id: "source", route_id: "route" } });
    expect(input.provenance.community_id).toBe("456");
    expect(input.provenance.community_kind).toBe("supergroup");
    expect(JSON.stringify(input)).not.toContain("PRIVATE");
    expect(input).not.toHaveProperty("discovery_id");
  });
  it("maps stable administrator identity and trusted source when the displayed handle is missing", () => {
    const withoutDisplayedHandle = { ...admin, user: { ...admin.user, username: null } };
    let input: ReturnType<typeof telegramAdministratorCandidateInput> | undefined;
    expect(() => {
      input = telegramAdministratorCandidateInput(withoutDisplayedHandle, community, source);
    }).not.toThrow();
    expect(input).toMatchObject({
      target: { platform: "telegram_administrator", user_id: "123", community: { kind: "saved_community_route", creator_id: "source", route_id: "route" } },
      connection: { platform: "telegram", account_ref_id: "operator" },
      provenance: { kind: "administrator_selection", community_id: "456", community_kind: "supergroup", source: { kind: "saved_community_route", creator_id: "source", route_id: "route" } },
    });
    expect(creatorInvoke).not.toHaveBeenCalled();
  });
  it("never transmits authority origin or arbitrary snapshot data to confirmation", () => {
    const binding = webResultBinding(snapshot().binding);
    expect(binding).not.toHaveProperty("origin");
    expect(Object.keys(binding)).toEqual(["result_ref", "view", "fingerprint", "revision"]);
  });
  it("confirms only displayed chosen routes and retains required routes", () => {
    const rows = creatorSaveReviewRows(snapshot());
    expect(creatorSaveSelections(rows, {})).toEqual([{ row_id: "row", confirmed_route_ids: ["required"] }]);
    expect(creatorSaveSelections(rows, { row: ["optional", "forged-route"] })).toEqual([{ row_id: "row", confirmed_route_ids: ["required", "optional"] }]);
  });
  it("does not truncate a 1000-row review to the generic MCP call budget", () => {
    const base = creatorSaveReviewRows(snapshot())[0];
    const rows = Array.from({ length: 1000 }, (_, index) => ({ ...structuredClone(base), row_id: `row-${index}` }));
    const selected = creatorSaveSelections(rows, {});
    expect(selected).toHaveLength(1000);
    expect(selected[999]).toEqual({ row_id: "row-999", confirmed_route_ids: ["required"] });
  });
  it("does not label accepted, submitted, unknown or failed outcomes saved", () => {
    const value = snapshot();
    value.presentation.sections = [{ kind: "creator_save_rows", rows: [{ row_id: "row", status: "saved", creator_id: "creator-1", failure_code: null }] }];
    for (const state of [{ kind: "available" }, { kind: "submitted" }, { kind: "unknown", code: "unknown", summary: "Unknown" }, { kind: "failed", code: "failed", summary: "Failed" }] as const) {
      value.state = state;
      expect(savedTelegramCreatorId(value)).toBeNull();
    }
    value.state = { kind: "resolved", code: "saved", summary: "Saved" };
    expect(savedTelegramCreatorId(value)).toBe("creator-1");
    expect(creatorSaveReviewRows(value)).toEqual([]);
  });
  it("excludes conflicted optional routes and blocks a conflicted required route", () => {
    const rows = creatorSaveReviewRows(snapshot());
    rows[0].preview.routes[1].collision = { status: "possible", creator_ids: ["existing"] };
    expect(creatorSaveSelections(rows, { row: ["optional"] })).toEqual([{ row_id: "row", confirmed_route_ids: ["required"] }]);
    expect(creatorSaveReviewBlocked(rows)).toBe(false);
    rows[0].preview.routes[0].collision = { status: "conflict", creator_ids: ["existing"] };
    expect(creatorSaveReviewBlocked(rows)).toBe(true);
  });
});
