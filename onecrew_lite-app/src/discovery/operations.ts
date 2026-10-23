import { z } from "zod";
import type { OperationRegistry } from "../application/operation-registry.ts";
import type { AccountContext, AccountContextService } from "../identity/account-context.ts";
import type { DiscoveryRepository } from "./repository.ts";
import type { DiscoveryRepositoryResolver } from "./search-execution.ts";
import { discoveryCandidatePageInput } from "./candidate-repository.ts";
import { prepareDiscoveryDraftInput } from "./draft.ts";
import type { DiscoveryDraftRuntime } from "./draft-runtime.ts";
import { discoveryPlanInput, discoverySourceTargets } from "./contracts.ts";
import { submitDiscoveryReviewInput } from "./review-submission.ts";
import { youtubeContactObservation } from "./youtube-contact-observation.ts";
import { discoveryWebAuthorResolution } from "./web-author-material.ts";

const text = z.string().min(1).refine(value => value.isWellFormed());
const material = z.strictObject({ campaign_id: text, expected_revision: z.number().int().min(1).max(4294967295), search_hit_id: text });
async function ownedOperation<T>(accounts: AccountContextService, resolver: DiscoveryRepositoryResolver, context: AccountContext, operation: (repository: DiscoveryRepository) => T): Promise<T> {
  accounts.assertCurrent(context); const owned = await resolver.resolve(context); owned.assertOwned(); accounts.assertCurrent(context);
  if (owned.owner.accountId !== context.principal.user.id || owned.owner.tenantId !== context.principal.tenantId || owned.owner.baseId !== context.principal.baseId) throw new Error("discovery_repository_owner_mismatch");
  const result = operation(owned.repository); owned.assertOwned(); accounts.assertCurrent(context); return result;
}
export function registerDiscoveryReadOperations(registry: OperationRegistry, accounts: AccountContextService, resolver: DiscoveryRepositoryResolver): void {
  const read = <T>(context: AccountContext, operation: (repository: DiscoveryRepository) => T) => ownedOperation(accounts, resolver, context, operation);
  registry.register("onecrew_discovery_campaign_get", z.strictObject({ id: text }), async ({ id }, context) => read(context, repository => repository.getPlan(id)));
  registry.register("onecrew_discovery_campaigns_recent", z.strictObject({}), async (_input, context) => read(context, repository => repository.recentPlans()));
  registry.register("onecrew_discovery_candidates_list", z.strictObject({ input: discoveryCandidatePageInput }), async ({ input }, context) => read(context, repository => repository.listCandidates(input)));
  registry.register("onecrew_discovery_search_hits_list", z.strictObject({ input: discoveryCandidatePageInput }), async ({ input }, context) => read(context, repository => repository.listSearchHits(input)));
  registry.register("onecrew_discovery_candidates_count", z.strictObject({ id: text }), async ({ id }, context) => read(context, repository => repository.countCandidates(id)));
  registry.register("onecrew_discovery_review_material_get", z.strictObject({ input: material }), async ({ input }, context) => read(context, repository => repository.prepareReviewMaterial(input.campaign_id, input.expected_revision, input.search_hit_id)));
  registry.register("onecrew_discovery_review_get", z.strictObject({ input: z.strictObject({ campaign_id: text, request_id: text }) }), async ({ input }, context) => read(context, repository => repository.getReview(context.principal.user.id, input.campaign_id, input.request_id)));
  registry.register("onecrew_discovery_profile_get", z.strictObject({ input: z.strictObject({ campaign_id: text, search_hit_id: text }) }), async ({ input }, context) => read(context, repository => repository.getProfile(input.campaign_id, input.search_hit_id)));
  registry.register("onecrew_discovery_content_get", z.strictObject({ input: z.strictObject({ campaign_id: text, search_hit_id: text }) }), async ({ input }, context) => read(context, repository => repository.getContent(input.campaign_id, input.search_hit_id)));
  registry.register("onecrew_discovery_identity_get", z.strictObject({ input: z.strictObject({ campaign_id: text, search_hit_id: text }) }), async ({ input }, context) => read(context, repository => repository.getIdentity(input.campaign_id, input.search_hit_id)));
  registry.register("onecrew_discovery_transcript_get", z.strictObject({ input: z.strictObject({ campaign_id: text, search_hit_id: text }) }), async ({ input }, context) => read(context, repository => repository.getTranscript(input.campaign_id, input.search_hit_id)));
  registry.register("onecrew_discovery_public_page_get", z.strictObject({ input: z.strictObject({ campaign_id: text, candidate_id: text, requested_url: text }) }), async ({ input }, context) => read(context, repository => repository.getPublicPage(input.campaign_id, input.candidate_id, input.requested_url)));
}
export function registerDiscoveryWriteOperations(registry: OperationRegistry, accounts: AccountContextService, resolver: DiscoveryRepositoryResolver): void {
  const write = <T>(context: AccountContext, operation: (repository: DiscoveryRepository) => T) => ownedOperation(accounts, resolver, context, operation);
  const revision = z.number().int().min(1).max(4294967295), count = z.number().int().min(1).max(10000);
  registry.register("onecrew_discovery_campaign_create", z.strictObject({ input: discoveryPlanInput }), async ({ input }, context) => write(context, repository => { repository.create({ ...input, criteria: { contact_preference: "public_email_first", requires_ai_tool_demo: false, requires_public_tool_url: false, ...input.criteria } }); return repository.getPlan(input.id); }));
  registry.register("onecrew_discovery_campaign_approve", z.strictObject({ id: text, expectedRevision: revision }), async ({ id, expectedRevision }, context) => write(context, repository => { repository.approve(id, expectedRevision); return repository.getPlan(id); }));
  registry.register("onecrew_discovery_source_targets_set", z.strictObject({ input: z.strictObject({ campaign_id: text, expected_revision: revision, source_targets: discoverySourceTargets }) }), async ({ input }, context) => write(context, repository => { repository.setSourceTargets(input.campaign_id, input.expected_revision, input.source_targets); return { campaign: repository.getPlan(input.campaign_id), source_progress: repository.sourceProgress(input.campaign_id) }; }));
  registry.register("onecrew_discovery_target_increase", z.strictObject({ input: z.strictObject({ campaign_id: text, expected_revision: revision, expected_target_count: z.number().int().min(1).max(4294967295), new_target_count: count, decision_id: text }) }), async ({ input }, context) => write(context, repository => { repository.adjustTarget(context.principal.user.id, { decisionId: input.decision_id, planId: input.campaign_id, expectedRevision: input.expected_revision, expectedTargetCount: input.expected_target_count, newTargetCount: input.new_target_count }); return repository.getPlan(input.campaign_id); }));
  registry.register("onecrew_discovery_review_submit", z.strictObject({ input: submitDiscoveryReviewInput }), async ({ input }, context) => write(context, repository => repository.submitReview(context.principal.user.id, input)));
  registry.register("onecrew_discovery_web_author_resolve", z.strictObject({ input: material.extend({ material_hash: text.regex(/^[a-f0-9]{64}$/u), resolution: discoveryWebAuthorResolution }) }), async ({ input }, context) => write(context, repository => repository.resolveWebAuthor(input.campaign_id, input.expected_revision, input.search_hit_id, input.material_hash, input.resolution)));
  registry.register("onecrew_discovery_youtube_contact_observation_record", z.strictObject({ input: youtubeContactObservation }), async ({ input }, context) => write(context, repository => repository.recordYoutubeContactObservation(input)));
}
export function registerDiscoveryDraftOperations(registry: OperationRegistry, runtime: DiscoveryDraftRuntime): void {
  registry.register("onecrew_discovery_draft_prepare", z.strictObject({ input: prepareDiscoveryDraftInput }), async ({ input }, context) => runtime.launch(context, input));
  registry.register("onecrew_discovery_draft_progress_get", z.strictObject({ id: text }), async ({ id }, context) => runtime.progress(context, id));
}
