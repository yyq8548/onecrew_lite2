export type VerticalSafeArea = {
  top: number;
  right: number;
  bottom: number;
  left: number;
};

export type PageCapacityInput = {
  availableHeight: number | null;
  listTop: number;
  actionHeight: number;
  rowHeight: number;
  pagerHeight?: number;
  listToPagerGap?: number;
  contentBottomPadding?: number;
  safeArea: VerticalSafeArea | null;
};

/**
 * Return the number of complete rows that fit between the list and the action
 * bar. The Host's safe-area budget is deducted here exactly once; callers must
 * not also turn it into padding before passing their measured list top.
 */
export function pageCapacity({ availableHeight, listTop, actionHeight, rowHeight, pagerHeight = 0, listToPagerGap = 0, contentBottomPadding = 0, safeArea }: PageCapacityInput): number | null {
  if (availableHeight == null || !Number.isFinite(availableHeight) || rowHeight <= 0 || !Number.isFinite(rowHeight)) return null;
  const insets = verticalSafeAreaBudget(safeArea);
  const tail = positive(pagerHeight) + positive(listToPagerGap) + positive(contentBottomPadding);
  const room = availableHeight - positive(listTop) - positive(actionHeight) - tail - insets;
  return Math.max(1, Math.floor(room / rowHeight));
}

/** Vertical Host insets are consumed once by a frame budget. */
export function verticalSafeAreaBudget(safeArea: VerticalSafeArea | null): number {
  return safeArea == null ? 0 : positive(safeArea.top) + positive(safeArea.bottom);
}

/** Keep a stable row visible when a true resize changes page capacity. */
export function pageForAnchor(rowIds: readonly string[], anchorId: string | null, pageSize: number): number {
  if (pageSize <= 0 || rowIds.length === 0) return 0;
  const index = anchorId == null ? 0 : Math.max(0, rowIds.indexOf(anchorId));
  return Math.floor(index / pageSize);
}

export function clampPage(page: number, pageCount: number): number {
  return Math.min(Math.max(0, page), Math.max(1, pageCount) - 1);
}

function positive(value: number): number {
  return Number.isFinite(value) && value > 0 ? value : 0;
}
