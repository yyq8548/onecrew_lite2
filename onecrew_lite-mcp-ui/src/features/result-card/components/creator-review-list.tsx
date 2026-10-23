import { displayLabel } from "@mcp-ui/lib/display-label";
import { useCallback, useEffect, useLayoutEffect, useRef, useState } from "react";
import { ChevronRight, Ellipsis } from "lucide-react";
import { useFullscreenEscape, useHostFrame } from "@mcp-ui/app/host-frame";
import { Button } from "@mcp-ui/components/ui/button";
import { Checkbox } from "@mcp-ui/components/ui/checkbox";
import { ChannelMark } from "@mcp-ui/components/ui/platform-mark";
import { useAppLocale, type CopyKey } from "@mcp-ui/lib/i18n";
import type { CreatorSaveBatchReview, CreatorSavePreview } from "@mcp-ui/lib/result-contract";
import { creatorSaveRouteIds, creatorSaveRowSelectable, selectableCreatorRoutes } from "@mcp-ui/features/result-card/creator-save-state";
import { CardDetailView } from "./text-details";
import { CreatorCandidateEvidence } from "./creator-candidate-evidence";
import { clampPage, pageCapacity, pageForAnchor } from "../creator-review-layout";

import { CreatorScore } from "./creator-score";
export { CreatorScore } from "./creator-score";

export function CreatorIdentity({ preview, expanded = false, profileUrl, children, accountStatus }: { accountStatus?: string; children?: React.ReactNode; profileUrl?: string | null; expanded?: boolean; preview: Pick<CreatorSavePreview, "display_name" | "handle" | "platform"> & { avatar_url?: string | null } }) {
  const [imageFailed, setImageFailed] = useState(false);
  useEffect(() => setImageFailed(false), [preview.avatar_url]);
  const { t, locale } = useAppLocale();
  const name = preview.display_name ?? preview.handle ?? t("creatorUnnamed");
  return <div className="flex min-w-0 items-center gap-2.5"><span className="relative block size-9 shrink-0"><span className="grid size-full place-items-center rounded-full bg-muted text-sm font-medium" aria-hidden="true">{preview.avatar_url && preview.platform !== "instagram" && !imageFailed ? <img src={preview.avatar_url} alt="" className="size-full rounded-full object-cover" onError={() => setImageFailed(true)} /> : Array.from(name)[0]?.toUpperCase()}</span><span className="creator-platform-badge absolute -bottom-0.5 -right-0.5 inline-flex rounded-full ring-1 ring-background"><ChannelMark platform={preview.platform} compact /></span></span><span className="min-w-0"><span className="flex min-w-0 items-center gap-1.5"><span className={`text-sm font-medium ${expanded ? "[overflow-wrap:anywhere]" : "truncate"}`}>{name}</span>{profileUrl && <a href={profileUrl} target="_blank" rel="noopener noreferrer" title={t("profileLink")} aria-label={`${t("profileLink")} · ${name}`} className="inline-flex shrink-0 items-center justify-center p-1 text-muted-foreground hover:text-foreground"><span className="text-sm leading-none" aria-hidden="true">↗</span></a>}</span><span className="flex min-w-0 items-center gap-3 text-xs text-muted-foreground"><span className={expanded ? "min-w-0 [overflow-wrap:anywhere]" : "min-w-0 truncate"}>{preview.handle ? `@${preview.handle.replace(/^@/, "")}` : preview.platform}</span>{accountStatus && <span className="shrink-0 whitespace-nowrap">{accountStatus}</span>}</span>{children}</span></div>;
}

function ChannelIcon({ channel }: { channel: string }) {
  const { t, locale } = useAppLocale();
  return <ChannelMark platform={channel} compact label={channel === "email" ? t("mailChannel") : undefined} />;
}

function Followers({ preview, withLabel = false }: { preview: CreatorSavePreview; withLabel?: boolean }) {
  const { t, locale } = useAppLocale();
  const entry = preview.follower_counts[preview.platform] != null ? [preview.platform, preview.follower_counts[preview.platform]] as const : Object.entries(preview.follower_counts)[0];
  return <span className="min-w-0 truncate text-xs tabular-nums text-muted-foreground" aria-label={entry ? `${t("creatorFollowers")}: ${new Intl.NumberFormat(locale).format(entry[1]!)}` : t("creatorNoFollowers")}>{entry ? new Intl.NumberFormat(locale, { notation: "compact", maximumFractionDigits: 1 }).format(entry[1]!) : "—"}{withLabel ? ` ${t("creatorFollowers")}` : ""}{Object.keys(preview.follower_counts).length > 1 ? ` +${Object.keys(preview.follower_counts).length - 1}` : ""}</span>;
}

