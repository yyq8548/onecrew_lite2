import type {
  ContactChannel,
  CreatorContactRouteDeleteBlockingArtifact,
  CreatorContactRouteDeleteConfirmInput,
  CreatorContactRouteDeleteConfirmResult,
  CreatorContactRouteDeleteImpact,
  CreatorContactRouteDeletePrepareInput,
  CreatorContactRouteDeletePrepareResult,
  CreatorContactRouteRecord,
  ManualCreatorContactRouteAddInput,
  ManualCreatorContactRouteUpdateInput,
  ManualCreatorCreateInput,
} from "@/lib/creator";
import type { LeadsT } from "./i18n-labels";
import { creatorTagsFromDraft, type CreatorTagsDraft, type CreatorTagsValidationKey } from "./creator-tags-model";

export type ManualCreatorDraft = {
  displayName: string;
  notes: string;
  tags?: CreatorTagsDraft;
  contactRoutes?: Array<{ channel: ContactChannel; value: string; sourceUrl: string }>;
};

export type ManualContactRouteDraft = {
  creatorId: string;
  channel: ContactChannel;
  value: string;
  sourceUrl: string;
};

export type ManualContactRouteUpdateDraft = ManualContactRouteDraft & {
  contactRouteId: string;
};

export type ManualEntryValidationKey =
  | "contactRoutes.validation.creatorNameRequired"
  | "contactRoutes.validation.valueRequired"
  | CreatorTagsValidationKey;

export type DraftResult<T> =
  | { input: T; error: null }
  | { input: null; error: ManualEntryValidationKey };

export function manualCreatorInput(draft: ManualCreatorDraft): DraftResult<ManualCreatorCreateInput> {
  if (!draft.displayName.trim()) {
    return { input: null, error: "contactRoutes.validation.creatorNameRequired" };
  }
  const tags = creatorTagsFromDraft(draft.tags ?? { tags: [], text: "" });
  if (tags.error) return { input: null, error: tags.error };
  return {
    input: {
      display_name: draft.displayName,
      tags: tags.tags,
      notes: draft.notes.trim() || null,
      contact_routes: (draft.contactRoutes ?? [])
        .filter((route) => route.value.trim())
        .map(({ channel, value, sourceUrl }) => ({
          channel,
          value,
          source_url: sourceUrl.trim() || null,
        })),
    },
    error: null,
  };
}

export function manualContactRouteInput(
  draft: ManualContactRouteDraft,
): DraftResult<ManualCreatorContactRouteAddInput> {
  if (!draft.value.trim()) {
    return { input: null, error: "contactRoutes.validation.valueRequired" };
  }
  return {
    input: {
      creator_id: draft.creatorId,
      channel: draft.channel,
      value: draft.value,
      source_url: draft.sourceUrl.trim() || null,
    },
    error: null,
  };
}

export function contactRouteEditDraft(record: CreatorContactRouteRecord): ManualContactRouteUpdateDraft {
  const manualObservation = record.observations.find(({ source }) => source.kind === "manual_entry");
  return {
    creatorId: record.route.creator_id,
    contactRouteId: record.route.id,
    channel: record.locator.channel,
    value: manualObservation?.observation.observed_value ?? record.locator.display_value,
    sourceUrl: manualObservation?.source.source_url ?? "",
  };
}

export function manualContactRouteUpdateInput(
  draft: ManualContactRouteUpdateDraft,
): DraftResult<ManualCreatorContactRouteUpdateInput> {
  if (!draft.value.trim()) {
    return { input: null, error: "contactRoutes.validation.valueRequired" };
  }
  return {
    input: {
      creator_id: draft.creatorId,
      contact_route_id: draft.contactRouteId,
      channel: draft.channel,
      value: draft.value,
      source_url: draft.sourceUrl.trim() || null,
    },
    error: null,
  };
}

export function contactRouteDeletePrepareInput(
  record: CreatorContactRouteRecord,
): CreatorContactRouteDeletePrepareInput {
  return {
    creator_id: record.route.creator_id,
    contact_route_id: record.route.id,
  };
}

export function contactRouteDeleteConfirmInput(operationId: string): CreatorContactRouteDeleteConfirmInput {
  return { operation_id: operationId };
}

export function contactRouteDeleteDescription(record: CreatorContactRouteRecord, t: LeadsT): string {
  return t("contactRoutes.delete.description", { value: record.locator.display_value });
}

