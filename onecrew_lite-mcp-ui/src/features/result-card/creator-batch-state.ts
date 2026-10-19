import type { CreatorSaveBatchReview } from "@mcp-ui/lib/result-contract";
import { creatorSaveRouteIds, toggleCreatorSaveRoute, defaultCreatorRouteIds, creatorSaveRowSelectable } from "./creator-save-state";

export type CreatorReviewRow = CreatorSaveBatchReview["rows"][number];
export type CreatorBatchDraft = { selectedIds: string[]; optionalIds: Record<string, string[]> };
export function initialCreatorBatchDraft(rows: CreatorReviewRow[]): CreatorBatchDraft {
  return { selectedIds: rows.filter(row => creatorSaveRowSelectable(row.preview)).map(row => row.row_id), optionalIds: Object.fromEntries(rows.map(row => [row.row_id, defaultCreatorRouteIds(row.preview)])) };
}
export function setCreatorBatchSelected(draft: CreatorBatchDraft, rowId: string, checked: boolean, rows: CreatorReviewRow[]): CreatorBatchDraft {
  const row = rows.find(candidate => candidate.row_id === rowId);
  if (!row || !creatorSaveRowSelectable(row.preview)) return draft;
  const selected = checked ? [...draft.selectedIds, rowId] : draft.selectedIds.filter(id => id !== rowId);
  return { ...draft, selectedIds: rows.filter(row => selected.includes(row.row_id)).map(row => row.row_id) };
}
export function setCreatorBatchRoute(draft: CreatorBatchDraft, rowId: string, routeId: string, checked: boolean, rows: CreatorReviewRow[]): CreatorBatchDraft {
  const route = rows.find(row => row.row_id === rowId)?.preview.routes.find(route => route.route_id === routeId);
  if (!route || !draft.selectedIds.includes(rowId)) return draft;
  return { ...draft, optionalIds: { ...draft.optionalIds, [rowId]: toggleCreatorSaveRoute(Object.hasOwn(draft.optionalIds, rowId) ? draft.optionalIds[rowId]! : [], route, checked) } };
}
export function creatorBatchSelections(rows: CreatorReviewRow[], draft: CreatorBatchDraft) {
  return rows.filter(row => draft.selectedIds.includes(row.row_id) && creatorSaveRowSelectable(row.preview)).map(row => ({ row_id: row.row_id, confirmed_route_ids: creatorSaveRouteIds(row.preview, Object.hasOwn(draft.optionalIds, row.row_id) ? draft.optionalIds[row.row_id]! : []) }));
}
export function rebaseCreatorBatchDraft(rows: CreatorReviewRow[], draft: CreatorBatchDraft): CreatorBatchDraft {
  return {
    selectedIds: rows.filter(row => draft.selectedIds.includes(row.row_id) && creatorSaveRowSelectable(row.preview)).map(row => row.row_id),
    optionalIds: Object.fromEntries(rows.map(row => [row.row_id, (Object.hasOwn(draft.optionalIds, row.row_id) ? draft.optionalIds[row.row_id]! : []).filter(id => row.preview.routes.some(route => route.route_id === id && !route.required))])),
  };
}