const CREATOR_STATUS_KEYS: Record<CreatorSavePreview["status"], CopyKey> = {
  eligible_new: "creatorStatusEligibleNew",
  eligible_update: "creatorStatusEligibleUpdate",
  existing_skipped: "creatorStatusExistingSkipped",
  duplicate_skipped: "creatorStatusDuplicateSkipped",
  identity_pending: "creatorStatusIdentityPending",
  contact_conflict: "creatorStatusContactConflict",
  failed: "creatorStatusFailed",
};

const CREATOR_REASON_KEYS: Record<Exclude<CreatorSavePreview["reason"], null>, CopyKey> = {
  existing_creator: "creatorSaveReasonExistingCreator",
  duplicate_selection: "creatorSaveReasonDuplicateSelection",
  identity_ambiguous: "creatorSaveReasonIdentityAmbiguous",
  conflicting_update: "creatorSaveReasonConflictingUpdate",
  required_contact_route_conflict: "creatorSaveReasonRequiredContactRouteConflict",
  preparation_failed: "creatorSaveReasonPreparationFailed",
};

function creatorReviewNotice(preview: CreatorSavePreview, t: ReturnType<typeof useAppLocale>["t"]): string | null {
  if (preview.status === "eligible_new") return null;
  const status = t(CREATOR_STATUS_KEYS[preview.status]);
  if (preview.reason == null) return status;
  return t("creatorReviewStatusReason", { status, reason: t(CREATOR_REASON_KEYS[preview.reason]) });
}

function creatorReviewStatus(preview: CreatorSavePreview, t: ReturnType<typeof useAppLocale>["t"]): string {
  return t(CREATOR_STATUS_KEYS[preview.status]);
}

function publicEvidenceUrl(value: string | null | undefined): string | null {
  if (!value) return null;
  try {
    const url = new URL(value);
    return (url.protocol === "https:" || url.protocol === "http:") && !url.username && !url.password
      ? url.href
      : null;
  } catch {
    return null;
  }
}

function hasCreatorEvidence(preview: CreatorSavePreview): boolean {
  return publicEvidenceUrl(preview.profile_url) !== null
    || publicEvidenceUrl(preview.source_url) !== null
    || (preview.assessment?.trim().length ?? 0) > 0;
}

/** Out-of-flow row copies let capacity account for the tallest warning row on
 * every page without putting all 50 rows into the card's normal height. */
function CreatorReviewMeasureRow({ preview, selected, optional, notice }: {
  preview: CreatorSavePreview; selected: boolean; optional: string[]; notice: string | null;
}) {
  const { t } = useAppLocale();
  const name = preview.display_name ?? preview.handle ?? t("creatorUnnamed");
  const visibleRoutes = selectableCreatorRoutes(preview);
  const routes = creatorSaveRouteIds(preview, optional);
  const channels = selected ? [...new Set(visibleRoutes.filter(route => routes.includes(route.route_id)).map(route => route.channel))] : [];
  const profileUrl = publicEvidenceUrl(preview.profile_url);
  return <li aria-hidden="true" data-creator-measure-row="true">
    <div className="grid grid-cols-[20px_minmax(0,1fr)_88px_76px_76px_16px] items-center gap-3 px-2 py-3">
      <span className="size-4" />
      <div className="min-w-0 text-left"><CreatorIdentity preview={preview} profileUrl={profileUrl} accountStatus={creatorReviewStatus(preview, t)} /></div>
      <div className="channel-stack flex items-center">{channels.slice(0, 3).map(channel => <ChannelIcon key={channel} channel={channel} />)}</div>
      <Followers preview={preview} />
      <span className="h-4" />
      <span className="size-4" />
      {notice && <p className="col-start-2 col-end-7 text-xs [overflow-wrap:anywhere]">{notice}</p>}
    </div>
  </li>;
}

type CreatorListItem =
  | { kind: "review"; row: { row_id: string; preview: CreatorSavePreview } }
  | { kind: "failure"; failure: CreatorSaveBatchReview["failures"][number] };

function creatorListItemKey(item: CreatorListItem): string {
  return item.kind === "review" ? `review:${item.row.row_id}` : `failure:${item.failure.row_id}`;
}

