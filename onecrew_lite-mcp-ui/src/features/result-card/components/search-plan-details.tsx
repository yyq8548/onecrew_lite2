import { Button } from "@mcp-ui/components/ui/button";
import { useAppLocale } from "@mcp-ui/lib/i18n";
import { textPages } from "@mcp-ui/features/result-card/text-pages";

export function SearchPlanPages({ page, total, onPage, disabled = false }: { page: number; total: number; onPage: (page: number) => void; disabled?: boolean }) {
  const { t } = useAppLocale();
  if (total <= 1) return null;
  return <div className="flex flex-wrap items-center justify-between gap-2">
    <Button type="button" variant="outline" size="sm" disabled={disabled || page === 0} onClick={() => onPage(page - 1)}>{t("previousPage")}</Button>
    <span className="text-sm" role="status">{t("page", { current: page + 1, total })}</span>
    <Button type="button" variant="outline" size="sm" disabled={disabled || page + 1 === total} onClick={() => onPage(page + 1)}>{t("nextPage")}</Button>
  </div>;
}

/** Paged long text, shown instead of the body it replaced.
 *
 * The way back is this panel's own, whether a body region opened it out of its
 * own state or the shared detail frame did: reading long text is reading, so
 * the card's action bar keeps the card's actions while it is open. */
export function SearchPlanTextDetails({ title, text, page, onPage, onClose }: { title: string; text: string; page: number; onPage: (page: number) => void; onClose: () => void }) {
  const { t } = useAppLocale();
  const pages = textPages(text);
  const current = Math.min(page, pages.length - 1);
  return <section className="grid min-w-0 gap-3" aria-label={title}>
    <h2 className="text-sm font-medium">{title}</h2>
    <p className="whitespace-pre-wrap text-sm leading-6 [overflow-wrap:anywhere]">{pages[current]}</p>
    <SearchPlanPages page={current} total={pages.length} onPage={onPage} />
    <Button data-card-detail-back="true" type="button" variant="outline" size="sm" onClick={onClose}>{t("back")}</Button>
  </section>;
}
