import { useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { Sheet, SheetContent, SheetHeader, SheetTitle, SheetDescription, SheetTrigger } from "@/components/ui/sheet";
import type { SubmitDiscoveryReviewInput } from "@/lib/generated/discovery-contracts";
import { discoveryReviewKey, getDiscoveryReview, evaluateDiscoveryMatch, reviewRefreshInterval, type ReviewMaterial, type ReviewRequest } from "@/lib/discovery-review";
import { syncDiscoveryLibrary } from "@/lib/opencli-discovery";
import { collectDiscoveryEvidence } from "@/lib/discovery-evidence";
import { PublicPageReceipt } from "./discovery-public-page";

type Criterion = Extract<SubmitDiscoveryReviewInput["findings"][number]["criterion"], string> | `requirement:${number}`;
type Outcome = SubmitDiscoveryReviewInput["findings"][number]["outcome"];
type DraftFinding = { outcome: Outcome; rationale: string; citation: string; quote: string; toolName: string; corroboration: string; corroborationQuote: string; countryCode: string; matchLevel?: "excellent" | "good" | "medium" | "low" };
const emptyFinding = (): DraftFinding => ({ outcome: "insufficient", rationale: "", citation: "", quote: "", toolName: "", corroboration: "", corroborationQuote: "", countryCode: "" });

export function reviewCriteria(material: ReviewMaterial): Criterion[] {
  const criteria = material.criteria && typeof material.criteria === "object" ? material.criteria as Record<string, unknown> : {};
  const legacy = ([
    ["requires_ai_tool_demo", "own_ai_tool_demo"],
    ["requires_public_tool_url", "public_tool_url"],
  ] as const).filter(([key]) => criteria[key] !== false).map(([, criterion]) => criterion);
  return [
    ...legacy,
    ...(Array.isArray(criteria.requirements) ? criteria.requirements.map((_, index) => `requirement:${index}` as const) : []),
    ...(criteria.category === "服装" ? ["apparel_business_content" as const] : []),
    ...(typeof criteria.category === "string" && criteria.category.length > 0 && criteria.category !== "服装"
      ? ["category_content" as const] : []),
    ...(criteria.region === "中国大陆以外" ? ["creator_residence_outside_mainland" as const] : []),
    ...(typeof criteria.language === "string" && !["", "不限", "语言不限", "any", "all", "unrestricted"].includes(criteria.language.trim().toLowerCase())
      ? ["content_language" as const] : []),
  ];
}

export function reviewCitation(material: ReviewMaterial, selected: string, quote: string) {
  if (!quote.trim() || quote.length > 4000) return null;
  if (selected === "description" && material.video_description?.includes(quote)) return { kind: "description" as const, quote };
  if (selected === "caption" && material.hit.platform === "tiktok" && material.hit.title.includes(quote))
    return { kind: "search_caption" as const, quote };
  const native = material.native_content?.native_content;
  if (selected === "native" && native && (("text" in native && native.text.includes(quote))
    || ("description" in native && native.description.includes(quote))))
    return { kind: "native_content" as const, quote };
  const [kind, indexText] = selected.split(":");
  const index = Number(indexText);
  if (!Number.isSafeInteger(index) || index < 0) return null;
  if (kind === "transcript" && material.transcript?.status === "completed" && !material.transcript.stop_code
    && material.transcript.segments[index]?.text.includes(quote)) return { kind: "transcript" as const, segment_index: index, quote };
  if (kind === "page") {
    const page = material.public_pages[index];
    if (page?.status === "completed" && !page.stop_code && page.markdown_excerpt.includes(quote))
      return { kind: "public_page" as const, requested_url: page.requested_url, quote };
  }
  return null;
}

export function buildReviewFindings(material: ReviewMaterial, drafts: Partial<Record<Criterion, DraftFinding>>): SubmitDiscoveryReviewInput["findings"] | null {
  const result: SubmitDiscoveryReviewInput["findings"] = [];
  for (const criterion of [...reviewCriteria(material), ...(drafts.match_fit?.rationale.trim() ? ["match_fit" as const] : [])]) {
    const draft = drafts[criterion];
    if (!draft) return null;
    if (!draft.rationale.trim() || draft.rationale.length > 4000) return null;
    const citation = draft.outcome === "insufficient" ? null : reviewCitation(material, draft.citation, draft.quote);
    if (draft.outcome !== "insufficient" && !citation) return null;
    if (criterion === "own_ai_tool_demo" && draft.outcome === "supported" && citation?.kind !== "transcript") return null;
    if ((criterion === "content_language" || criterion === "match_fit") && draft.outcome !== "insufficient"
      && citation?.kind !== "description" && citation?.kind !== "transcript" && citation?.kind !== "native_content"
      && !(material.hit.platform === "tiktok" && citation?.kind === "search_caption")) return null;
    const language = typeof material.criteria === "object" && material.criteria && "language" in material.criteria ? String(material.criteria.language).toLowerCase() : "";
    if (criterion === "content_language" && draft.outcome !== "insufficient"
      && ["spoken", "speech", "口语", "口播", "语音"].some(word => language.includes(word)) && citation?.kind !== "transcript") return null;
    if (criterion === "match_fit" && (draft.outcome === "contradicted" || (draft.outcome === "supported" && !draft.matchLevel))) return null;
    if (criterion === "apparel_business_content" && draft.outcome === "supported"
      && citation?.kind !== "transcript" && citation?.kind !== "native_content" && citation?.kind !== "search_caption") return null;
    if (criterion === "category_content" && draft.outcome === "supported"
      && citation?.kind !== "description" && citation?.kind !== "transcript" && citation?.kind !== "native_content"
      && !(material.hit.platform === "tiktok" && citation?.kind === "search_caption")) return null;
    if (criterion === "creator_residence_outside_mainland" && draft.outcome === "supported"
      && citation?.kind !== "transcript" && citation?.kind !== "native_content") return null;
    const residence = criterion === "creator_residence_outside_mainland" && draft.outcome === "supported";
    const corroboration = residence ? reviewCitation(material, draft.corroboration, draft.corroborationQuote) : null;
    if (residence && (corroboration?.kind !== "public_page" || !/^[A-Z]{2}$/.test(draft.countryCode) || draft.countryCode === "CN")) return null;
    const tool = criterion === "public_tool_url" && draft.outcome === "supported"
      ? material.public_pages.find(page => page.requested_url === (citation?.kind === "public_page" ? citation.requested_url : "")) : null;
    if (criterion === "public_tool_url" && draft.outcome === "supported"
      && (!tool || !draft.toolName.trim()
        || !(tool.title.toLowerCase().includes(draft.toolName.trim().toLowerCase())
          || draft.quote.toLowerCase().includes(draft.toolName.trim().toLowerCase())))) return null;
    result.push({ criterion: criterion.startsWith("requirement:") ? { requirement: Number(criterion.slice(12)) } : criterion as Extract<SubmitDiscoveryReviewInput["findings"][number]["criterion"], string>, outcome: draft.outcome, rationale: draft.rationale.trim(),
      citations: [...(citation ? [citation] : []), ...(corroboration ? [corroboration] : [])],
      tool: tool ? { name: draft.toolName.trim(), requested_url: tool.requested_url } : null,
      ...(residence ? { country_code: draft.countryCode } : {}),
      ...(criterion === "match_fit" && draft.outcome === "supported" ? { match_level: draft.matchLevel } : {}),
    });
  }
  return result;
}

export function ReviewMaterialContent({ material }: { material: ReviewMaterial }) {
  const { t, i18n } = useTranslation("shell");
  const transcript = material.transcript;
  const criteria = material.criteria && typeof material.criteria === "object" ? material.criteria as Record<string, unknown> : {};
  const requirements: unknown[] = Array.isArray(criteria.requirements) ? criteria.requirements : [];
  return <div className="space-y-4 break-words text-sm">
    <p className="text-muted-foreground">{t("opencliPilot.reviewLimit")}</p>
    <p>{material.hit.source_url}</p>
    <p>{t("opencliPilot.reviewAuthor", { id: material.native_author_id })}</p>
    <p>{t("opencliPilot.reviewRevision", { revision: material.criteria_revision })}</p>
    {requirements.length > 0 && <ul className="list-disc pl-5">
      {requirements.map((requirement, index) => <li key={index}>
        {typeof requirement === "object" && requirement !== null && "excerpt" in requirement && typeof requirement.excerpt === "string" ? requirement.excerpt : t("opencliPilot.reason.unsupported")}
      </li>)}
    </ul>}
    {typeof material.native_followers === "number" && <p>{t("opencliPilot.reviewNativeFollowers", { count: material.native_followers })}</p>}
    {material.native_follower_estimate && <p>{t("opencliPilot.reviewApproximateFollowers", {
      display: material.native_follower_estimate.display,
      lower: material.native_follower_estimate.lower.toLocaleString(i18n.resolvedLanguage),
      upper: material.native_follower_estimate.upper.toLocaleString(i18n.resolvedLanguage),
    })}</p>}
    {material.native_profile && <div className="space-y-1 rounded-md border p-3">
      <p className="font-medium">{t("opencliPilot.reviewNativeProfile")}</p>
      <p className="break-all">{material.native_profile.profile_url}</p>
      <p>{t("opencliPilot.reviewCaptured", { time: new Date(material.native_profile.observed_at * 1000).toLocaleString(i18n.resolvedLanguage) })}</p>
      {material.native_profile.bio && <p className="whitespace-pre-wrap">{material.native_profile.bio}</p>}
      {material.native_profile.self_declared_location && <p>{t("opencliPilot.reviewSelfDeclaredLocation", { location: material.native_profile.self_declared_location })}</p>}
      <p className="text-muted-foreground">{t("opencliPilot.reviewNativeProfileLimit")}</p>
    </div>}
    <p className="break-all text-xs">{t("opencliPilot.reviewHash", { hash: material.material_hash })}</p>
    {material.native_content?.native_content && "text" in material.native_content.native_content && <>
      <h3 className="font-medium">{t("opencliPilot.reviewNativeContent")}</h3>
      <p className="whitespace-pre-wrap">{material.native_content.native_content.text}</p>
      {"text_truncated" in material.native_content.native_content && material.native_content.native_content.text_truncated && <p>{t("opencliPilot.reviewTruncated")}</p>}
    </>}
    {material.native_content?.native_content && "description" in material.native_content.native_content && <>
      <h3 className="font-medium">{t("opencliPilot.reviewNativeContent")}</h3>
      <p className="whitespace-pre-wrap">{material.native_content.native_content.description}</p>
      {material.native_content.native_content.description_truncated && <p>{t("opencliPilot.reviewTruncated")}</p>}
    </>}
    {material.hit.platform === "tiktok" && <>
      <h3 className="font-medium">{t("opencliPilot.reviewSearchCaption")}</h3>
      <p className="whitespace-pre-wrap">{material.hit.title}</p>
      <p className="text-muted-foreground">{t("opencliPilot.reviewSearchCaptionLimit")}</p>
    </>}
    {material.hit.platform === "youtube" && <>
      <h3 className="font-medium">{t("opencliPilot.reviewDescription")}</h3>
      <p className="whitespace-pre-wrap">{material.video_description ?? t("opencliPilot.reviewMissing")}</p>
      <h3 className="font-medium">{t("opencliPilot.reviewTranscript")}</h3>
    </>}
    {material.hit.platform === "youtube" && (!transcript ? <p>{t("opencliPilot.reviewMissing")}</p> : <>
      <p>{t(`opencliPilot.reviewStatus.${transcript.status}`)}{transcript.stop_code ? ` · ${transcript.stop_code}` : ""}</p>
      <p>{t("opencliPilot.reviewCaptured", { time: new Date(transcript.captured_at * 1000).toLocaleString(i18n.resolvedLanguage) })}</p>
      {transcript.status === "completed" && <>
        {transcript.segments.map((line, index) => <p className="whitespace-pre-wrap" key={index}>
          <span className="font-mono">{line.timestamp}</span> {line.speaker} {line.text}
        </p>)}
        {transcript.truncated && <p>{t("opencliPilot.reviewTruncated")}</p>}
      </>}
    </>)}
    <h3 className="font-medium">{t("opencliPilot.reviewPages")}</h3>
    {material.public_pages.length === 0 ? <p>{t("opencliPilot.reviewMissing")}</p>
      : material.public_pages.map(page => <PublicPageReceipt key={page.requested_url} read={page} />)}
  </div>;
}

type ReviewDrawerProps = { request: ReviewRequest; title: string; platform?: string; pending?: boolean; retryAutomatic?: boolean; onReviewSaved?: (campaignId: string) => Promise<void> };
export function SavedReviewMaterial({ request, title, platform, pending, retryAutomatic = false, onReviewSaved }: ReviewDrawerProps) {
  const { t } = useTranslation("shell");
  const [opened, setOpened] = useState(false);
  const [syncFailed, setSyncFailed] = useState(false);
  const client = useQueryClient();
  const query = useQuery({ queryKey: discoveryReviewKey(request), queryFn: () => getDiscoveryReview(request),
    enabled: opened, retry: false, refetchInterval: state => reviewRefreshInterval(state.state.dataUpdateCount, state.state.status === "error") });
  const collect = useMutation({
    retry: false,
    mutationFn: (kind: "transcript" | "profile" | "content") => collectDiscoveryEvidence({
      campaign_id: request.campaignId, expected_revision: request.revision,
      search_hit_id: request.hitId, profile_ref: request.profileRef, kind,
    }),
    onSettled: async () => {
      await Promise.all(["opencli-campaign", "opencli-campaign-candidates", "opencli-campaign-candidate-counts"]
        .map(key => client.invalidateQueries({ queryKey: [key, request.campaignId] })));
      await query.refetch();
    },
  });
  const source = query.data?.hit.platform ?? platform;
  const review = useMutation({ mutationFn: () => evaluateDiscoveryMatch({ campaign_id: request.campaignId,
    expected_revision: request.revision, search_hit_id: request.hitId }, !retryAutomatic),
    onSuccess: async receipt => {
      if (!receipt) return;
      await Promise.all([
        client.invalidateQueries({ queryKey: discoveryReviewKey(request) }),
        client.invalidateQueries({ queryKey: ["opencli-campaign-candidates", request.campaignId] }),
        client.invalidateQueries({ queryKey: ["opencli-campaign-candidate-counts", request.campaignId] }),
      ]);
      try { await (onReviewSaved ? onReviewSaved(request.campaignId) : syncDiscoveryLibrary(request.campaignId)); setSyncFailed(false); }
      catch { setSyncFailed(true); }
    },
  });
  return <Sheet open={opened} onOpenChange={setOpened}>
    <SheetTrigger asChild><Button size="sm" variant="outline" aria-label={t("opencliPilot.reviewOpen", { title })} title={title}>
      {t(pending ? "opencliPilot.pendingResolve" : "opencliPilot.reviewOpenAction")}
    </Button></SheetTrigger>
    <SheetContent className="w-full overflow-y-auto sm:max-w-xl">
      <SheetHeader><SheetTitle>{title}</SheetTitle><SheetDescription>{t("opencliPilot.initialHelp")}</SheetDescription></SheetHeader>
      <div className="space-y-3 px-4 pb-6">
        <Button variant="outline" disabled={query.isFetching || review.isPending} onClick={() => { void query.refetch(); }}>{t("opencliPilot.reviewRefresh")}</Button>
        {query.isFetching && <p role="status">{t("opencliPilot.publicPageLoading")}</p>}
        {query.isError && <p role="alert">{t("opencliPilot.reviewUnavailable")}</p>}
        {query.data && <ReviewMaterialContent material={query.data} />}
        {pending && <div className="space-y-2 rounded-md border p-3">
          <p className="text-sm">{t("opencliPilot.evidenceCollectHelp")}</p>
          <div className="flex flex-wrap gap-2">
            {source === "youtube" && <Button variant="outline" disabled={collect.isPending || review.isPending} onClick={() => collect.mutate("transcript")}>{t("opencliPilot.collectTranscript")}</Button>}
            {["tiktok", "bilibili", "twitter"].includes(source ?? "") && <Button variant="outline" disabled={collect.isPending || review.isPending} onClick={() => collect.mutate("profile")}>{t("opencliPilot.collectFollowers")}</Button>}
            {["bilibili", "twitter"].includes(source ?? "") && <Button variant="outline" disabled={collect.isPending || review.isPending} onClick={() => collect.mutate("content")}>{t("opencliPilot.collectContent")}</Button>}
          </div>
          {collect.isPending && <p role="status">{t("opencliPilot.evidenceCollect")}</p>}
          {collect.isError && <p role="alert">{t("opencliPilot.evidenceCollectFailed")}</p>}
          {collect.data && <p role="status">{t(collect.data.status === "completed" ? "opencliPilot.evidenceCollected" : "opencliPilot.evidenceCollectFailed")}</p>}
        </div>}
        <Button disabled={!query.data || review.isPending || collect.isPending} onClick={() => review.mutate()}>{t("opencliPilot.initialRescore")}</Button>
        {review.isPending && <p role="status">{t("opencliPilot.initialMatching")}</p>}
        {review.isError && <p role="alert">{t("opencliPilot.initialFailed")}</p>}
        {review.isSuccess && review.data === null && <p>{t("opencliPilot.initialMissing")}</p>}
        {review.data && <p role="status">{t("opencliPilot.reviewSaved", { verdict: review.data.verdict === "qualified"
          ? t("opencliPilot.verdict.qualified") : review.data.verdict === "rejected" ? t("opencliPilot.verdict.rejected") : t("opencliPilot.verdict.pending") })}</p>}
        {syncFailed && <p role="alert">{t("opencliPilot.reviewLibrarySyncFailed")}</p>}
      </div>
    </SheetContent>
  </Sheet>;
}
