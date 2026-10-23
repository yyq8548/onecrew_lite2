import type { DatabaseSync } from "node:sqlite";
import { z } from "zod";
import { discoveryEligibleContactAuthor, loadDiscoveryNativeContactMaterial } from "./contact-native.ts";
import { storeDiscoveryContacts } from "./contact-repository.ts";

export const youtubeContactObservation = z.strictObject({
  campaign_id: z.string().min(1), expected_revision: z.number().int().positive(), candidate_id: z.string().min(1),
  profile_ref: z.string().min(1), channel_id: z.string().regex(/^UC[A-Za-z0-9_-]{22}$/u),
  email: z.email().max(254), captured_at: z.number().int().positive().safe(),
  source_excerpt: z.string().min(1).max(20000), method: z.literal("attended_browser"),
});
export function recordYoutubeContactObservation(database: DatabaseSync, raw: z.infer<typeof youtubeContactObservation>) {
  if (!database.isTransaction) throw new Error("youtube_contact_observation_requires_transaction");
  const input = youtubeContactObservation.parse(raw);
  if (input.captured_at > Math.floor(Date.now() / 1000) + 60 || !input.source_excerpt.includes(input.email)) throw new Error("youtube_contact_observation_invalid");
  const source = `https://www.youtube.com/channel/${input.channel_id}`;
  const author = discoveryEligibleContactAuthor(database, input.campaign_id, input.expected_revision, input.candidate_id);
  const material = loadDiscoveryNativeContactMaterial(database, { campaign_id: input.campaign_id, expected_revision: input.expected_revision, candidate_id: input.candidate_id, profile_ref: input.profile_ref, requested_url: source });
  if (author !== input.channel_id || material === null || (material.native_platform ?? material.hit.platform) !== "youtube") throw new Error("youtube_contact_observation_author_mismatch");
  const at = input.email.lastIndexOf("@"), email = input.email.slice(0, at) + "@" + input.email.slice(at + 1).toLowerCase();
  storeDiscoveryContacts(database, input.campaign_id, input.expected_revision, input.candidate_id, [{
    kind: "email", original_value: input.email, normalized_value: email, source_url: source,
    source_excerpt: `YouTube attended browser business email observation: ${input.source_excerpt}`,
    captured_at: input.captured_at, native_author_id: author, attribution: "author_published",
  }]);
  return { campaign_id: input.campaign_id, candidate_id: input.candidate_id, channel_id: author, email, source_url: source };
}
