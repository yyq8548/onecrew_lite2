import { containsDiscoveryCitation, type DiscoveryReviewMaterial } from "./review-material.ts";
import type { DiscoveryReviewFinding } from "./initial-match.ts";
import { requiredDiscoveryLanguage } from "./qualification.ts";

const object = (value: unknown): Record<string, unknown> => value !== null && typeof value === "object" && !Array.isArray(value) ? value as Record<string, unknown> : {};
const text = (value: string, maximum: number) => !!value.trim() && Buffer.byteLength(value) <= maximum;
export function applyDiscoveryReviewFindings(material: DiscoveryReviewMaterial, findings: readonly DiscoveryReviewFinding[], rawEvidence: unknown, reviewId: string): Record<string, unknown> {
  if (rawEvidence === null || typeof rawEvidence !== "object" || Array.isArray(rawEvidence)) throw new Error("review_evidence_invalid");
  const evidence = structuredClone(rawEvidence) as Record<string, unknown>, criteria = object(material.criteria), language = requiredDiscoveryLanguage(criteria);
  const proof = (policy: string) => ({ verified: true, native_author_id: material.native_author_id, source_url: material.hit.source_url, review_id: reviewId, material_hash: material.material_hash, policy_version: policy });
  const authorContent = (citation: DiscoveryReviewFinding["citations"][number]) => ["transcript", "native_content", "description"].includes(citation.kind) || material.hit.platform === "tiktok" && citation.kind === "search_caption";
  // LinkedIn职业资料和Instagram作品预览可证明匹配；语言、口播及演示仍须正文证据。
  const matchingContent = (citation: DiscoveryReviewFinding["citations"][number]) => authorContent(citation)
    || citation.kind === "native_profile" && (material.native_platform ?? material.hit.platform) === "linkedin"
    || citation.kind === "native_profile" && (material.native_platform ?? material.hit.platform) === "instagram"
      && material.native_profile?.web_text?.startsWith("Visible profile post previews; bounded excerpts, not verified full original captions.\n") === true
      && material.native_profile.web_text.slice(material.native_profile.web_text.indexOf("\n") + 1).includes(citation.quote);
  for (const criterion of ["content_language", "match_fit"] as const) {
    const selected = findings.filter(finding => finding.criterion === criterion);
    if (selected.length > 1 || criterion === "content_language" && language === null && selected.length) throw new Error("review_conditions_mismatch");
    const finding = selected[0]; if (!finding) continue;
    if (!text(finding.rationale, 4000) || finding.tool != null || finding.country_code != null || finding.citations.length > 8 || !finding.citations.every(citation => containsDiscoveryCitation(material, citation)) || finding.outcome !== "insufficient" && !finding.citations.some(criterion === "match_fit" ? matchingContent : authorContent)) throw new Error("review_first_hand_evidence_required");
    if (criterion === "content_language" && finding.outcome !== "insufficient" && language !== null && ["spoken", "speech", "口语", "口播", "语音"].some(word => language.toLowerCase().includes(word)) && !finding.citations.some(citation => citation.kind === "transcript")) throw new Error("review_transcript_required");
    if (criterion !== "match_fit" && finding.match_level != null || criterion === "match_fit" && (finding.outcome === "supported") !== (finding.match_level != null) || criterion === "match_fit" && finding.outcome === "contradicted") throw new Error("review_match_level_invalid");
    const key = criterion === "content_language" ? "reviewed_language" : "reviewed_match"; delete evidence[key];
    if (finding.outcome !== "insufficient") evidence[key] = { ...proof("language-match-review-v1"), language: criteria.language ?? null, outcome: finding.outcome, level: finding.match_level ?? null, reason: finding.rationale };
  }
  if (findings.some(finding => finding.criterion !== "match_fit" && finding.match_level != null)) throw new Error("review_match_level_invalid");
  delete evidence.own_ai_tool_demo; delete evidence.public_tool_url;
  for (const [criterion, key] of [["own_ai_tool_demo", "requires_ai_tool_demo"], ["public_tool_url", "requires_public_tool_url"]] as const) {
    const selected = findings.filter(finding => finding.criterion === criterion);
    if (selected.length !== Number(criteria[key] !== false)) throw new Error("review_conditions_mismatch");
    const finding = selected[0]; if (!finding) continue;
    if (finding.country_code != null || !text(finding.rationale, 4000) || finding.citations.length > 8 || !finding.citations.every(citation => containsDiscoveryCitation(material, citation)) || finding.outcome !== "insufficient" && !finding.citations.length) throw new Error("review_citation_invalid");
    if (finding.outcome !== "supported") { if (finding.tool != null) throw new Error("review_tool_unexpected"); continue; }
    const legacyProof = { verified: true, source_url: material.hit.source_url, review_id: reviewId, material_hash: material.material_hash, policy_version: "youtube-text-review-v1" };
    if (criterion === "own_ai_tool_demo") {
      if (finding.tool != null || !finding.citations.some(citation => citation.kind === "transcript")) throw new Error("review_first_hand_evidence_required");
      evidence.own_ai_tool_demo = { ...legacyProof, channel_id: material.native_author_id };
    } else {
      const tool = finding.tool; if (tool == null) throw new Error("review_tool_required");
      const page = material.public_pages.find(page => page.requested_url === tool.requested_url && page.status === "completed" && page.stop_code === null && page.source_excerpt.includes(tool.requested_url));
      if (!page) throw new Error("review_tool_source_unavailable");
      const observedName = page.title.toLowerCase().includes(tool.name.toLowerCase()) || finding.citations.some(citation => citation.kind === "public_page" && citation.requested_url === page.requested_url && citation.quote.toLowerCase().includes(tool.name.toLowerCase()));
      if (!text(tool.name, 256) || !observedName || !finding.citations.some(citation => citation.kind === "public_page" && citation.requested_url === page.requested_url)) throw new Error("review_tool_citation_required");
      evidence.public_tool_url = { ...legacyProof, tool_name: tool.name, url: page.requested_url };
    }
  }
  const apparelPolicy = ["twitter", "tiktok"].includes(material.hit.platform) ? "apparel-native-review-v2" : "apparel-residence-review-v1";
  for (const [criterion, required] of [["apparel_business_content", criteria.category === "服装"], ["category_content", typeof criteria.category === "string" && !!criteria.category && criteria.category !== "服装"], ["creator_residence_outside_mainland", criteria.region === "中国大陆以外"]] as const) {
    const selected = findings.filter(finding => finding.criterion === criterion);
    if (selected.length !== Number(required)) throw new Error("review_conditions_mismatch");
    const finding = selected[0]; if (!finding) continue;
    if (!text(finding.rationale, 4000) || finding.citations.length > 8 || !finding.citations.every(citation => containsDiscoveryCitation(material, citation)) || finding.outcome !== "insufficient" && !finding.citations.length || finding.tool != null) throw new Error("review_citation_invalid");
    const key = criterion === "creator_residence_outside_mainland" ? "reviewed_residence" : criterion === "apparel_business_content" ? "reviewed_apparel_content" : "reviewed_category_content";
    if (finding.outcome !== "supported") {
      if (finding.country_code != null) throw new Error("review_country_unexpected");
      if (finding.outcome === "contradicted" && (criterion === "creator_residence_outside_mainland" || object(evidence[key]).source_url === material.hit.source_url)) delete evidence[key]; continue;
    }
    if (criterion === "apparel_business_content") {
      if (finding.country_code != null || !finding.citations.some(citation => ["transcript", "native_content", "search_caption"].includes(citation.kind))) throw new Error("review_first_hand_evidence_required");
      evidence[key] = proof(apparelPolicy);
    } else if (criterion === "category_content") {
      if (finding.country_code != null || !finding.citations.some(matchingContent)) throw new Error("review_first_hand_evidence_required");
      evidence[key] = { ...proof("category-content-review-v1"), category: criteria.category };
    } else {
      const country = finding.country_code; if (country == null) throw new Error("review_country_required");
      if (!/^[A-Z]{2}$/u.test(country) || country === "CN") throw new Error("review_country_invalid");
      if (!finding.citations.some(citation => ["transcript", "native_content"].includes(citation.kind))) throw new Error("review_residence_statement_required");
      const pageCitation = finding.citations.find(citation => citation.kind === "public_page");
      if (pageCitation?.kind !== "public_page") throw new Error("review_residence_corroboration_required");
      const page = material.public_pages.find(page => page.requested_url === pageCitation.requested_url && page.status === "completed" && page.stop_code === null && page.source_excerpt.includes(pageCitation.requested_url));
      if (!page) throw new Error("review_residence_corroboration_unavailable");
      evidence[key] = { ...proof(apparelPolicy), country_code: country, corroboration_url: page.requested_url };
    }
  }
  if (!Array.isArray(evidence.source_urls)) throw new Error("review_evidence_invalid");
  if (!evidence.source_urls.includes(material.hit.source_url)) evidence.source_urls.push(material.hit.source_url);
  return evidence;
}
