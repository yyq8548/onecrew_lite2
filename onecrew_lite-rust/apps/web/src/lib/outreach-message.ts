import type {
  DirectMessageCapability,
  DirectMessagePreparation,
  CommitRelationshipActionResult,
} from "@/lib/creator";
import type { TFunction } from "i18next";

export type OutreachIdentitySource = "preferred" | "explicit" | "recent" | null;

export type FollowUiState = "checking" | "yes" | "no" | "unknown" | "updating" | "error";

export function followUiState({
  preparation,
  preparationFetching,
  preparationError,
  mutationPending,
  mutationError,
}: {
  preparation?: DirectMessagePreparation;
  preparationFetching: boolean;
  preparationError: unknown;
  mutationPending: boolean;
  mutationError: unknown;
}): FollowUiState {
  if (mutationPending) return "updating";
  if (mutationError || preparationError) return "error";
  if (preparationFetching || !preparation) return "checking";
  return preparation.relationship.following.value;
}

/** Patch the cached preparation after a follow the provider confirmed, so the
 * send gate reopens without a second preflight round trip.
 *
 * Only the two typed facts the follow actually settled are touched. `warnings`
 * is provider prose that no surface renders and that the next preflight
 * replaces; the earlier version filtered it by comparing against the runtime's
 * exact English sentence, which silently stopped matching whenever that
 * sentence was reworded. */
export function preparationAfterFollow(
  current: DirectMessagePreparation | undefined,
  recorded: Pick<CommitRelationshipActionResult, "relationship" | "can_message">,
): DirectMessagePreparation | undefined {
  if (!current) return current;
  return {
    ...current,
    relationship: { following: recorded.relationship, can_message: recorded.can_message },
    unfollowed_confirmation_required: recorded.relationship.value !== "yes",
  };
}

export function directMessageCapabilityFact(
  capability: DirectMessageCapability,
  platformLabel: string,
  t: TFunction<"inbox">,
): string | null {
  switch (capability.reason_code) {
    case "target_dm_blocking":
      return t("outreach.capability.targetDmBlocking", { platform: platformLabel });
    case "platform_dm_not_allowed":
      return t("outreach.capability.platformDmNotAllowed", { platform: platformLabel });
    case "operator_premium_required":
      return t("outreach.capability.premiumRequired", { platform: platformLabel });
    case "operator_chat_passcode_missing":
      return t("outreach.capability.chatPasscodeMissing", { platform: platformLabel });
    default:
      if (capability.state === "unknown") {
        return capability.detail
          ? t("outreach.capability.unknownWithDetail", { detail: capability.detail })
          : t("outreach.capability.unknown");
      }
      if (capability.state === "unavailable") {
        return capability.detail
          ? t("outreach.capability.unavailableWithDetail", {
              platform: platformLabel,
              detail: capability.detail,
            })
          : t("outreach.capability.unavailable", { platform: platformLabel });
      }
      return null;
  }
}
