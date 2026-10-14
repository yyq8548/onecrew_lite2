import { createContext, useCallback, useContext, useEffect, useLayoutEffect, useRef, useState, type ReactNode } from "react";
import { Eye } from "lucide-react";
import { Button } from "@mcp-ui/components/ui/button";
import { useHostFrame } from "@mcp-ui/app/host-frame";
import { verticalSafeAreaBudget } from "../creator-review-layout";
import { useAppLocale } from "@mcp-ui/lib/i18n";
import { SearchPlanTextDetails } from "@mcp-ui/features/result-card/components/search-plan-details";

/** What a card shows instead of its body.
 *
 * Growing is the one move this Host punishes: it ignores
 * `ui/notifications/size-changed` and keeps the frame height it already chose,
 * so a disclosure that adds height pushes the newly revealed content — and the
 * action bar beneath it — outside the frame. A disclosure therefore replaces
 * the body rather than extending it, and carries its own way back.
 *
 * `text` keeps the pager long text always had. `view` carries arbitrary
 * content, so a card can swap in a panel, a table, or a row's details without
 * each site inventing a second disclosure of its own. */
export type CardDetail =
  | { kind: "text"; title: string; text: string; page: number }
  | { kind: "view"; title: string; content: ReactNode };

type CardDetails = {
  detail: CardDetail | null;
  /** Read long text in the detail view, paged as it always was. */
  openText: (title: string, text: string) => void;
  /** Read arbitrary content in the detail view. */
  openView: (title: string, content: ReactNode) => void;
  close: () => void;
  page: (page: number) => void;
};

const TextDetailsContext = createContext<CardDetails | null>(null);

export function ResultTextDetailsProvider({ revision, children }: { revision: number; children: ReactNode }) {
  const [detail, setDetail] = useState<CardDetail | null>(null);
  useEffect(() => { setDetail(null); }, [revision]);
  return <TextDetailsContext.Provider value={{
    detail,
    openText: (title, text) => setDetail({ kind: "text", title, text, page: 0 }),
    openView: (title, content) => setDetail({ kind: "view", title, content }),
    close: () => setDetail(null),
    page: (page) => setDetail((value) => value && value.kind === "text" ? { ...value, page } : value),
  }}>{children}</TextDetailsContext.Provider>;
}

export function useCardDetails() {
  const context = useContext(TextDetailsContext);
  if (!context) throw new Error("Card details require a ResultShell");
  return context;
}

/** Rows on a detail view when the Host stated no height at all.
 *
 * A conservative constant, and the only number here that was not measured: it
 * applies only when the Host described no frame, where guessing large would
 * put the way back outside a frame this App cannot see. */
const UNSTATED_DETAIL_HEIGHT = 320;

/** How much room a swapped-in detail actually has.
 *
 * Every term is stated or measured rather than assumed: the Host states the
 * frame height, and the region reads where it actually begins, how much the
 * card's action bar occupies, and how much its own way back occupies. A
 * hand-picked pixel height is a guess about someone else's screen. */
function useDetailBound(): [(node: HTMLDivElement | null) => void, number] {
  const { availableHeight, displayMode, safeArea } = useHostFrame();
  const [region, setRegion] = useState<HTMLDivElement | null>(null);
  const [stated, setStated] = useState<number | null>(null);

  useEffect(() => {
    if (!region || displayMode !== "fullscreen" || availableHeight == null) {
      setStated(null);
      return;
    }
    const shell = region.closest(".result-shell");
    const measure = () => {
      const actions = shell?.querySelector("[data-result-actions]")?.getBoundingClientRect().height ?? 0;
      const back = region.parentElement?.querySelector("[data-card-detail-back]")?.getBoundingClientRect().height ?? 0;
      const shellTop = shell?.getBoundingClientRect().top ?? 0;
      const regionTop = region.getBoundingClientRect().top - shellTop;
      const room = availableHeight - regionTop - actions - back - verticalSafeAreaBudget(safeArea);
      setStated(room > 0 ? Math.floor(room) : null);
    };
    measure();
    if (typeof ResizeObserver === "undefined") return;
    const observer = new ResizeObserver(measure);
    observer.observe(shell ?? region);
    return () => observer.disconnect();
  }, [region, availableHeight, displayMode, safeArea]);

  return [useCallback((node: HTMLDivElement | null) => setRegion(node), []), stated ?? UNSTATED_DETAIL_HEIGHT];
}

