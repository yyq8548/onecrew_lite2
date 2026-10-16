import { z } from "zod";
import { isLosslessNumber, isSafeNumber } from "lossless-json";
import { channel, confidence, followers, integer as safeInteger, locatorKind, platform, sourceKind, stage, tags, text } from "./contracts.ts";
import type { CandidateBatchResult } from "./candidate-execute.ts";

const nullable = text.nullable();
const integer = z.preprocess(value => isLosslessNumber(value) && isSafeNumber(value.toString()) ? Number(value.toString()) : value, safeInteger);
const timestamps = { created_at: integer, updated_at: integer };
const target = z.enum(["user", "bot", "channel", "supergroup", "group", "creator", "business"]);
const source = z.object({ id: text, kind: sourceKind, platform: platform.nullable(), source_url: nullable, external_ref: nullable, account_ref_id: nullable, metadata: z.unknown(), observed_at: integer });
const route = z.object({ id: text, creator_id: text, locator_id: text, confidence, ...timestamps });
const observation = z.object({ id: text, locator_id: text, source_id: text, creator_id: nullable, platform_account_id: nullable, observed_value: text, observed_at: integer });
const administratorSource = z.discriminatedUnion("kind", [z.object({ kind: z.literal("operator_account"), operator_account_ref_id: text }), z.object({ kind: z.literal("creator_contact_route"), source_creator_id: text, source_contact_route_id: nullable, operator_account_ref_id: text, source_resolved_target_id: text })]);
const originBase = { id: text, creator_id: text, source_id: text };
const origin = z.discriminatedUnion("kind", [
  z.object({ ...originBase, kind: z.literal("manual_entry"), created_at: integer }),
  z.object({ ...originBase, kind: z.literal("creator_import"), content_sha256: text, imported_creator_id: text, imported_at: integer }),
  z.object({ ...originBase, kind: z.literal("platform_search"), discovery_id: text, discovered_at: integer, account_ref_id: text }),
  z.object({ ...originBase, kind: z.literal("community_administrator"), discovery_id: text, discovered_at: integer, role: z.enum(["owner", "administrator"]), community: z.object({ external_id: text, title: text, handle: nullable, target_kind: z.enum(["channel", "supergroup"]) }), source: administratorSource }),
]);
export const candidateCreatorRecord = z.object({
  creator: z.object({ id: text, display_name: text, notes: nullable, score: z.preprocess(value => isLosslessNumber(value) && isSafeNumber(value.toString()) ? Number(value.toString()) : value, safeInteger.max(100)).nullable(), assessment: nullable, follower_counts: followers, tags, archived_at: integer.nullable(), pinned_at: integer.nullable(), ...timestamps }),
  group: z.object({ id: text, name: text }).nullable(),
  relationship: z.object({ creator_id: text, stage, last_touch_at: integer.nullable(), ...timestamps }),
  platform_accounts: z.array(z.object({ id: text, platform, target_kind: target, external_id: text, ...timestamps })),
  account_links: z.array(z.object({ id: text, creator_id: text, platform_account_id: text, state: z.enum(["claimed", "confirmed", "rejected"]), confidence, ...timestamps })),
  account_observations: z.array(z.object({ id: text, platform_account_id: text, source_id: text, handle: nullable, display_name: nullable, profile_url: nullable, public_data: z.unknown(), observed_at: integer })),
  observation_sources: z.array(source), origins: z.array(origin),
  contact_routes: z.array(z.object({ route, locator: z.object({ id: text, channel, kind: locatorKind, normalized_value: text, display_value: text, ...timestamps }), observations: z.array(z.object({ observation, source })), allowed_actions: z.array(z.enum(["update", "delete"])) })),
});
const receipt = z.object({ outcomes: z.array(z.discriminatedUnion("status", [
  z.object({ status: z.literal("saved"), row_id: text, creator: candidateCreatorRecord, created: z.boolean(), outreach_context: z.object({ creator_id: text, contact_route_id: text, operator_account_ref_id: text }).nullable() }),
  z.object({ status: z.literal("skipped"), row_id: text, reason: z.enum(["existing_creator", "duplicate_selection"]) }),
  z.object({ status: z.literal("failed"), row_id: text, error: text }),
])) });
export function decodeCandidateReceipt(value: unknown): CandidateBatchResult { return receipt.parse(value); }