const DELETE_IMPACT_LABEL_KEYS = [
  ["contact_routes", "contactRoutes.delete.impact.contactRoutes"],
  ["email_threads", "contactRoutes.delete.impact.emailThreads"],
  ["email_messages", "contactRoutes.delete.impact.emailMessages"],
  ["email_drafts", "contactRoutes.delete.impact.emailDrafts"],
  ["email_recipients", "contactRoutes.delete.impact.emailRecipients"],
  ["direct_message_conversations", "contactRoutes.delete.impact.directMessageConversations"],
  ["direct_messages", "contactRoutes.delete.impact.directMessages"],
  ["outreach_touchpoints", "contactRoutes.delete.impact.outreachTouchpoints"],
  ["dependencies", "contactRoutes.delete.impact.dependencies"],
  ["contact_locators", "contactRoutes.delete.impact.contactLocators"],
  ["observation_sources", "contactRoutes.delete.impact.observationSources"],
  ["resolved_targets", "contactRoutes.delete.impact.resolvedTargets"],
  ["creator_origins_detached", "contactRoutes.delete.impact.creatorOriginsDetached"],
  ["target_resolutions", "contactRoutes.delete.impact.targetResolutions"],
  ["identity_conflicts", "contactRoutes.delete.impact.identityConflicts"],
  ["observations", "contactRoutes.delete.impact.observations"],
  ["audit_events", "contactRoutes.delete.impact.auditEvents"],
  ["idempotency_records", "contactRoutes.delete.impact.idempotencyRecords"],
] as const satisfies ReadonlyArray<[keyof CreatorContactRouteDeleteImpact, string]>;

export function contactRouteDeleteImpactItems(
  impact: CreatorContactRouteDeleteImpact,
  t: LeadsT,
) {
  return DELETE_IMPACT_LABEL_KEYS.map(([key, labelKey]) => ({
    key,
    label: t(labelKey),
    count: impact[key],
  }));
}

const BLOCKING_ARTIFACT_LABEL_KEYS = {
  in_flight_email: "contactRoutes.delete.blockingArtifacts.inFlightEmail",
  in_flight_direct_message: "contactRoutes.delete.blockingArtifacts.inFlightDirectMessage",
  in_flight_relationship_mutation: "contactRoutes.delete.blockingArtifacts.inFlightRelationshipMutation",
  ambiguous_direct_message_history: "contactRoutes.delete.blockingArtifacts.ambiguousDirectMessageHistory",
  shared_email_history: "contactRoutes.delete.blockingArtifacts.sharedEmailHistory",
  shared_direct_message_history: "contactRoutes.delete.blockingArtifacts.sharedDirectMessageHistory",
  shared_contact_locator: "contactRoutes.delete.blockingArtifacts.sharedContactLocator",
  ambiguous_local_audit_history: "contactRoutes.delete.blockingArtifacts.ambiguousLocalAuditHistory",
} as const satisfies Record<CreatorContactRouteDeleteBlockingArtifact["kind"], string>;

export function contactRouteDeleteBlockingArtifactLabel(
  kind: CreatorContactRouteDeleteBlockingArtifact["kind"],
  t: LeadsT,
) {
  return t(BLOCKING_ARTIFACT_LABEL_KEYS[kind]);
}

export function contactRouteDeleteBlockedMessage(
  reason: Extract<CreatorContactRouteDeletePrepareResult, { status: "blocked" }>["reason"],
  t: LeadsT,
) {
  if (reason === "has_derived_contact_routes") {
    return t("contactRoutes.delete.blockedReasons.derivedRoutes");
  }
  if (reason === "root_initial_platform_account") {
    return t("contactRoutes.delete.blockedReasons.rootAccount");
  }
  if (reason === "archived_creator") {
    return t("contactRoutes.delete.blockedReasons.archivedCreator");
  }
  return t("contactRoutes.delete.blockedReasons.dependencies");
}

export type ContactRouteDeleteRetryKey =
  | "contactRoutes.delete.retryExpired"
  | "contactRoutes.delete.retryStale";

export function contactRouteDeleteRetryKey(
  result: Extract<CreatorContactRouteDeleteConfirmResult, { status: "stale" | "expired" }>,
): ContactRouteDeleteRetryKey {
  return result.status === "expired"
    ? "contactRoutes.delete.retryExpired"
    : "contactRoutes.delete.retryStale";
}
