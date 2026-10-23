import { ExternalLink } from "lucide-react";
import { useTranslation } from "react-i18next";
import { candidateConditions, candidateRequirementGaps } from "@/lib/discovery-candidate-conditions";
import type { DiscoveryCampaign, DiscoveryCandidate } from "@/lib/opencli-discovery";
import { discoveryQualityScore } from "@onecrew/runtime-contracts/discovery";

export function DiscoveryCandidateCard({ item, campaign, reasonLabels }: {
  item: DiscoveryCandidate; campaign: DiscoveryCampaign; reasonLabels: Record<string, string>;
  sync: () => Promise<void>;
}) {
  const { t } = useTranslation("shell");
  const { met, reasons } = candidateConditions(item, campaign.criteria);
  const gaps = candidateRequirementGaps(item, campaign.criteria);
  const requirementLabels: Record<string, string> = {
    related_alternative: t("opencliPilot.relatedAlternative"),
    requirement_unverified: t("opencliPilot.reason.requirement"), requirement_contradicted: t("opencliPilot.reason.requirementConflict"),
    linkedin_current_employment_evidence_exhausted: t("opencliPilot.reason.experienceExhausted"),
    excluded_by_customer: t("opencliPilot.reason.excludedAccount"), excluded_account_identity_unverified: t("opencliPilot.reason.requirement"),
    requirements_invalid: t("opencliPilot.reason.unsupported"), excluded_accounts_invalid: t("opencliPilot.reason.unsupported"),
  };
  const assessment = item.evidence.assessment;
  const rating = assessment && Object.hasOwn(assessment, "quality_rating") ? assessment.quality_rating : assessment?.match_rating;
  return <li className="rounded-md border p-3 text-sm">
    <div className="flex flex-wrap items-center justify-between gap-2">
      <a className="inline-flex items-center gap-1 text-primary underline" href={item.profile_url} target="_blank" rel="noreferrer">
        {item.display_name}<ExternalLink className="size-3" aria-hidden />
      </a>

    </div>
    {rating && <p className="mt-1">
      {discoveryQualityScore(rating.level)} · {t(`opencliPilot.matchLevel.${rating.level}`)}
    </p>}
    {!rating && assessment?.selection && <p className="mt-1">{t("opencliPilot.qualityUnrated")}</p>}
    {assessment?.selection && <p className="mt-1">{t(`opencliPilot.selectionState.${assessment.selection.state}`)}</p>}
    {met.length > 0 && <p className="mt-1 text-muted-foreground">{met.map(key => t(`opencliPilot.${key}`)).join(" · ")}</p>}
    <p className="mt-1 text-muted-foreground">{rating?.reason || (reasons.length
      ? reasons.map(reason => requirementLabels[reason] ?? reasonLabels[reason] ?? reason).join(" · ")
      : t(item.verdict === "qualified" ? "opencliPilot.autoRecommended" : "opencliPilot.matchUnknown"))}</p>
    {assessment?.selection && <p className="mt-1 text-muted-foreground">{assessment.selection.reason}</p>}
    {gaps.length > 0 && <div className="mt-2 space-y-1"><p>{t("opencliPilot.additionalConditions")}</p>
      <ul className="list-disc pl-5">{gaps.map((gap, index) => <li key={index}>{gap.condition}{gap.reason && <p className="text-muted-foreground">{gap.reason}</p>}</li>)}</ul>
    </div>}

  </li>;
}
