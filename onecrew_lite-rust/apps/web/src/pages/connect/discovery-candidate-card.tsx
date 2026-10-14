import { ExternalLink } from "lucide-react";
import { useTranslation } from "react-i18next";
import { candidateConditions } from "@/lib/discovery-candidate-conditions";
import type { DiscoveryCampaign, DiscoveryCandidate } from "@/lib/opencli-discovery";

export function DiscoveryCandidateCard({ item, campaign, reasonLabels }: {
  item: DiscoveryCandidate; campaign: DiscoveryCampaign; reasonLabels: Record<string, string>;
  sync: () => Promise<void>;
}) {
  const { t } = useTranslation("shell");
  const { met, reasons } = candidateConditions(item, campaign.criteria);
  const rating = item.verdict !== "pending" ? item.evidence.assessment?.match_rating : null;
  return <li className="rounded-md border p-3 text-sm">
    <div className="flex flex-wrap items-center justify-between gap-2">
      <a className="inline-flex items-center gap-1 text-primary underline" href={item.profile_url} target="_blank" rel="noreferrer">
        {item.display_name}<ExternalLink className="size-3" aria-hidden />
      </a>

    </div>
    {rating && <p className="mt-1">
      {t(`opencliPilot.matchLevel.${rating.level}`)}
    </p>}
    {met.length > 0 && <p className="mt-1 text-muted-foreground">{met.map(key => t(`opencliPilot.${key}`)).join(" · ")}</p>}
    <p className="mt-1 text-muted-foreground">{rating?.reason || (reasons.length
      ? reasons.map(reason => reasonLabels[reason] ?? reason).join(" · ")
      : t(item.verdict === "qualified" ? "opencliPilot.autoRecommended" : "opencliPilot.matchUnknown"))}</p>

  </li>;
}
