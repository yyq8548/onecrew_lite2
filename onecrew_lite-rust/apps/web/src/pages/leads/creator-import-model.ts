import {
  PLATFORM_LABEL,
  type CreatorImportConflict,
  type CreatorImportImpact,
} from "@/lib/creator";
import type { LeadsT } from "./i18n-labels";

const IMPACT_LABEL_KEYS = [
  ["creators_created", "transfer.import.impact.creatorsCreated"],
  ["creators_merged_by_public_identity", "transfer.import.impact.creatorsMergedByPublicIdentity"],
  ["creator_scores_filled", "transfer.import.impact.creatorScoresFilled"],
  ["creator_tags_filled", "transfer.import.impact.creatorTagsFilled"],
  ["follower_counts_filled", "transfer.import.impact.followerCountsFilled"],
  ["platform_accounts_created", "transfer.import.impact.platformAccountsCreated"],
  ["claimed_links_created", "transfer.import.impact.claimedLinksCreated"],
  ["confirmed_links_created", "transfer.import.impact.confirmedLinksCreated"],
  ["rejected_links_created", "transfer.import.impact.rejectedLinksCreated"],
  ["links_promoted_to_confirmed", "transfer.import.impact.linksPromotedToConfirmed"],
  ["profile_observations_created", "transfer.import.impact.profileObservationsCreated"],
  ["contact_routes_created", "transfer.import.impact.contactRoutesCreated"],
  ["creators_unchanged", "transfer.import.impact.creatorsUnchanged"],
] as const satisfies ReadonlyArray<[keyof CreatorImportImpact, string]>;

const CONFLICT_LABEL_KEYS = {
  incoming_account_claimed_by_multiple_creators: "transfer.import.conflicts.duplicateIncomingAccount",
  account_owned_by_another_creator: "transfer.import.conflicts.accountOwnedElsewhere",
  possible_handle_match: "transfer.import.conflicts.possibleHandleMatch",
  rejected_link_would_be_revived: "transfer.import.conflicts.reviveRejectedLink",
  incoming_rejected_link_conflicts_with_active_local_link: "transfer.import.conflicts.rejectedLinkConflict",
  contact_route_owned_by_another_creator: "transfer.import.conflicts.contactRouteOwnedElsewhere",
  possible_contact_match: "transfer.import.conflicts.possibleContactMatch",
  incoming_creator_resolves_to_multiple_local_creators: "transfer.import.conflicts.creatorMatchesMultipleLocal",
  creator_tags_conflict: "transfer.import.conflicts.creatorTagsConflict",
} as const satisfies Record<CreatorImportConflict["kind"], string>;

export function creatorImportImpactRows(impact: CreatorImportImpact, t: LeadsT) {
  return IMPACT_LABEL_KEYS.flatMap(([key, labelKey]) =>
    impact[key] > 0 ? [{ key, label: t(labelKey), count: impact[key] }] : [],
  );
}

export function creatorImportConflictLabel(conflict: CreatorImportConflict, t: LeadsT) {
  return t(CONFLICT_LABEL_KEYS[conflict.kind]);
}

export function creatorImportConflictAccountLabel(conflict: CreatorImportConflict) {
  if (!conflict.account) return null;
  return `${PLATFORM_LABEL[conflict.account.platform]} · ${conflict.account.target_kind} · ${conflict.account.external_id}`;
}
