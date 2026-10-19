import { invoke } from "./local-runtime";
import type { DiscoveryReviewMaterialInput, GetDiscoveryReviewInput, SubmitDiscoveryReviewInput as OriginalSubmitDiscoveryReviewInput } from "./generated/discovery-contracts";
import { discoveryReviewMaterial, discoveryReviewReceipt, type DiscoveryReviewCriterion, type DiscoveryReviewMaterial as GeneratedMaterial, type DiscoveryReviewReceipt } from "@onecrew/runtime-contracts/discovery";
export type SubmitDiscoveryReviewInput = Omit<OriginalSubmitDiscoveryReviewInput, "findings"> & { findings: (Omit<OriginalSubmitDiscoveryReviewInput["findings"][number], "criterion"> & { criterion: DiscoveryReviewCriterion })[] };
import { isU64 } from "@onecrew/runtime-contracts/result-wire";
const validateDiscoveryReviewMaterial = (value: unknown): value is GeneratedMaterial => discoveryReviewMaterial.safeParse(value).success;
const validateDiscoveryReviewReceipt = (value: unknown): value is DiscoveryReviewReceipt => discoveryReviewReceipt.safeParse(value).success;
import { parseDiscoveryContent, parseDiscoveryPublicPage, parseDiscoveryTranscript, type DiscoveryAdvanceResult } from "./opencli-discovery";
import { additionalNativePlatforms, additionalNativeReviewBound } from "./discovery-native-content";

export function initialMatchHit(result: DiscoveryAdvanceResult): string | null {
  if (result.provider_stop_code) return null;
  const step = result.step;
  if (step.kind === "read_video") return step.search_hit_id;
  if (step.kind === "read_web_source" && result.web_source?.status === "completed" && result.web_source.is_primary_source && result.web_source.candidate_id) return step.search_hit_id;
  if (step.kind === "read_profile" && result.profile?.status === "completed") return step.search_hit_id;
  if (step.kind === "read_source_author" && result.source_author?.status === "completed" && result.source_author.duplicate_of === null) return step.search_hit_id;
  if (step.kind === "read_content" && result.content?.status === "completed") return step.search_hit_id;
  if (step.kind === "read_transcript" && result.transcript?.status === "completed") return step.search_hit_id;
  if (step.kind === "read_public_page" && result.public_page?.status === "completed") return result.public_page.search_hit_id;
  return null;
}

export function initialMatchReason(proof: unknown): string | null {
  if (!proof || typeof proof !== "object" || !("policy_version" in proof) || !("reason" in proof)) return null;
  return proof.policy_version === "initial-match-evaluation-v1" && typeof proof.reason === "string" ? proof.reason : null;
}

/** Automatic scoring writes only an owner-bound judgment, never searches or approves. */
export async function evaluateDiscoveryMatch(input: DiscoveryReviewMaterialInput, reassess = false,
  onDeferred?: (code: string) => void): Promise<DiscoveryReviewReceipt | null> {
  let raw: unknown;
  try {
    raw = await invoke<unknown>(reassess ? "onecrew_discovery_match_reevaluate" : "onecrew_discovery_match_evaluate", { input });
  } catch (error) {
    const code = typeof error === "string" ? error : error instanceof Error ? error.message : "";
    // These owner errors occur before review submission. Transport failures and
    // uncertain writes must still stop; manual scoring still reports the error.
    if (onDeferred && !reassess && ["initial_match_busy",
      "initial_match_discovery_draft_codex_unavailable", "initial_match_discovery_draft_codex_login_required",
      "initial_match_discovery_draft_codex_failed", "initial_match_discovery_draft_codex_timeout",
      "initial_match_discovery_draft_plan_invalid"].includes(code)) {
      onDeferred(code);
      return null;
    }
    throw error;
  }
  if (raw === null) return null;
  if (!validateDiscoveryReviewReceipt(raw) || raw.campaign_id !== input.campaign_id
    || raw.criteria_revision !== input.expected_revision || raw.search_hit_id !== input.search_hit_id
    || raw.policy_version !== "initial-match-evaluation-v1") throw new Error("initial_match_receipt_invalid");
  return raw;
}

export type ReviewRequest = { campaignId: string; revision: number; hitId: string; profileRef: string };
export interface ReviewMaterial extends GeneratedMaterial {
  transcript: ReturnType<typeof parseDiscoveryTranscript> | null;
  public_pages: ReturnType<typeof parseDiscoveryPublicPage>[];
  native_content?: ReturnType<typeof parseDiscoveryContent> | null;
}
export const discoveryReviewKey = (r: ReviewRequest) =>
  ["opencli-campaign-review", r.campaignId, r.revision, r.hitId, r.profileRef] as const;

