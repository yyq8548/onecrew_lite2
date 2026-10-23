import { creatorSavePreview, creatorSaveReview, type CreatorSaveReview } from "../results/creator-save-contracts.ts";
import { creatorSavePresentationProjection, type PresentationProjection } from "../results/contracts.ts";
import { candidateJson, type CandidateIntent } from "./candidate-contracts.ts";
import { boundedCandidateText, CandidateWarnings, type CreatorSavePreparedFailure } from "./candidate-materialize.ts";
import type { CandidatePreparedRow } from "./candidate-prepare.ts";
import { candidateSourceUrl } from "./candidate-provenance.ts";

export function projectCreatorCandidateReview(rows: readonly CandidatePreparedRow[], failures: readonly CreatorSavePreparedFailure[], intent: CandidateIntent, expiresAt: bigint, warnings: CandidateWarnings): { review: CreatorSaveReview; presentation: PresentationProjection } {
  const summary = { total: rows.length + failures.length, eligible_new: 0, eligible_update: 0, existing_skipped: 0, duplicate_skipped: 0, blocked: 0, failed: failures.length };
  const projected = rows.map(row => {
    const p = row.preview, identity = p.identity_resolution;
    summary[p.status === "identity_pending" || p.status === "contact_conflict" ? "blocked" : p.status]++;
    const priority = p.route_options.map((route, index) => ({ route, index })).sort((left, right) => (left.route.required ? 0 : left.route.sources.includes("link_hub") ? 2 : 1) - (right.route.required ? 0 : right.route.sources.includes("link_hub") ? 2 : 1));
    const order = priority.slice(0, 128); let dropped = order.length < priority.length;
    const preview = () => ({ display_name: boundedCandidateText(p.account.display_name), platform: p.account.platform, handle: p.account.handle, profile_url: p.account.profile_url, source_url: candidateSourceUrl(p.provenance), avatar_url: p.profile.avatar_url, identity_status: identity.status, identity_creator_ids: identity.status === "exact_existing_creator" ? [identity.creator_id] : identity.status === "new" ? [] : identity.creator_ids, intent: p.intent, status: p.status, reason: p.reason, notes: boundedCandidateText(p.notes), score: p.quality.score, assessment: boundedCandidateText(p.quality.assessment), follower_counts: Object.fromEntries(Object.entries(p.quality.follower_counts).sort(([a], [b]) => a < b ? -1 : a > b ? 1 : 0).map(([key, value]) => [key, BigInt(value)])), routes: [...order].sort((a, b) => a.index - b.index).map(({ route }) => structuredClone(route)), expires_at: expiresAt });
    let current = preview();
    while (Buffer.byteLength(candidateJson(current)) > 64 * 1024 && order.at(-1)?.route.required === false) { order.pop(); dropped = true; current = preview(); }
    if (dropped) warnings.add("Some optional contact routes are not shown because these Creators offered more than one review can carry.", boundedCandidateText(p.account.display_name) ?? p.row_id);
    return { row_id: p.row_id, preview: creatorSavePreview.parse(current) };
  });
  const review = creatorSaveReview.parse({ rows: projected, failures, intent, summary });
  const notices = warnings.render().map(body => ({ kind: "notice" as const, heading: "Contact verification", body, tone: "warning" as const }));
  const details = rows.map(({ preview: p }) => {
    const fields: { label: string; value: string }[] = [];
    for (const [label, raw] of [["Description", p.profile.description], ["Location", p.profile.location], ["Websites", p.profile.website_urls.join("\n")], ["Tags", p.tags === null ? null : p.tags.length ? p.tags.join(", ") : "Clear tags"]]) { const value = boundedCandidateText(raw ?? null); if (value) fields.push({ label: label!, value }); }
    const source = p.provenance;
    const sourceText = source.kind === "discovery" ? "Original discovery selection" : source.kind === "direct_selection" ? source.source_ref : source.kind === "provider_read" ? `${source.operation_id}: ${source.source_ref}` : `${source.community.title} (${source.community.external_id})`;
    fields.push({ label: "Selection source", value: boundedCandidateText(sourceText) ?? "Unnamed selection source" });
    return { kind: "fields" as const, heading: boundedCandidateText(p.account.display_name), fields };
  });
  return { review, presentation: creatorSavePresentationProjection.parse({ kind: "creator_save_batch_confirmation", title: "Review Creator save", summary: `Review ${review.rows.length} freshly read selections, choose contact routes, and confirm to save.`, sections: [...notices, ...details] }) };
}
