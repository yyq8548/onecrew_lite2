import { z } from "zod";
import { discoveryReviewCriterion } from "@onecrew/runtime-contracts/discovery";
import { matchedDiscoveryLanguageSource, type DiscoveryReviewMaterial } from "./review-material.ts";

export const INITIAL_MATCH_REVIEWER = "onecrew-match-evaluation-0cebf122-initial-v1";
const text = z.string().refine(value => value.isWellFormed());
const quote = z.discriminatedUnion("kind", [
  z.strictObject({ kind: z.literal("description"), quote: text }), z.strictObject({ kind: z.literal("search_caption"), quote: text }), z.strictObject({ kind: z.literal("native_content"), quote: text }),
  z.strictObject({ kind: z.literal("transcript"), segment_index: z.number().int().min(0).max(4294967295), quote: text }),
  z.strictObject({ kind: z.literal("native_profile"), profile_url: text, quote: text }), z.strictObject({ kind: z.literal("public_page"), requested_url: text, quote: text }),
]);
export const discoveryReviewFinding = z.strictObject({ criterion: discoveryReviewCriterion, outcome: z.enum(["supported", "contradicted", "insufficient"]), rationale: text, citations: z.array(quote), tool: z.strictObject({ name: text, requested_url: text }).nullish(), country_code: text.nullish(), match_level: z.enum(["excellent", "good", "medium", "low"]).nullish() });
export type DiscoveryReviewFinding = z.infer<typeof discoveryReviewFinding>;
const object = (value: unknown): Record<string, unknown> => value !== null && typeof value === "object" && !Array.isArray(value) ? value as Record<string, unknown> : {};

export function validateAudienceLanguage(material: DiscoveryReviewMaterial, finding: DiscoveryReviewFinding, selectionOnly = false) {
  if (finding.outcome !== "supported" || !selectionOnly && finding.match_level === "low") return;
  const samples = material.language_samples ?? [], native = object(material.native_content?.native_content), original = native[material.hit.platform === "bilibili" ? "description" : "text"];
  const audienceClaim = finding.rationale.includes("粉丝评论语言符合") || finding.rationale.includes("Audience comment language matches");
  const commentOnly = finding.citations.some(citation => citation.kind === "native_content" && samples.some(sample => sample.comments.some(comment => comment.text.includes(citation.quote))) && !samples.some(sample => sample.text.includes(citation.quote)) && !(typeof original === "string" && original.includes(citation.quote)));
  if (!audienceClaim && !commentOnly) return;
  const language = object(material.criteria).language;
  if (typeof language === "string" && ["spoken", "speech", "口语", "口播", "语音"].some(word => language.toLowerCase().includes(word)) || !audienceClaim || !selectionOnly && finding.match_level !== "good") throw new Error("initial_match_audience_language_invalid");
  const source = matchedDiscoveryLanguageSource(material);
  const enough = samples.some(sample => {
    if (sample.author_id !== material.native_author_id || sample.source_url !== source) return false;
    const texts = new Set<string>(), sources = new Set<string>(); let cited = 0;
    for (const comment of sample.comments.slice(0, 5)) {
      const value = comment.text.trim();
      if ([...value].filter(character => /\p{Alphabetic}/u.test(character)).length < 3 || value.startsWith("https://") || value.startsWith("http://")) continue;
      const normalized = value.toLowerCase(); if (texts.has(normalized)) continue; texts.add(normalized);
      if (sources.has(comment.source_url)) continue; sources.add(comment.source_url);
      if (finding.citations.some(citation => citation.kind === "native_content" && citation.quote.trim() === value)) cited++;
    }
    return cited >= 3;
  });
  if (!enough) throw new Error("initial_match_audience_sample_insufficient");
}