// No discovery evidence runtime event exists yet. Poll local snapshots only,
// while the drawer is open, at most twelve observations; never retry an error.
export const reviewRefreshInterval = (observations: number, failed: boolean) =>
  !failed && observations < 12 ? 5000 : false;

export async function getSavedDiscoveryReview(input: GetDiscoveryReviewInput): Promise<DiscoveryReviewReceipt | null> {
  const raw = await invoke<unknown>("onecrew_discovery_review_get", { input });
  if (raw === null) return null;
  if (!validateDiscoveryReviewReceipt(raw) || raw.campaign_id !== input.campaign_id || raw.request_id !== input.request_id)
    throw new Error("discovery_review_receipt_invalid");
  return raw;
}

/** A failed submission has an unknown effect; read the same request before any retry. */
export async function submitDiscoveryReview(input: SubmitDiscoveryReviewInput): Promise<DiscoveryReviewReceipt> {
  const matchesInput = (receipt: DiscoveryReviewReceipt) =>
    receipt.campaign_id === input.campaign_id && receipt.search_hit_id === input.search_hit_id
    && receipt.criteria_revision === input.expected_revision && receipt.material_hash === input.material_hash
    && receipt.request_id === input.request_id
    && receipt.judgments.length === input.findings.length
    && receipt.judgments.every((judgment, index) => JSON.stringify(judgment.criterion) === JSON.stringify(input.findings[index].criterion)
      && judgment.outcome === input.findings[index].outcome);
  let raw: unknown;
  try {
    raw = await invoke<unknown>("onecrew_discovery_review_submit", { input });
  } catch (error) {
    const saved = await getSavedDiscoveryReview({ campaign_id: input.campaign_id, request_id: input.request_id });
    if (saved && matchesInput(saved)) return saved;
    throw error;
  }
  if (!validateDiscoveryReviewReceipt(raw) || !matchesInput(raw))
    throw new Error("discovery_review_receipt_invalid");
  return raw;
}

