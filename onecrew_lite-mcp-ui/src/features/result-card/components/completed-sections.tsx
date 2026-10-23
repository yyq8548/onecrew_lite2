import type { ResultSnapshot } from "@mcp-ui/lib/result-contract";
import { useAppLocale } from "@mcp-ui/lib/i18n";
import { ProjectionSections } from "./projection-sections";

/** The server publishes the exact safe completed prefix; never infer it from
 * labels, provider kinds, or the current pending action. */
export function CompletedResultSections({ snapshot }: { snapshot: ResultSnapshot }) {
  const { t } = useAppLocale();
  if (!snapshot.completed_sections_count) return null;
  return <section className="grid gap-3 rounded-lg border bg-muted/20 p-3" aria-label={t("completedActions")}>
    <h2 className="text-sm font-medium">{t("completedActions")}</h2>
    <ProjectionSections projection={{ ...snapshot.presentation, sections: snapshot.presentation.sections.slice(0, snapshot.completed_sections_count) }} />
  </section>;
}
