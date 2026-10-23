import { useAppLocale } from "@mcp-ui/lib/i18n";
import type { CreatorSavePreview } from "@mcp-ui/lib/result-contract";
import { CardText } from "./text-details";

function publicLink(value: string | null): string | null {
  if (!value) return null;
  try {
    const url = new URL(value);
    return (url.protocol === "https:" || url.protocol === "http:") && !url.username && !url.password
      ? url.href : null;
  } catch {
    return null;
  }
}

export function CreatorCandidateEvidence({ preview }: { preview: CreatorSavePreview }) {
  const { t } = useAppLocale();
  const profileUrl = publicLink(preview.profile_url);
  const sourceUrl = publicLink(preview.source_url);
  return <section className="grid min-w-0 gap-3" aria-label={t("creatorCandidateEvidence")}>
    <div className="flex flex-wrap gap-3 text-sm">
      {profileUrl && <a href={profileUrl} target="_blank" rel="noopener noreferrer" className="underline">{t("creatorPublicProfile")}</a>}
      {sourceUrl && <a href={sourceUrl} target="_blank" rel="noopener noreferrer" className="underline">{t("creatorContentEvidence")}</a>}
    </div>
    <CardText text={preview.assessment ?? t("creatorNoAssessment")} label={t("creatorCandidateEvidence")} className="text-sm" />
  </section>;
}
