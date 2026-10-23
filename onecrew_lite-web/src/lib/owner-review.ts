// Shared Web projection of the runtime result owner; no client-authored authority.
import { wireResultSnapshot } from "@onecrew/runtime-contracts/result-wire";
export type OwnerResultBinding = {
  result_ref: string;
  view: { identity: string; kind: "read_only" | "confirmation" | "choice" };
  fingerprint: string;
  revision: number | bigint;
  origin: string;
};
export type CreatorSaveRoute = {
  route_id: string; channel: string; kind: string; display_value: string; normalized_value: string;
  sources: string[]; required: boolean; collision: { status: "none" | "possible" | "conflict"; creator_ids: string[] };
};
export type CreatorSavePreviewRow = {
  row_id: string;
  preview: {
    display_name: string | null; platform: string; handle: string | null;
    profile_url: string | null; avatar_url: string | null; identity_status: string;
    identity_creator_ids: string[]; notes: string | null; score: number | null;
    assessment: string | null; follower_counts: Record<string, number | bigint>;
    routes: CreatorSaveRoute[]; expires_at: number | bigint;
  };
};
export type CreatorSaveRowOutcome = {
  row_id: string; status: "saved" | "failed" | "unknown" | "not_run" | "not_selected";
  creator_id: string | null; failure_code: string | null;
};
/** Fields consumed from the shared runtime ResultSnapshot, without a second prepared model. */
export type OwnerReviewSnapshot = {
  binding: OwnerResultBinding;
  completed_sections_count: number;
  pending_action: { kind: "creator_save_batch"; review: { rows: CreatorSavePreviewRow[] } } | { kind: "confirm"; prompt: string; label: string } | null;
  state: { kind: "available" | "submitted" } | { kind: "resolved" | "failed" | "unknown"; code: string; summary: string };
  presentation: { title: string; summary: string | null; sections: (
    | { kind: "creator_save_rows"; rows: CreatorSaveRowOutcome[] }
    | { kind: "fields"; heading: string | null; fields: { label: string; value: string }[] }
    | { kind: "items"; heading: string | null; items: { title: string; description: string | null }[] }
    | { kind: "notice"; heading: string | null; body: string; tone: "neutral" | "success" | "warning" | "critical" }
  )[] };
};
export type CreatorSaveRouteSelection = { row_id: string; confirmed_route_ids: string[] };

export function parseOwnerReviewSnapshot(value: unknown): OwnerReviewSnapshot {
  const snapshot = wireResultSnapshot.parse(value);
  const pending = snapshot.pending_action;
  if (pending !== null && pending.kind !== "creator_save_batch" && pending.kind !== "confirm") throw new Error("unexpected_creator_save_review");
  const sections = snapshot.presentation.sections.filter(section => section.kind === "creator_save_rows" || section.kind === "fields" || section.kind === "items" || section.kind === "notice");
  if (sections.length !== snapshot.presentation.sections.length) throw new Error("unexpected_creator_save_presentation");
  return { ...snapshot, pending_action: pending, presentation: { ...snapshot.presentation, sections } };
}

/** Authority is supplied by the authenticated service session on every request. */
export function webResultBinding(binding: OwnerResultBinding) {
  return { result_ref: binding.result_ref, view: binding.view, fingerprint: binding.fingerprint, revision: binding.revision };
}
