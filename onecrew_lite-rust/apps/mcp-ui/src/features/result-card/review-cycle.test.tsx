import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { creatorSaveSnapshot, creatorSavedSnapshot } from "@mcp-ui/test-support/creator-save-fixture";
import { creatorBatchSelections, initialCreatorBatchDraft } from "./creator-batch-state";
import { reviewRevisionAfterSnapshot, resultStatusPresentation } from "./presentation";
import { ResultCardView } from "./view";
import { parseResultSnapshot } from "@mcp-ui/lib/result-contract";
import { SNAPSHOT } from "@mcp-ui/test-support/fixtures";

describe("same-result successive owner reviews", () => {
  it("parses and renders the runtime outreach kinds without making a pending record look completed", () => {
    const review = structuredClone(SNAPSHOT);
    review.locale = "en-US";
    review.presentation = { kind: "outreach_confirmation", title: "Review message", summary: null,
      sections: [{ kind: "fields", heading: null, fields: [{ label: "Body", value: "Frozen message" }] }] };
    review.pending_action = { kind: "confirm", prompt: "Send this exact message?", label: "Confirm message" };
    review.batch.outcomes[0]!.operation_id = "onecrew.outreach.message.send";
    expect(parseResultSnapshot(review)).not.toBeNull();
    const handlers = { onConfirm() {}, onSelect() {}, onCancel() {}, onRequestEdit() {} };
    expect(renderToStaticMarkup(createElement(ResultCardView, { snapshot: review, busy: false, ...handlers }))).toContain("Frozen message");
    const outcome = structuredClone(review);
    outcome.pending_action = null;
    outcome.binding.revision += 1;
    outcome.completed_sections_count = 1;
    outcome.state = { kind: "unknown", code: "outreach_record_pending", summary: "Remote receipt verified; local recording is pending." };
    outcome.presentation = { kind: "outreach_outcome", title: "Message outcome", summary: null,
      sections: [{ kind: "notice", heading: null, body: outcome.state.summary, tone: "warning" }] };
    outcome.batch.outcomes[0]!.disposition = { kind: "unknown", code: "outreach_record_pending" };
    expect(parseResultSnapshot(outcome)).not.toBeNull();
    expect(resultStatusPresentation(outcome).terminal?.tone).toBe("warning");
    const html = renderToStaticMarkup(createElement(ResultCardView, { snapshot: outcome, busy: false, ...handlers }));
    expect(html).toContain("local recording is pending");
    expect(html).not.toContain("Confirm message");
    expect(parseResultSnapshot({ ...outcome, pending_action: review.pending_action })).toBeNull();
    expect(parseResultSnapshot(creatorSavedSnapshot())?.presentation.kind).toBe("creator_save_batch_outcome");
  });
  it("starts fresh choices after an acknowledged second review even when local row/route IDs repeat", () => {
    const first = creatorSaveSnapshot();
    const second = structuredClone(first);
    second.binding.revision += 2;
    if (first.pending_action?.kind !== "creator_save_batch" || second.pending_action?.kind !== "creator_save_batch") throw new Error("review fixture");
    second.pending_action.review.rows[0]!.preview.display_name = "A different Creator";
    const previousDraft = initialCreatorBatchDraft(first.pending_action.review.rows);
    previousDraft.selectedIds = [];
    expect(reviewRevisionAfterSnapshot(first.binding.revision, first, second, true)).toBe(second.binding.revision);
    expect(creatorBatchSelections(second.pending_action.review.rows, initialCreatorBatchDraft(second.pending_action.review.rows))).toHaveLength(1);
    expect(previousDraft.selectedIds).toEqual([]);
  });
  it("preserves a draft for a plain conflict read, but resets when completed-prefix growth proves the next review", () => {
    const before = creatorSaveSnapshot();
    const next = structuredClone(before);
    next.binding.revision += 2;
    expect(reviewRevisionAfterSnapshot(before.binding.revision, before, next, false)).toBe(before.binding.revision);
    next.completed_sections_count = 1;
    next.presentation.sections = creatorSavedSnapshot().presentation.sections;
    expect(reviewRevisionAfterSnapshot(before.binding.revision, before, next, false)).toBe(next.binding.revision);
    next.state = { kind: "submitted", action: { kind: "confirmed" } };
    expect(reviewRevisionAfterSnapshot(before.binding.revision, before, next, true)).toBe(before.binding.revision);
  });
  it("renders the completed prefix once alongside only the current candidate review", () => {
    const next = creatorSaveSnapshot();
    if (next.pending_action?.kind !== "creator_save_batch") throw new Error("review fixture");
    next.pending_action.review.rows[0]!.preview.display_name = "Second candidate";
    next.completed_sections_count = 1;
    next.presentation.sections = [...creatorSavedSnapshot().presentation.sections,
      { kind: "fields", heading: null, fields: [{ label: "Internal current detail", value: "CURRENT_DUPLICATE_SENTINEL" }] }];
    const html = renderToStaticMarkup(createElement(ResultCardView, { snapshot: next, busy: false, onConfirm() {}, onSelect() {}, onCancel() {}, onRequestEdit() {}, onConfirmCreatorSaveBatch() {} }));
    expect(html).toContain("Demo beauty creator");
    expect(html).toContain("Second candidate");
    expect(html).not.toContain("CURRENT_DUPLICATE_SENTINEL");
    expect(parseResultSnapshot(next)).not.toBeNull();
    expect(parseResultSnapshot({ ...next, completed_sections_count: 3 })).toBeNull();
  });
  it("retains completed rows while reporting the current review expiry or uncertain effect", () => {
    const snapshot = creatorSavedSnapshot();
    snapshot.state = { kind: "failed", code: "review_expired", summary: "Second review expired; the first save is retained.", retryable: false };
    expect(resultStatusPresentation(snapshot).terminal?.tone).toBe("warning");
    expect(resultStatusPresentation(snapshot).terminal?.detail).toBe(snapshot.state.summary);
    snapshot.state = { kind: "unknown", code: "owner_unknown", summary: "Current effect is unknown; prior save is retained." };
    expect(resultStatusPresentation(snapshot).terminal?.detail).toBe(snapshot.state.summary);
    expect(snapshot.presentation.sections[0]?.kind).toBe("creator_save_rows");
  });
});
