import type { DiscoveryCandidate } from "./opencli-discovery";

// These are navigation entries, not proof that Reddit permits a message.
export function redditManualContactLinks(candidate: DiscoveryCandidate) {
  const profile = candidate.profile_url;
  const post = candidate.evidence.source_urls[0];
  if (candidate.platform !== "reddit" || candidate.evidence.manual_profile_contact !== true
    || !/^https:\/\/www\.reddit\.com\/user\/[A-Za-z0-9_-]{3,20}\/$/.test(profile)
    || !/^https:\/\/www\.reddit\.com\/comments\/[a-z0-9]{1,15}\/$/.test(post ?? "")) return null;
  return { profile, post };
}
