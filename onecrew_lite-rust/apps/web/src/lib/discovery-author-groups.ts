import { invoke } from "./local-runtime";
import type { DiscoveryIdentityGroupsSnapshot, GetDiscoveryIdentityGroupsInput } from "./generated/discovery-contracts";
import { validateDiscoveryIdentityGroupsSnapshot } from "./generated/discovery-contracts/validators/DiscoveryIdentityGroupsSnapshot.js";

export type AuthorGroupsRequest = { campaignId: string; revision: number };
export const authorGroupsKey = (input: AuthorGroupsRequest) =>
  ["discovery-author-groups", input.campaignId, input.revision] as const;

// Identity reviews currently have no runtime change event. Observe local data
// for the selected plan, with a bounded polling window and focus/manual refresh.
export const authorGroupsRefreshInterval = (observations: number, failed: boolean) =>
  !failed && observations < 12 ? 5000 : false;

export async function getDiscoveryAuthorGroups(input: AuthorGroupsRequest): Promise<DiscoveryIdentityGroupsSnapshot> {
  const raw = await invoke<unknown>("onecrew_discovery_identity_groups_get", { input: {
    campaign_id: input.campaignId, expected_revision: input.revision,
  } satisfies GetDiscoveryIdentityGroupsInput });
  if (!validateDiscoveryIdentityGroupsSnapshot(raw)
    || raw.campaign_id !== input.campaignId || raw.criteria_revision !== input.revision
    || !/^[a-f0-9]{64}$/.test(raw.snapshot_hash))
    throw new Error("discovery_author_groups_invalid");
  return raw;
}

// Qualification can stay unchanged while a review merges or splits authors.
// Only the owner's current, successfully read identity hash can trigger sync.
export function discoveryLibrarySyncKey(input: AuthorGroupsRequest & {
  usedCalls: number; visibleHits: number; candidateReviewVersion: string;
  groups: DiscoveryIdentityGroupsSnapshot | undefined;
  fetching: boolean; failed: boolean;
}): string | null {
  const groups = input.groups;
  if (input.visibleHits === 0 || input.fetching || input.failed || !groups
    || groups.campaign_id !== input.campaignId || groups.criteria_revision !== input.revision
    || !/^[a-f0-9]{64}$/.test(groups.snapshot_hash)) return null;
  return JSON.stringify([input.campaignId, input.revision, input.usedCalls,
    input.visibleHits, input.candidateReviewVersion, groups.snapshot_hash]);
}

export function authorProfileLink(value: string): string | null {
  try {
    const url = new URL(value);
    return url.protocol === "https:" && !url.username && !url.password ? url.href : null;
  } catch { return null; }
}
