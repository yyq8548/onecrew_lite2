import { CreatorSaveOutcomeRows } from "./creator-save-outcomes";
import { EmailPreviewCard } from "./email-preview";
import { CardDetailText } from "@mcp-ui/features/result-card/components/text-details";
import { PlatformMark } from "@mcp-ui/components/ui/platform-mark";
import { displayLabel } from "@mcp-ui/lib/display-label";
import { useAppLocale } from "@mcp-ui/lib/i18n";
import type { PresentationProjection } from "@mcp-ui/lib/result-contract";
import { emailAsideSections, emailPreview } from "../email-preview";
import { projectionRows } from "../projection-pages";

/** Pick the shape this projection already is.
 *
 * A prepared message reads as a message; everything else keeps the labelled
 * table it has always had. Each shape is its own component so switching between
 * them mounts fresh state instead of reordering hooks. */
export function ProjectionSections({ projection }: { projection: PresentationProjection }) {
  const mail = emailPreview(projection);
  if (!mail) return <ProjectionRows projection={projection} />;
  const aside = emailAsideSections(projection);
  return <div className="grid min-w-0 gap-3">
    <EmailPreviewCard model={mail} />
    {aside.length > 0 && <ProjectionRows projection={{ ...projection, sections: aside }} />}
  </div>;
}

function ProjectionRows({ projection }: { projection: PresentationProjection }) {
  const { t, locale } = useAppLocale();
  const outcomeRows = projection.sections.flatMap(section => section.kind === "creator_save_rows" ? section.rows : []);
  const rows = projectionRows({...projection, sections: projection.sections.filter(section => section.kind !== "creator_save_rows")});
  return <div className="grid min-w-0 gap-3">
    {outcomeRows.length > 0 && <CreatorSaveOutcomeRows rows={outcomeRows} />}
    {!rows.length && !outcomeRows.length && <p className="text-sm text-muted-foreground">{t("noResults")}</p>}
    <dl className="min-w-0 divide-y">{rows.map((row, index) => <div key={index} className={`min-w-0 py-2 ${row.tone === "critical" ? "text-destructive" : row.tone === "warning" ? "text-warning-foreground" : ""}`}>
      {row.heading && (index === 0 || row.heading !== rows[index - 1]?.heading) && <h2 className="mb-4 text-sm font-medium [overflow-wrap:anywhere]">{displayLabel(row.heading, locale)}</h2>}
      <div className="grid min-w-0 grid-cols-[max-content_minmax(0,1fr)] items-center gap-3">
        {row.label && <dt className="text-sm text-muted-foreground [overflow-wrap:anywhere]">{displayLabel(row.label, locale)}</dt>}
        <dd className={`min-w-0 text-sm [overflow-wrap:anywhere] ${!row.label ? "col-span-2" : ""}`}>
          {/^(创作者|creator|display name|显示名称)$/i.test(row.label) ? <span className="flex items-center gap-3"><span className="grid size-9 shrink-0 place-items-center rounded-full bg-muted text-sm" aria-hidden="true">{Array.from(row.value)[0]?.toUpperCase()}</span><span className="font-medium">{row.value}</span></span> : /^(平台|platform)$/i.test(row.label) && ["x","instagram","youtube","tiktok","telegram"].includes(row.value.toLowerCase()) ? <PlatformMark platform={row.value.toLowerCase() as "x" | "instagram" | "youtube" | "tiktok" | "telegram"} /> : row.value.length > 160 || row.value.split("\n").length > 4 ? <CardDetailText text={row.value} label={row.label ? displayLabel(row.label, locale) : t("cardText")} /> : <span className="whitespace-pre-wrap">{row.value}</span>}
        </dd>
      </div>
    </div>)}</dl>
  </div>;
}