export async function getDiscoveryReview(request: ReviewRequest): Promise<ReviewMaterial> {
  const raw = await invoke<unknown>("onecrew_discovery_review_material_get", { input: {
    campaign_id: request.campaignId, expected_revision: request.revision, search_hit_id: request.hitId,
  } satisfies DiscoveryReviewMaterialInput });
  if (!validateDiscoveryReviewMaterial(raw)) throw new Error("discovery_review_invalid");
  const { native_content: rawNativeContent, ...saved } = raw;
  const material: ReviewMaterial = { ...saved,
    transcript: raw.transcript === null ? null : parseDiscoveryTranscript(raw.transcript),
    public_pages: raw.public_pages.map(parseDiscoveryPublicPage),
    ...(rawNativeContent == null ? {} : { native_content: parseDiscoveryContent(rawNativeContent) }),
  };
  const bound = (receipt: NonNullable<ReviewMaterial["transcript"]> | ReviewMaterial["public_pages"][number]) =>
    receipt.search_hit_id === request.hitId && receipt.candidate_id === material.candidate_id
    && receipt.profile_ref === request.profileRef && receipt.platform_account_id === material.native_author_id
    && receipt.source_url === material.hit.source_url;
  const native = material.native_content;
  const sourceFormat = (["google","yahoo"].includes(material.hit.platform) && material.format_version===2
      && "native_platform" in material
      && ((material.native_platform==="youtube"
        && /^UC[A-Za-z0-9_-]{22}$/.test(material.native_author_id)
        && material.hit.author_url===`https://www.youtube.com/channel/${material.native_author_id}`)
        || (material.native_platform==="twitter" && /^[1-9][0-9]{0,19}$/.test(material.native_author_id)
          && /^https:\/\/x\.com\/([A-Za-z0-9_]{1,15})\/status\/[1-9][0-9]{0,19}$/.test(material.hit.source_url)
          && material.hit.author_url===material.hit.source_url.split("/status/")[0].toLowerCase()
          && material.native_profile?.profile_url===material.hit.author_url)
        || (material.native_platform==="douyin"
          && /^MS4wLjABAAAA[A-Za-z0-9_-]{8,88}$/.test(material.native_author_id)
          && /^https:\/\/www\.douyin\.com\/video\/[1-9][0-9]{0,19}$/.test(material.hit.source_url)
          && material.hit.author_url===`https://www.douyin.com/user/${material.native_author_id}`
          && material.native_profile?.profile_url===material.hit.author_url))
      && material.hit.author_id===material.native_author_id
      && typeof material.video_description==="string" && material.video_description.trim().length>0
      && material.native_content==null)
    || ((additionalNativePlatforms as readonly string[]).includes(material.hit.platform) && material.format_version === 2
      && native?.status === "completed" && !native.stop_code && !!native.native_content
      && native.claim.campaign_id === request.campaignId && native.claim.search_hit_id === request.hitId
      && native.claim.expected_revision === request.revision && native.claim.platform === material.hit.platform
      && native.claim.profile_ref === request.profileRef && material.hit.author_id === material.native_author_id
      && !!material.hit.author_url && material.native_profile?.profile_url === material.hit.author_url
      && additionalNativeReviewBound(material.hit.platform, native.claim, native.native_content,
        material.native_author_id, material.hit.author_url, material.hit.source_url)
      && material.video_description === null && material.transcript === null)
    || (material.hit.platform === "douyin" && material.format_version === 2
      && native?.status === "completed" && !native.stop_code && !!native.native_content
      && "evidence_source" in native.native_content && native.native_content.evidence_source === "visible_native_video_caption"
      && native.claim.campaign_id === request.campaignId && native.claim.search_hit_id === request.hitId
      && native.claim.expected_revision === request.revision && native.claim.platform === "douyin"
      && native.claim.profile_ref === request.profileRef && native.claim.author_id === material.native_author_id
      && native.claim.source_url === material.hit.source_url
      && material.hit.author_id === material.native_author_id
      && material.hit.author_url === `https://www.douyin.com/user/${material.native_author_id}`
      && material.native_profile?.profile_url === material.hit.author_url
      && material.video_description === null && material.transcript === null)
    || (material.hit.platform === "youtube" && material.format_version === 1
      && native === undefined && material.native_followers === undefined)
    || (material.hit.platform === "twitter" && material.format_version === 2
      && native?.status === "completed" && !native.stop_code && !!native.native_content
      && "identity" in native.native_content && native.native_content.identity.account_id === material.native_author_id
      && native.claim.campaign_id === request.campaignId && native.claim.search_hit_id === request.hitId
      && native.claim.expected_revision === request.revision && native.claim.platform === "twitter"
      && native.claim.profile_ref === request.profileRef && native.claim.source_url === material.hit.source_url
      && isU64(material.native_followers)
      && material.video_description === null && material.transcript === null)
    || (material.hit.platform === "tiktok" && material.format_version === 2
      && native === undefined && isU64(material.native_followers)
      && material.hit.author_id === material.native_author_id && !!material.hit.author_url
      && typeof material.hit.author_observed_at === "number" && material.hit.author_observed_at > 0
      && /^https:\/\/www\.tiktok\.com\/@[^/]+\/video\/\d+$/.test(material.hit.source_url)
      && material.hit.title.trim().length > 0 && [...material.hit.title].length <= 500
      && material.video_description === null && material.transcript === null)
    || (material.hit.platform === "bilibili" && material.format_version === 2
      && native?.status === "completed" && !native.stop_code && !!native.native_content
      && "bvid" in native.native_content
      && native.native_content.author_id === material.native_author_id
      && native.native_content.source_url === material.hit.source_url
      && material.hit.source_url === `https://www.bilibili.com/video/${native.native_content.bvid}`
      && native.claim.campaign_id === request.campaignId && native.claim.search_hit_id === request.hitId
      && native.claim.expected_revision === request.revision && native.claim.platform === "bilibili"
      && native.claim.profile_ref === request.profileRef && native.claim.author_id === material.native_author_id
      && native.claim.source_url === material.hit.source_url
      && isU64(material.native_followers)
      && material.hit.author_id === material.native_author_id && !!material.hit.author_url
      && typeof material.hit.author_observed_at === "number" && material.hit.author_observed_at > 0
      && material.video_description === null && material.transcript === null);
  if (!sourceFormat || material.campaign_id !== request.campaignId
    || material.criteria_revision !== request.revision || material.hit.id !== request.hitId
    || material.hit.campaign_id !== request.campaignId || material.hit.profile_ref !== request.profileRef
    || !material.candidate_id || !material.native_author_id
    || !/^[a-f0-9]{64}$/.test(material.material_hash) || !/^[a-f0-9]{64}$/.test(material.candidate_evidence_hash)
    || !material.approved_scopes.some(scope => scope.platform === material.hit.platform && scope.profile_ref === request.profileRef
      && scope.keywords.includes(material.hit.search_keyword))
    || (material.transcript !== null && !bound(material.transcript))
    || material.public_pages.length > 20 || !material.public_pages.every(bound)
    || new Set(material.public_pages.map(page => page.requested_url)).size !== material.public_pages.length) {
    throw new Error("discovery_review_scope_changed");
  }
  return material;
}
