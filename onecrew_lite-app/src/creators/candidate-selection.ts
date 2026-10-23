import { creatorSaveSelection, type CreatorSaveSelection } from "../results/creator-save-contracts.ts";
import { candidateJson, type CandidateIntent, type CanonicalCandidate } from "./candidate-contracts.ts";
import type { CandidatePreparedRow, CandidateRoutePreview } from "./candidate-prepare.ts";

export interface CandidateContactConfirmation { preview: CandidateRoutePreview; confidence: "exact" | "asserted" }
export interface SelectedCandidateRow { input: CanonicalCandidate; confirmations: CandidateContactConfirmation[] }
export interface SelectedCandidateBatch { rows: SelectedCandidateRow[]; intent: CandidateIntent }
export class CandidateSelectionError extends Error {
  readonly code: "creator_save_batch_selection" | "creator_save_duplicate_row" | "creator_save_unknown_row" | "creator_save_routes" | "creator_save_row_not_eligible";
  constructor(code: CandidateSelectionError["code"], message: string) { super(message); this.code = code; }
}
export function selectCandidateRoutes(rows: readonly CandidatePreparedRow[], intent: CandidateIntent, raw: readonly CreatorSaveSelection[]): SelectedCandidateBatch {
  if (!raw.length || raw.length > rows.length) throw new CandidateSelectionError("creator_save_batch_selection", "Select a nonempty subset of prepared Creator rows");
  const selections = raw.map(value => creatorSaveSelection.parse(value)), selected = new Map<string, SelectedCandidateRow>();
  for (const selection of selections) {
    if (selected.has(selection.row_id)) throw new CandidateSelectionError("creator_save_duplicate_row", "Creator save row was selected more than once");
    const row = rows.find(row => row.input.row_id === selection.row_id);
    if (!row) throw new CandidateSelectionError("creator_save_unknown_row", "Creator save row is not part of this review");
    if (!["eligible_new", "eligible_update"].includes(row.preview.status)) throw new CandidateSelectionError("creator_save_row_not_eligible", "Creator save row is not eligible for confirmation");
    const ids = new Set(selection.confirmed_route_ids);
    if (ids.size !== selection.confirmed_route_ids.length || row.preview.route_options.some(option => option.required && !ids.has(option.route_id))) throw new CandidateSelectionError("creator_save_routes", "Confirm each required contact route exactly once");
    for (const id of ids) if (row.required_parent_routes[id]?.some(parent => !ids.has(parent))) throw new CandidateSelectionError("creator_save_routes", "Confirm the parent LinkHub route for each derived contact");
    const confirmations: CandidateContactConfirmation[] = [];
    for (const id of selection.confirmed_route_ids) {
      const route = row.routes.find(([routeId]) => routeId === id), option = row.preview.route_options.find(option => option.route_id === id);
      if (!route || !option) throw new CandidateSelectionError("creator_save_routes", "Contact route is not part of this Creator review");
      confirmations.push({ preview: structuredClone(route[1]), confidence: option.required ? "exact" : "asserted" });
    }
    selected.set(selection.row_id, { input: structuredClone(row.input), confirmations });
  }
  return { rows: rows.filter(row => selected.has(row.input.row_id)).map(row => selected.get(row.input.row_id)!), intent };
}
const collisionPayload = (collision: CandidateRoutePreview["collision"]) => collision.status === "none" ? { status: "none" } : { status: collision.status, creator_ids: collision.creator_ids };
const routePayload = (route: CandidateRoutePreview) => ({ locator: route.locator, observations: route.observations, collision: collisionPayload(route.collision) });
export function candidateSelectedPayload(batch: SelectedCandidateBatch) { return { rows: batch.rows.map(row => ({ input: row.input, confirmations: row.confirmations.map(confirmation => ({ preview: routePayload(confirmation.preview), confidence: confirmation.confidence })) })), intent: batch.intent }; }
export function candidatePreparedPayload(rows: readonly CandidatePreparedRow[], intent: CandidateIntent): Buffer {
  return Buffer.from(candidateJson([intent, rows.map(row => {
    const p = row.preview;
    return { input: row.input, preview: { row_id: p.row_id, intent: p.intent, status: p.status, reason: p.reason, account: p.account, profile: p.profile, profile_observed_at: p.profile_observed_at, provenance: p.provenance, notes: p.notes, tags: p.tags, quality: p.quality, identity_resolution: p.identity_resolution, route_options: p.route_options.map(route => ({ ...route, collision: collisionPayload(route.collision) })) }, routes: row.routes.map(([id, route]) => [id, routePayload(route)]), required_parent_routes: Object.fromEntries(Object.entries(row.required_parent_routes).sort(([a], [b]) => a < b ? -1 : a > b ? 1 : 0)) };
  })]));
}
