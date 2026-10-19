import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it, vi } from "vitest";
import { ResultCardView } from "@mcp-ui/features/result-card/view";
import { creatorSaveBlocked, creatorSaveRouteIds, toggleCreatorSaveRoute, creatorSaveRowSelectable } from "@mcp-ui/features/result-card/creator-save-state";
import { confirmCreatorSaveBatch } from "@mcp-ui/lib/result-client";
import { parseResultSnapshot, type CreatorSavePreview } from "@mcp-ui/lib/result-contract";
import { appMock } from "@mcp-ui/test-support/app-mock";
import { CREATOR_SAVE_PREVIEW, creatorSaveSnapshot, creatorSavedSnapshot } from "@mcp-ui/test-support/creator-save-fixture";

const handlers = { onConfirm: vi.fn(), onCancel: vi.fn(), onRequestEdit: vi.fn(), onSelect: vi.fn(), onConfirmCreatorSaveBatch: vi.fn() };

describe("One-row Creator save uses batch confirmation", () => {
  it("keeps required routes selected while optional choices remain independent", () => {
    const safe = structuredClone(CREATOR_SAVE_PREVIEW);
    safe.routes.forEach((route) => { route.collision = { status: "none", creator_ids: [] }; });
    const [required, email, site] = safe.routes;
    let selected = toggleCreatorSaveRoute([], required!, false);
    selected = toggleCreatorSaveRoute(selected, email!, true);
    selected = toggleCreatorSaveRoute(selected, site!, true);
    expect(creatorSaveRouteIds(safe, selected)).toEqual(["route-profile", "route-email", "route-site"]);
    expect(creatorSaveRouteIds(safe, toggleCreatorSaveRoute(selected, email!, false))).toEqual(["route-profile", "route-site"]);
  });

  it("rejects conflicting routes from stale selections and blocks required conflicts", () => {
    const preview = structuredClone(CREATOR_SAVE_PREVIEW);
    expect(creatorSaveRouteIds(preview, ["route-email", "route-site"])).toEqual(["route-profile", "route-site"]);
    expect(toggleCreatorSaveRoute(["route-email"], preview.routes[1]!, true)).toEqual([]);
    preview.routes[0]!.collision = { status: "conflict", creator_ids: ["other"] };
    preview.status = "contact_conflict";
    preview.reason = "required_contact_route_conflict";
    expect(creatorSaveBlocked(preview)).toBe(true);
  });

  it("takes row actionability from the owner status rather than identity guesses", () => {
    const preview = structuredClone(CREATOR_SAVE_PREVIEW);
    preview.identity_status = "new";
    preview.status = "existing_skipped";
    preview.reason = "existing_creator";
    expect(creatorSaveRowSelectable(preview)).toBe(false);
    expect(creatorSaveBlocked(preview)).toBe(true);
    expect(creatorSaveRouteIds(preview, ["route-site"])).toEqual([]);
  });

  it("accepts the owner's distinct reason for contradictory updates", () => {
    const preview = structuredClone(CREATOR_SAVE_PREVIEW);
    preview.status = "identity_pending";
    preview.reason = "conflicting_update";
    expect(parseResultSnapshot(creatorSaveSnapshot(preview))).not.toBeNull();
  });

  it("rejects owner status and intent combinations that cannot be true", () => {
    const preview = structuredClone(CREATOR_SAVE_PREVIEW);
    preview.intent = "add_new";
    preview.status = "eligible_update";
    expect(parseResultSnapshot(creatorSaveSnapshot(preview))).toBeNull();
    preview.intent = "save_or_update";
    preview.status = "existing_skipped";
    preview.reason = "existing_creator";
    expect(parseResultSnapshot(creatorSaveSnapshot(preview))).toBeNull();
  });

  it("renders an available one-row review without treating it as saved", () => {
    const snapshot = creatorSaveSnapshot();
    expect(parseResultSnapshot(snapshot)).toEqual(snapshot);
    expect(snapshot.pending_action?.kind).toBe("creator_save_batch");
    const html = renderToStaticMarkup(createElement(ResultCardView, { snapshot, busy: false, ...handlers }));
    expect(html).toContain("Demo beauty creator");
    expect(html).toContain("data-score-value");
    expect(html).not.toContain("Creator saved to OneCrew.");
  });

  it("calls only batch confirmation with the reviewed row and selected route IDs", async () => {
    const snapshot = creatorSaveSnapshot();
    const saved = creatorSavedSnapshot();
    const selections = [{ row_id: "save.creator.1", confirmed_route_ids: ["route-profile", "route-site"] }];
    const { app, callServerTool } = appMock([{ content: [], structuredContent: { kind: "snapshot", snapshot: saved } }]);
    expect(await confirmCreatorSaveBatch(app, snapshot.binding, selections)).toEqual(saved);
    expect(callServerTool).toHaveBeenCalledWith(
      { name: "onecrew_result_state", arguments: { action: "confirm_creator_save_batch", binding: snapshot.binding, selections } },
      { timeout: 180_000 },
    );
  });

  it("rejects retired single previews and duplicate routes", () => {
    const snapshot = creatorSaveSnapshot();
    expect(parseResultSnapshot({ ...snapshot, pending_action: { kind: "creator_save", preview: CREATOR_SAVE_PREVIEW } })).toBeNull();
    expect(parseResultSnapshot(creatorSaveSnapshot({ ...CREATOR_SAVE_PREVIEW, routes: [CREATOR_SAVE_PREVIEW.routes[0]!, CREATOR_SAVE_PREVIEW.routes[0]!] }))).toBeNull();
  });

  it("normalizes only nullable fields inside the batch row", () => {
    const omitted = structuredClone(creatorSaveSnapshot());
    if (omitted.pending_action?.kind !== "creator_save_batch") throw new Error("batch fixture");
    const preview: Partial<CreatorSavePreview> = omitted.pending_action.review.rows[0]!.preview;
    delete preview.notes;
    expect(parseResultSnapshot(omitted)?.pending_action).toMatchObject({ review: { rows: [{ preview: { notes: null } }] } });
    delete preview.routes;
    expect(parseResultSnapshot(omitted)).toBeNull();
  });
});