/** A detail that replaces the card's body, bounded, with the way back.
 *
 * The bound is on this region alone, never on the card: the card still reports
 * its own height and the action bar stays in the shell footer, so opening a
 * detail never moves a decision out of reach.
 *
 * The way back belongs to this panel in every one of its uses: the one a body
 * region opens out of its own live state, and the one the shared frame below
 * opens. Reading is not a mode with actions of its own, so it never takes the
 * card's action bar for its own use — which is why `onClose` is required. */
export function CardDetailView({ title, onClose, backRef, children }: { title: string; onClose: () => void; backRef?: React.RefObject<HTMLButtonElement | null>; children: ReactNode }) {
  const { t } = useAppLocale();
  const { displayMode } = useHostFrame();
  const fullscreen = displayMode === "fullscreen";
  const [regionRef, bound] = useDetailBound();
  return <section data-card-detail="true" className="grid min-w-0 gap-3" aria-label={title}>
    <h2 className="text-sm font-medium">{title}</h2>
    <div ref={regionRef} data-card-detail-scroll="true" className="grid min-w-0 gap-3" style={fullscreen && bound != null ? { maxHeight: bound } : undefined}>{children}</div>
    <Button ref={backRef} data-card-detail-back="true" type="button" variant="outline" size="sm" onClick={onClose}>{t("back")}</Button>
  </section>;
}

/** The body region: an open detail replaces the body instead of stacking onto
 * it. The body stays mounted behind the detail so a draft in it survives the
 * trip, but it is never shown alongside.
 *
 * The detail carries its own way back, and the card's action bar keeps the
 * card's own actions while it is open: reading a message's internals or one
 * row's routes is reading, so it never asks the reader to come back before
 * deciding. */
export function CardDetailFrame({ detail, onPage, onClose, children }: {
  detail: CardDetail | null; onPage: (page: number) => void; onClose: () => void; children: ReactNode;
}) {
  return <>
    {detail && (detail.kind === "text"
      ? <SearchPlanTextDetails title={detail.title} text={detail.text} page={detail.page} onPage={onPage} onClose={onClose} />
      : <CardDetailView title={detail.title} onClose={onClose}>{detail.content}</CardDetailView>)}
    <div hidden={detail != null}>{children}</div>
  </>;
}

export function ResultDetailContent({ children }: { children: ReactNode }) {
  const { detail, close, page } = useCardDetails();
  return <CardDetailFrame detail={detail} onPage={page} onClose={close}>{children}</CardDetailFrame>;
}

export function CardText({ text, label, className = "", lines = 3 }: { text: string; label: string; className?: string; lines?: 1 | 2 | 3 | 4 }) {
  const { t } = useAppLocale();
  const { openText } = useCardDetails();
  const element = useRef<HTMLSpanElement>(null);
  const [clipped, setClipped] = useState(false);
  useLayoutEffect(() => {
    const target = element.current;
    if (!target) return;
    const measure = () => setClipped(target.scrollHeight > target.clientHeight + 1 || target.scrollWidth > target.clientWidth + 1);
    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(target);
    return () => observer.disconnect();
  }, [text, lines]);
  return <span className="flex min-w-0 items-start gap-2"><span ref={element} className={`min-w-0 flex-1 whitespace-pre-wrap [overflow-wrap:anywhere] ${["", "line-clamp-1", "line-clamp-2", "line-clamp-3", "line-clamp-4"][lines]} ${className}`}>{text}</span>
    {clipped && <Button type="button" variant="ghost" size="sm" className="h-7 shrink-0 px-1.5" aria-label={`${t("details")}: ${label}`} onClick={() => openText(label, text)}><Eye aria-hidden="true" /></Button>}
  </span>;
}

/** A long value, read in the detail view rather than expanded under its row.
 *
 * The caller has already decided the value is long; this control is a way to
 * that view, not a disclosure, so it carries no expanded state to report. */
export function CardDetailText({ text, label }: { text: string; label: string }) {
  const { t } = useAppLocale();
  const { openText } = useCardDetails();
  return <div className="min-w-0 text-sm">
    <p className="line-clamp-2 whitespace-pre-wrap break-words leading-5">{text}</p>
    <button type="button" className="mt-1 cursor-pointer text-xs text-muted-foreground hover:underline" onClick={() => openText(label, text)}>{t("details")}</button>
  </div>;
}
