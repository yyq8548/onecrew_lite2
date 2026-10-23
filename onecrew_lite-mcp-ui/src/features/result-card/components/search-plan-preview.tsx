import { ProjectionSections } from "./projection-sections";
import { useState } from "react";
import { Eye } from "lucide-react";
import { Badge } from "@mcp-ui/components/ui/badge";
import { Button } from "@mcp-ui/components/ui/button";
import { ChannelMark } from "@mcp-ui/components/ui/platform-mark";
import { useAppLocale } from "@mcp-ui/lib/i18n";
import type { PresentationProjection, SearchPlanEditorDraft } from "@mcp-ui/lib/result-contract";
import { searchScopePreviews, type SearchScopePreview } from "@mcp-ui/features/result-card/search-plan-presentation";
import { SearchPlanPages, SearchPlanTextDetails } from "@mcp-ui/features/result-card/components/search-plan-details";

// Scopes are bound to registry operations and shown read-only in business
// language: the App never edits, reorders, or dispatches them. Only
// description and keywords are editable through the typed revision patch.
export function SearchPlanPreview({ draft, scopeOnly = false }: { draft: SearchPlanEditorDraft; scopeOnly?: boolean }) {
  const { t, locale } = useAppLocale();
  const [panel, setPanel] = useState<"overview" | "keywords" | "scopes">("overview");
  const [keywordPage, setKeywordPage] = useState(0);
  const [scopePage, setScopePage] = useState(0);
  const [details, setDetails] = useState<{ title: string; text: string; page: number } | null>(null);
  const scopes = searchScopePreviews(draft, locale);
  const inspect = (title: string, text: string) => setDetails({ title, text, page: 0 });
  if (details) return <SearchPlanTextDetails {...details} onPage={(page) => setDetails({ ...details, page })} onClose={() => setDetails(null)} />;
  const keywords = (values: string[]) => <ul className="flex min-w-0 flex-wrap gap-3">
    {values.map((value, index) => <li key={index}><Badge variant="secondary" className="max-w-full whitespace-normal break-words text-sm">{value}</Badge></li>)}
  </ul>;
  const scopeRow = (scope: SearchScopePreview, index: number) => <div className="search-scope-row" key={index}>
    <ChannelMark platform={scope.toolkit} compact />
    <div className="search-scope-facts">
      <span>{scope.kind}</span>
      {scope.details && <Button type="button" variant="link" size="sm" className="h-auto p-0" onClick={() => inspect(t("searchPlanFiltersTitle", { kind: scope.kind }), scope.details)}>{t("searchPlanFilters")}</Button>}
      {scope.summary && <span className="line-clamp-1 basis-full text-muted-foreground">{scope.summary}</span>}
    </div>
  </div>;
  if (panel === "keywords") {
    const total = Math.max(1, Math.ceil(draft.keywords.length / 6));
    const page = Math.min(keywordPage, total - 1);
    return <section className="grid min-w-0 gap-3" aria-label={t("searchPlanKeywords")}>
      <h2 className="text-sm font-medium">{t("searchPlanKeywords")} ({draft.keywords.length})</h2>
      {keywords(draft.keywords.slice(page * 6, (page + 1) * 6))}
      <SearchPlanPages page={page} total={total} onPage={setKeywordPage} />
      <Button type="button" variant="outline" size="sm" onClick={() => setPanel("overview")}>{t("back")}</Button>
    </section>;
  }
  if (panel === "scopes") {
    const page = Math.min(scopePage, scopes.length - 1);
    const scope = scopes[page];
    return <section className="grid min-w-0 gap-3" aria-label={t("searchPlanScope")}>
      <h2 className="text-sm font-medium">{t("searchPlanScopeProgress", { current: page + 1, total: scopes.length })}</h2>
      {scope && <div className="search-scope-list">{scopeRow(scope, page)}</div>}
      <SearchPlanPages page={page} total={Math.max(1, scopes.length)} onPage={setScopePage} />
      <Button type="button" variant="outline" size="sm" onClick={() => setPanel("overview")}>{t("back")}</Button>
    </section>;
  }
  return <div className="search-plan-overview">
    {!scopeOnly && <><section className="grid min-w-0 gap-1.5" aria-label={t("searchPlanDescription")}>
      <div className="flex items-center justify-between gap-2"><h2 className="text-sm text-muted-foreground">{t("searchPlanDescription")}</h2>
        {draft.description.length > 240 && <Button type="button" variant="ghost" size="sm" className="h-7 px-1.5" aria-label={t("details")} onClick={() => inspect(t("searchPlanDescription"), draft.description)}><Eye aria-hidden="true" /></Button>}
      </div>
      <p className="line-clamp-3 whitespace-pre-wrap text-sm leading-6 [overflow-wrap:anywhere]">{draft.description}</p>
    </section>
    <section className="grid min-w-0 gap-2" aria-label={t("searchPlanKeywords")}>
      <div className="flex items-center justify-between gap-2"><h2 className="text-sm text-muted-foreground">{t("searchPlanKeywords")} ({draft.keywords.length})</h2>
        {draft.keywords.length > 4 && <Button type="button" variant="link" size="sm" className="h-auto p-0" onClick={() => setPanel("keywords")}>{t("scopeViewAll", { count: draft.keywords.length })}</Button>}
      </div>
      {draft.keywords.length ? keywords(draft.keywords.slice(0, 4)) : <p className="text-sm text-muted-foreground">{t("searchPlanNoKeywords")}</p>}
    </section>
    </>}
    {!scopeOnly && <p className="text-sm text-muted-foreground">{t("searchPlanHelp")}</p>}
    <section className="grid min-w-0 gap-1.5" aria-label={t("searchPlanScopes", { count: scopes.length })}>
      <h2 className="text-sm text-muted-foreground">{t("searchPlanScopes", { count: scopes.length })}</h2>
      {scopes.length <= 3 ? <div className="search-scope-list">{scopes.map(scopeRow)}</div> : <Button type="button" variant="outline" size="sm" className="justify-self-start" onClick={() => setPanel("scopes")}>{t("searchPlanScopes", { count: scopes.length })}</Button>}
    </section>
  </div>;
}

export function SearchPlanProjectionView({ projection }: { projection: PresentationProjection }) {
  return <ProjectionSections projection={projection} />;
}
