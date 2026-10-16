import { useTranslation } from "react-i18next";
import { z } from "zod";

export function DiscoveryPlanRequirements({ criteria }: { criteria: Record<string, unknown> }) {
  const { t } = useTranslation("shell");
  const conditions = z.array(z.object({ excerpt: z.string(), description: z.string() })).safeParse(criteria.requirements);
  const exclusions = z.array(z.object({ excerpt: z.string(), profile_url: z.string().url() })).safeParse(criteria.excluded_accounts);
  const quotes = [
    ...(conditions.success ? conditions.data.map(condition => ({ ...condition, profile_url: "" })) : []),
    ...(exclusions.success ? exclusions.data.map(condition => ({ ...condition, description: "" })) : []),
  ];
  if (quotes.length === 0 && !criteria.refinement && criteria.contact_preference !== "public_email_first") return null;
  return <div className="space-y-2 rounded-md border p-3 text-sm">
    {!!criteria.refinement && <p>{t("opencliPilot.refinementReview")}</p>}
    {quotes.length > 0 && <div>
      <p className="font-medium">{t("opencliPilot.additionalConditions")}</p>
      <ul className="list-disc pl-5">{quotes.map((condition, index) => <li key={index}>
        <p>{condition.excerpt}</p>
        {condition.description && condition.description !== condition.excerpt && <p className="text-muted-foreground">{condition.description}</p>}
        {condition.profile_url && <a className="break-all text-primary underline" href={condition.profile_url} target="_blank" rel="noreferrer">{condition.profile_url}</a>}
      </li>)}</ul>
    </div>}
    {criteria.contact_preference === "public_email_first" && <p className="text-muted-foreground">{t("opencliPilot.publicEmailPreference")}</p>}
  </div>;
}
