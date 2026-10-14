import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it, vi } from "vitest";
import { CreatorSaveRecovery } from "@mcp-ui/features/result-card/components/creator-save-recovery";
import { ResultCardView } from "@mcp-ui/features/result-card/view";
import { AppLocaleProvider, translate } from "@mcp-ui/lib/i18n";
import { creatorSaveSnapshot } from "@mcp-ui/test-support/creator-save-fixture";

describe("Creator conflict recovery", () => {
  it("lets a stale review acknowledge the current revision without confirming a save", () => {
    const html = renderToStaticMarkup(<AppLocaleProvider locale="en-US"><CreatorSaveRecovery stale busy={false} syncing={false} blocked={false} onAcknowledge={() => {}} /></AppLocaleProvider>);
    expect(html).toContain("Use the current route review");
    expect(html).not.toContain("disabled=");
    expect(html).not.toContain("Confirm addition");
  });

  it("keeps readback available while stale input and an uncertain result block acknowledgement", () => {
    const html = renderToStaticMarkup(<AppLocaleProvider locale="en-US"><CreatorSaveRecovery stale busy={false} syncing={false} blocked failure={{ kind: "transport" }} onAcknowledge={() => {}} onRefresh={() => {}} /></AppLocaleProvider>);
    const buttons = [...html.matchAll(/<button\b[^>]*>[\s\S]*?<\/button>/g)].map(match => match[0]);
    expect(buttons.find(button => button.includes("Use the current route review"))).toContain("disabled=");
    expect(buttons.find(button => button.includes("Check current preview"))).not.toContain("disabled=");
  });

  it("renders the recovery action inside a required-conflict card after failed readback", () => {
    const snapshot = creatorSaveSnapshot();
    snapshot.locale = "en-US";
    if (snapshot.pending_action?.kind !== "creator_save_batch") throw new Error("fixture");
    snapshot.pending_action.review.rows[0]!.preview = {
      ...snapshot.pending_action.review.rows[0]!.preview,
      status: "contact_conflict",
      reason: "required_contact_route_conflict",
    };
    const handlers = { onConfirm: vi.fn(), onCancel: vi.fn(), onRequestEdit: vi.fn(), onSelect: vi.fn(), onConfirmCreatorSaveBatch: vi.fn(), onRefreshSearchPlan: vi.fn() };
    const html = renderToStaticMarkup(<ResultCardView snapshot={snapshot} busy={false} revisionBlocked revisionFailure={{ kind: "transport" }} {...handlers} />);
    expect(html).toContain(translate("en-US", "creatorStatusContactConflict"));
    expect(html).toContain("A required contact route belongs to another Creator");
    expect(html).toContain("Saving could not be verified");
    expect(html).not.toContain("Confirm addition");
    const buttons = [...html.matchAll(/<button\b[^>]*>[\s\S]*?<\/button>/g)].map(match => match[0]);
    expect(buttons.find(button => button.includes("Check current preview"))).not.toContain("disabled=");
    // Read-only row expansion and score tooltips stay usable. Save authority,
    // cancellation, and selection mutation must remain disabled until readback.
    expect(buttons.find(button => button.includes("Save or update selected creators"))).toContain("disabled=");
    expect(buttons.find(button => button.includes(">Cancel<"))).toContain("disabled=");
    expect(buttons.filter(button => button.includes('role="checkbox"')).every(button => button.includes("disabled="))).toBe(true);
    expect(handlers.onConfirmCreatorSaveBatch).not.toHaveBeenCalled();
  });
});
