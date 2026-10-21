import { useTranslation } from "react-i18next";
import { z } from "zod";
import { discoveryQualityFilter, type DiscoveryQualityFilter } from "@onecrew/runtime-contracts/discovery";

export function DiscoveryPlanRequirements({ criteria }: { criteria: Record<string, unknown> }) {
  const { t, i18n } = useTranslation("shell");
  const conditions = z.array(z.object({ excerpt: z.string(), description: z.string() })).safeParse(criteria.requirements);
  const exclusions = z.array(z.object({ excerpt: z.string(), profile_url: z.string().url() })).safeParse(criteria.excluded_accounts);
  const rule = criteria.quality_filter == null ? null : discoveryQualityFilter.parse(criteria.quality_filter), grouped = new Set<number>();
  const english = i18n.resolvedLanguage?.startsWith("en") === true;
  const describe = (node: DiscoveryQualityFilter): string => {
    if (node.kind === "minimum") return `${english ? "Quality" : "评分"} ${node.operator === "gte" ? "≥" : ">"} ${node.score}`;
    if (node.kind === "requirement") {
      grouped.add(node.index); const value = conditions.success ? conditions.data[node.index] : null;
      if (!value) throw new Error("quality_filter_requirement_missing"); return value.excerpt;
    }
    return `(${node.items.map(describe).join(node.kind === "all" ? english ? " AND " : " 且 " : english ? " OR " : " 或 ")})`;
  };
  const filterLabel = rule === null ? null : describe(rule);
  const quotes = [...(conditions.success ? conditions.data.filter((_, index) => !grouped.has(index)).map(condition => ({ ...condition, profile_url: "" })) : []), ...(exclusions.success ? exclusions.data.map(condition => ({ ...condition, description: "" })) : [])];
  if (quotes.length === 0 && !Object.hasOwn(criteria, "quality_filter") && !criteria.refinement && criteria.contact_preference !== "public_email_first" && criteria.pinterest_min_monthly_views_exclusive === undefined) return null;
  return <div className="space-y-2 rounded-md border p-3 text-sm">
    {filterLabel !== null ? <p>{t("opencliPilot.qualityFilterLabel")}：{filterLabel}</p> : Object.hasOwn(criteria, "quality_filter") && <p>{t("opencliPilot.noQualityMinimum")}</p>}
    {typeof criteria.pinterest_min_monthly_views_exclusive === "number" && <p>{t("opencliPilot.pinterestScale", { followers: String(criteria.min_subscribers ?? criteria.min_followers ?? 0), views: criteria.pinterest_min_monthly_views_exclusive })}</p>}
    {!!criteria.refinement && <p>{t("opencliPilot.refinementReview")}</p>}
    {quotes.length > 0 && <div><p className="font-medium">{t("opencliPilot.additionalConditions")}</p><ul className="list-disc pl-5">{quotes.map((condition, index) => <li key={index}>
      <p>{condition.excerpt}</p>{condition.description && condition.description !== condition.excerpt && <p className="text-muted-foreground">{condition.description}</p>}
      {condition.profile_url && <a className="break-all text-primary underline" href={condition.profile_url} target="_blank" rel="noreferrer">{condition.profile_url}</a>}
    </li>)}</ul></div>}
    {criteria.contact_preference === "public_email_first" && <p className="text-muted-foreground">{t("opencliPilot.publicEmailPreference")}</p>}
  </div>;
}