/** A typed preparation failure has no route/selection authority. Keep its
 * owner-provided safe metadata in the same paged list, and never synthesize a
 * preview or profile when those fields are absent. */
function CreatorFailureRow({ failure, t, measurement = false }: {
  failure: CreatorSaveBatchReview["failures"][number];
  t: ReturnType<typeof useAppLocale>["t"];
  measurement?: boolean;
}) {
  const profileUrl = publicEvidenceUrl(failure.profile_url);
  const attributes = measurement
    ? { "data-creator-measure-row": "true" }
    : { "data-creator-failure-row": failure.row_id };
  return <li aria-hidden={measurement || undefined} {...attributes} className="grid gap-1 px-3 py-3">
    <CreatorIdentity preview={failure} profileUrl={profileUrl} accountStatus={t("creatorSavePreparedFailure")} />
    <p className="text-xs text-muted-foreground">{t("creatorSaveFailureRowId", { rowId: failure.row_id })} · {t("creatorSaveReasonPreparationFailed")}</p>
  </li>;
}

/** How many rows fit on one page of the frame the Host described.
 *
 * Every term is measured or stated rather than assumed: the Host states the
 * room, and the list reads where it actually begins, how tall a rendered row
 * actually is, and how much the action bar actually occupies. A hand-picked
 * page size is a guess about someone else's screen — too many rows on a phone,
 * needlessly few on a desktop — and dividing a guessed page into guessed
 * chrome would only have buried the guess one level down. */
function useRowsPerPage(): [number | null, (node: HTMLUListElement | null) => void, (node: HTMLUListElement | null) => void] {
  const { availableHeight, displayMode, safeArea } = useHostFrame();
  const [list, setList] = useState<HTMLUListElement | null>(null);
  const [probe, setProbe] = useState<HTMLUListElement | null>(null);
  const [rowsPerPage, setRowsPerPage] = useState<number | null>(null);

  useLayoutEffect(() => {
    if (!list) {
      setRowsPerPage(null);
      return;
    }
    const measure = () => {
      // The probe contains every row outside normal flow. Measuring it avoids
      // changing capacity when paging happens to reveal a taller warning row.
      const source = probe ?? list;
      const row = Math.max(0, ...Array.from(source.children, (child) => child.getBoundingClientRect().height));
      if (row <= 0) return;
      const shell = list.closest(".result-shell");
      const shellRect = shell?.getBoundingClientRect();
      const content = list.closest<HTMLElement>("[data-result-scroll]");
      const contentRect = content?.getBoundingClientRect();
      // Fullscreen owns a real viewport, so a mode-only grant must use the
      // actual shell box instead of a stale inline containerDimensions value.
      // Inline has no scrollport of its own and therefore uses only the room
      // explicitly stated by the Host.
      const frameHeight = displayMode === "fullscreen"
        ? shellRect?.height
        : availableHeight;
      if (frameHeight == null || !Number.isFinite(frameHeight) || frameHeight <= 0) {
        setRowsPerPage(null);
        return;
      }
      const actions = shell?.querySelector("[data-result-actions]")?.getBoundingClientRect().height ?? 0;
      const pager = content?.querySelector<HTMLElement>("[data-creator-list-pages]");
      const listRect = list.getBoundingClientRect();
      const pagerRect = pager?.getBoundingClientRect();
      const pagerHeight = pagerRect?.height ?? 0;
      const listToPagerGap = pagerRect ? Math.max(0, pagerRect.top - listRect.bottom) : 0;
      // Only CSS padding is part of the tail. The old contentRect-bottom
      // subtraction included flex's unused remainder, making capacity depend
      // on how many rows happened to be visible and eventually collapse to 1.
      const contentBottomPadding = content && typeof getComputedStyle === "function"
        ? Number.parseFloat(getComputedStyle(content).paddingBottom) || 0
        : 0;
      const listTop = displayMode === "fullscreen" && shellRect && contentRect
        ? contentRect.top - shellRect.top
        : shellRect
          ? listRect.top - shellRect.top
          : listRect.top;
      const capacity = pageCapacity({ availableHeight: frameHeight, listTop, actionHeight: actions, rowHeight: row, pagerHeight, listToPagerGap, contentBottomPadding, safeArea });
      setRowsPerPage(current => current === capacity ? current : capacity);
    };
    measure();
    if (typeof ResizeObserver === "undefined") return;
    const observer = new ResizeObserver(measure);
    observer.observe(list);
    if (probe) observer.observe(probe);
    const shell = list.closest(".result-shell");
    if (shell) observer.observe(shell);
    const header = shell?.querySelector<HTMLElement>('[data-slot="card-header"]');
    if (header) observer.observe(header);
    const actions = shell?.querySelector<HTMLElement>("[data-result-actions]");
    if (actions) observer.observe(actions);
    const content = list.closest<HTMLElement>("[data-result-scroll]");
    if (content) observer.observe(content);
    const pager = content?.querySelector<HTMLElement>("[data-creator-list-pages]");
    if (pager) observer.observe(pager);
    return () => observer.disconnect();
  }, [list, probe, availableHeight, displayMode, safeArea]);

  return [
    rowsPerPage,
    useCallback((node: HTMLUListElement | null) => setList(node), []),
    useCallback((node: HTMLUListElement | null) => setProbe(node), []),
  ];
}

