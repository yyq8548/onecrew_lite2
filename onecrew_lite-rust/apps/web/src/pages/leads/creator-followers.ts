import type { CreatorSummary, CreatorPlatform } from "@/lib/creator";

// Match the Creator list's final follower sort fallback independently of JSON
// property order. Primary and first-captured accounts still take precedence.
const FALLBACK_PLATFORMS: readonly CreatorPlatform[] = [
  "instagram", "telegram", "tiktok", "x", "youtube",
];

export function primaryFollowerEntry(
  summary: Pick<CreatorSummary, "follower_counts" | "primary_platform" | "platforms">,
): [CreatorPlatform, number] | null {
  const counts = summary.follower_counts;
  if (summary.primary_platform && counts[summary.primary_platform] != null) {
    return [summary.primary_platform, counts[summary.primary_platform]!];
  }
  for (const platform of summary.platforms) {
    if (counts[platform] != null) return [platform, counts[platform]!];
  }
  for (const platform of FALLBACK_PLATFORMS) {
    if (counts[platform] != null) return [platform, counts[platform]!];
  }
  return null;
}