// Fifty rows rendered at once did not become a long card — the Host caps inline
// height and clipped them, taking the confirm button with them. Paging is the
// bound, and it bounds reading only: confirm stays in the card's action bar,
// works on any page, and acts on every selected row, shown or not.
export function CreatorReviewList({ rows, failures, selectedIds, optionalIds, onSelect, onRoute, disabled }: {
  rows: { row_id: string; preview: CreatorSavePreview }[]; selectedIds: string[]; optionalIds: Record<string, string[]>;
  failures?: CreatorSaveBatchReview["failures"];
  onSelect: (rowId: string, checked: boolean) => void; onRoute: (rowId: string, routeId: string, checked: boolean) => void;
  disabled: boolean;
}) {
  const { t, locale } = useAppLocale();
  const [detailRowId, setDetailRowId] = useState<string | null>(null);
  const [page, setPage] = useState(0);
  const detailOpener = useRef<HTMLButtonElement | null>(null);
  const detailOpenerRowId = useRef<string | null>(null);
  const detailRoot = useRef<HTMLDivElement | null>(null);
  const pageAnchor = useRef<string | null>(null);
  const [measured, listRef, probeRef] = useRowsPerPage();
  const requestFullscreen = useFullscreenEscape();
  const items: CreatorListItem[] = [
    ...rows.map(row => ({ kind: "review" as const, row })),
    ...(failures ?? []).map(failure => ({ kind: "failure" as const, failure })),
  ];
  // Without a stated inline budget there is no honest page-size claim to make;
  // let the Host auto-resize the intrinsic card rather than inventing a row
  // count. A real fullscreen box or Host-stated inline height replaces this
  // only after the DOM measurement runs.
  const perPage = measured ?? Math.max(1, items.length);
  const rowIds = items.map(creatorListItemKey);
  const pageCount = Math.max(1, Math.ceil(items.length / perPage));
  const current = clampPage(page, pageCount);

  // A true resize changes capacity; preserve the first row the reader was
  // looking at and clamp exactly once if the list became shorter. This keeps a
  // stale page number from bouncing between renders.
  useEffect(() => {
    const anchorIndex = pageAnchor.current == null ? -1 : rowIds.indexOf(pageAnchor.current);
    if (anchorIndex < 0) {
      const fallbackIndex = Math.min(current * perPage, Math.max(0, rowIds.length - 1));
      pageAnchor.current = rowIds[fallbackIndex] ?? null;
    }
    const target = pageForAnchor(rowIds, pageAnchor.current, perPage);
    if (target !== current) setPage(target);
  }, [current, perPage, pageCount, rowIds.join("|")]);

  const goToPage = (next: number) => {
    const target = clampPage(next, pageCount);
    pageAnchor.current = rowIds[target * perPage] ?? null;
    setPage(target);
  };
  // A row's details replace the list instead of unfolding inside it. This Host
  // keeps its own frame height, so an unfolding row pushed the rows beneath it
  // — and the card's action bar — past the frame's edge. The swap bounds
  // reading only: the selection is untouched by what is displayed, and confirm
  // still acts on every selected row from the card's own action bar.
  const detailRow = detailRowId == null ? undefined : rows.find(row => row.row_id === detailRowId);
  const visible = items.slice(current * perPage, (current + 1) * perPage);
  return <div ref={detailRoot} data-creator-review-root="true" className="grid min-w-0 gap-3">
  {detailRow ? <CreatorRowDetail
    preview={detailRow.preview}
    selected={selectedIds.includes(detailRow.row_id)}
    optional={Object.hasOwn(optionalIds, detailRow.row_id) ? optionalIds[detailRow.row_id]! : []}
    disabled={disabled}
    onRoute={(routeId, checked) => onRoute(detailRow.row_id, routeId, checked)}
    onClose={() => {
      const openerRowId = detailOpenerRowId.current;
      setDetailRowId(null);
      if (typeof window !== "undefined") window.requestAnimationFrame(() => {
        const trigger = openerRowId == null
          ? null
          : [...(detailRoot.current?.querySelectorAll<HTMLButtonElement>("[data-creator-detail-trigger]") ?? [])]
            .find((element) => element.dataset.creatorDetailTrigger === openerRowId);
        trigger?.focus();
      });
    }}
  /> : <><div data-creator-list-scroll="true" className="min-w-0 overflow-x-auto"><div className="relative min-w-[640px]">
    {<div className="sticky top-0 z-10 grid grid-cols-[20px_minmax(0,1fr)_88px_76px_76px_16px] items-center gap-3 border-b bg-card px-2 py-2 text-xs text-muted-foreground"><span /><span>{t("creatorName")}</span><span>{t("creatorContactMethods")}</span><span>{t("creatorFollowers")}</span><span><CreatorScore score={null} labelOnly /></span><span /></div>}
    <ul ref={listRef} className="divide-y rounded-lg border">{visible.map(item => item.kind === "failure" ? <CreatorFailureRow key={creatorListItemKey(item)} failure={item.failure} t={t} /> : (() => {
      const { row_id, preview } = item.row;
      const name = preview.display_name ?? preview.handle ?? t("creatorUnnamed");
      const visibleRoutes = selectableCreatorRoutes(preview);
      const routes = creatorSaveRouteIds(preview, Object.hasOwn(optionalIds, row_id) ? optionalIds[row_id]! : []);
      const selected = selectedIds.includes(row_id);
      const channels = selected ? [...new Set(visibleRoutes.filter(route=>routes.includes(route.route_id)).map(route=>route.channel))] : [];
      const mainPlatform = preview.follower_counts[preview.platform] != null ? preview.platform : Object.keys(preview.follower_counts)[0];
      const moreFollowers = Object.entries(preview.follower_counts).filter(([platform])=>platform !== mainPlatform);
      const profileUrl = publicEvidenceUrl(preview.profile_url);
      const reviewNotice = creatorReviewNotice(preview, t);
      return <li key={row_id} data-creator-row={row_id}>
        <div className="grid grid-cols-[20px_minmax(0,1fr)_88px_76px_76px_16px] items-center gap-3 px-2 py-3">
          <Checkbox className="rounded-sm" aria-label={t("creatorSelectRow", { name })} disabled={disabled || !creatorSaveRowSelectable(preview)} checked={selected} onCheckedChange={checked => onSelect(row_id, checked === true)} />
          <div className="min-w-0 text-left"><CreatorIdentity preview={preview} profileUrl={profileUrl} accountStatus={creatorReviewStatus(preview, t)} /></div>
          {<div className="channel-stack flex items-center">{channels.length === 0 && <span className="text-xs text-muted-foreground">—</span>}{channels.slice(0, 3).map(channel => <ChannelIcon key={channel} channel={channel} />)}{channels.length > 3 && <span className="channel-overflow text-xs" title={channels.slice(3).map(channel=>displayLabel(channel,locale)).join(", ")} aria-label={channels.slice(3).map(channel=>displayLabel(channel,locale)).join(", ")}><Ellipsis aria-hidden="true" className="size-3 shrink-0" /></span>}</div>}
          {<Followers preview={preview} />}
          <span data-score-value="true"><CreatorScore valueOnly score={preview.score} subjectId={row_id} assessment={preview.assessment} /></span>
          {(moreFollowers.length > 0 || visibleRoutes.length > 0 || hasCreatorEvidence(preview)) ? <button data-creator-detail-trigger={row_id} type="button" className="grid size-4 place-items-center" aria-label={t("creatorExpandRow", { name })} onClick={(event) => { detailOpener.current = event.currentTarget; detailOpenerRowId.current = row_id; setDetailRowId(row_id); }}><ChevronRight aria-hidden="true" className="size-4" /></button> : <span />}
          {reviewNotice && <p data-review-warning="true" role="alert" className="col-start-2 col-end-7 text-xs text-warning-foreground [overflow-wrap:anywhere]">{reviewNotice}</p>}
        </div>
      </li>;
    })())}</ul>
    <ul ref={probeRef} aria-hidden="true" data-creator-row-probe="true" className="pointer-events-none invisible absolute left-0 top-0 w-full divide-y rounded-lg border">
      {items.map(item => item.kind === "failure"
        ? <CreatorFailureRow key={creatorListItemKey(item)} failure={item.failure} t={t} measurement />
        : <CreatorReviewMeasureRow key={creatorListItemKey(item)} preview={item.row.preview} selected={selectedIds.includes(item.row.row_id)} optional={Object.hasOwn(optionalIds, item.row.row_id) ? optionalIds[item.row.row_id]! : []} notice={creatorReviewNotice(item.row.preview, t)} />)}
    </ul>
  </div>
  </div>
  {(pageCount > 1 || requestFullscreen) && <div data-creator-list-pages="true" className="flex flex-wrap items-center justify-between gap-2">
    <Button type="button" variant="outline" size="sm" disabled={current === 0} onClick={() => goToPage(current - 1)}>{t("previousPage")}</Button>
    <span role="status" className="text-xs tabular-nums text-muted-foreground">{t("page", { current: current + 1, total: pageCount })}</span>
    <span className="flex flex-wrap gap-2">
      <Button type="button" variant="outline" size="sm" disabled={current + 1 === pageCount} onClick={() => goToPage(current + 1)}>{t("nextPage")}</Button>
      {/* Fullscreen states a bigger height, so the same measurement yields a
          bigger page there — fewer pages to move through, same mechanism. */}
      {requestFullscreen && <Button type="button" variant="ghost" size="sm" onClick={requestFullscreen}>{t("listReviewFullscreen", { total: items.length })}</Button>}
    </span>
  </div>}</>}</div>;
}

/** One creator's contact routes and remaining follower counts, shown instead of
 * the list rather than under the row.
 *
 * It is rendered from the current selection on every render, so a route toggled
 * here updates in place; the checkboxes remain the same mutations the row
 * carried, gated by the same rules. */
export function CreatorRowDetail({ preview, selected, optional, disabled, onRoute, onClose }: {
  preview: CreatorSavePreview; selected: boolean; optional: string[]; disabled: boolean;
  onRoute: (routeId: string, checked: boolean) => void; onClose: () => void;
}) {
  const { t, locale } = useAppLocale();
  const backRef = useRef<HTMLButtonElement | null>(null);
  useLayoutEffect(() => { backRef.current?.focus(); }, []);
  const name = preview.display_name ?? preview.handle ?? t("creatorUnnamed");
  const visibleRoutes = selectableCreatorRoutes(preview);
  const routes = creatorSaveRouteIds(preview, optional);
  const mainPlatform = preview.follower_counts[preview.platform] != null ? preview.platform : Object.keys(preview.follower_counts)[0];
  const moreFollowers = Object.entries(preview.follower_counts).filter(([platform]) => platform !== mainPlatform);
  return <CardDetailView title={name} onClose={onClose} backRef={backRef}>
    <div className="creator-supplement">
      {visibleRoutes.length > 0 && <section className="grid gap-2" aria-label={t("creatorContactMethods")}>{visibleRoutes.map(route => <label key={route.route_id} className="flex items-center gap-2 text-sm"><Checkbox aria-label={route.display_value} checked={routes.includes(route.route_id)} disabled={disabled || !selected || route.required} onCheckedChange={checked => onRoute(route.route_id, checked === true)} /><ChannelIcon channel={route.channel} /><span className="[overflow-wrap:anywhere]">{route.display_value}</span><span className="text-xs text-muted-foreground">{t(route.required ? "creatorRouteRequired" : "creatorRouteOptional")}</span></label>)}</section>}
      {moreFollowers.length > 0 && <section className="flex flex-wrap gap-x-6 gap-y-2" aria-label={t("creatorFollowers")}>{moreFollowers.map(([platform, count]) => <span key={platform} className="inline-flex items-center gap-2 text-sm"><ChannelIcon channel={platform} /><span>{displayLabel(platform, locale)} · {t("creatorFollowers")} {new Intl.NumberFormat(locale).format(count)}</span></span>)}</section>}
      {hasCreatorEvidence(preview) && <CreatorCandidateEvidence preview={preview} />}
    </div>
  </CardDetailView>;
}
