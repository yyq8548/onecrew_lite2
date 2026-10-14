import { invoke, isRuntimeAvailable, runtimeRequest } from "@/lib/local-runtime";

import { appI18n } from "@/i18n";
import zhCNCommon from "@/i18n/locales/zh-CN/common";
import { DEFAULT_LOCALE } from "@/i18n/resources";

export type CreatorPlatform = "x" | "instagram" | "telegram" | "youtube" | "tiktok";
export type CreatorStage = "prospect" | "engaged" | "collaborating" | "archived";
export type TargetKind = "user" | "bot" | "channel" | "supergroup" | "group" | "creator" | "business";
export type ContactChannel = "email" | "wechat" | "x" | "instagram" | "telegram" | "youtube" | "tiktok" | "website" | "link_hub";

/** Canonical cache entry for Creator-global relationship and touchpoint facts. */
export const CREATORS_QUERY_KEY = ["creator-creators"] as const;
export const CONNECTED_ACCOUNTS_QUERY_KEY = ["connected-accounts"] as const;
export const RUNTIME_UNAVAILABLE_MESSAGE = zhCNCommon.runtimeUnavailable.description;
export type LocatorKind = "email_address" | "handle" | "url" | "invite_link" | "channel_id";
export type SourceKind =
  | "initial_platform_account"
  | "profile_bio"
  | "profile_website"
  | "profile_location"
  | "video_description"
  | "channel_description"
  | "link_hub"
  | "manual_entry"
  | "manual_business_email"
  | "community_administrator"
  | "contact_route_resolution"
  | "creator_import";
export type CreatorContactRouteAction = "update" | "delete";
export type Confidence = "exact" | "asserted" | "possible";
export type ResolutionState = "active" | "invalidated";
export type TargetResolutionStatus =
  | "resolved"
  | "requires_operator_auth"
  | "browser_only"
  | "unsupported"
  | "not_found"
  | "conflict";
export type ContactAction =
  | "open_profile"
  | "read_public"
  | "follow"
  | "direct_message"
  | "send_email"
  | "open_community"
  | "extract_administrators";

export type ConflictState = "open" | "resolved" | "dismissed";
export type RelationshipKind = "following" | "followed_by" | "blocked" | "can_message";
export type RelationshipValue = "yes" | "no" | "unknown";
export type RelationshipMutation = "follow" | "unfollow";
export type DirectMessageCapabilityState = "allowed" | "unavailable" | "unknown";
export type DirectMessageCapabilityScope = "existing_conversation" | "new_message_request" | "unknown";
export type DirectMessageCapabilityReasonCode =
  | "target_dm_blocking"
  | "platform_dm_not_allowed"
  | "operator_premium_required"
  | "operator_chat_passcode_missing";
export type CommunityAdministratorRole = "owner" | "administrator";
export type CommunityTargetKind = "channel" | "supergroup";
export type EmailCapability = "send" | "sent_readback" | "inbox" | "reply";
export type EmailDirection = "inbound" | "outbound";
export type EmailFolder = "all" | "inbox" | "sent" | "drafts" | "archive" | "deleted";
export type EmailThreadFolder = Exclude<EmailFolder, "all" | "drafts">;
export type EmailThreadDispositionAction = "archive" | "unarchive" | "restore" | "delete";

/** One cursor page of a Creator list; a null/absent next_cursor ends the list. */
export type Page<T> = {
  items: T[];
  next_cursor?: string | null;
};

/** Bounded list row for `onecrew_creator_list`; full facts come from `onecrew_creator_get`. */
export type CreatorSummary = {
  id: string;
  display_name: string;
  tags: string[];
  stage: CreatorStage;
  primary_platform?: CreatorPlatform | null;
  primary_handle?: string | null;
  avatar_url?: string | null;
  platforms: CreatorPlatform[];
  channels: ContactChannel[];
  has_manual_origin: boolean;
  /** Agent quality score 0-100; empty for manually entered creators. */
  score?: number | null;
  /** Agent explanation of the score, shown on hover. */
  assessment?: string | null;
  /** Latest known follower/subscriber count per platform. */
  follower_counts: Partial<Record<CreatorPlatform, number>>;
  last_touch_at?: number | null;
  pinned_at?: number | null;
  platform_account_count: number;
  contact_route_count: number;
  archived_at?: number | null;
  group: CreatorGroupRef | null;
  created_at: number;
  updated_at: number;
};

export type CreatorGroup = {
  id: string;
  name: string;
  revision: number;
  position: number;
  created_at: number;
  updated_at: number;
};

export type CreatorGroupRef = {
  id: string;
  name: string;
};

export type CreatorGroupSummary = {
  group: CreatorGroup;
  creator_count: number;
};

export type CreatorGroupFilter =
  | { kind: "uncategorized" }
  | { kind: "group"; group_id: string };

export type CreatorGroupDestination = CreatorGroupFilter;

export type ListCreatorGroupsInput = {
  limit?: number | null;
  cursor?: string | null;
  query?: string | null;
  include_archived?: boolean;
};

export type CreatorGroupListResult = {
  items: CreatorGroupSummary[];
  uncategorized_count: number;
  next_cursor?: string | null;
};

export type CreateCreatorGroupInput = { name: string };

export type UpdateCreatorGroupInput = {
  group_id: string;
  expected_revision: number;
  name: string;
};

export type DeleteCreatorGroupInput = {
  group_id: string;
  expected_revision: number;
};

export type DeleteCreatorGroupResult = {
  group_id: string;
  uncategorized_creator_count: number;
};

export type SetCreatorGroupInput = {
  creator_id: string;
  destination: CreatorGroupDestination;
};

export type BulkMoveCreatorsToGroupInput = {
  creator_ids: string[];
  destination: CreatorGroupDestination;
};

export type CreatorGroupAssignment = {
  creator_id: string;
  group: CreatorGroupRef | null;
};

export type MoveCreatorsToGroupResult = {
  assignments: CreatorGroupAssignment[];
  moved_count: number;
  unchanged_count: number;
};

/** Archived creators are excluded unless `include_archived`; default limit 25, max 100. */
export type ListCreatorsInput = {
  limit?: number | null;
  cursor?: string | null;
  query?: string | null;
  stages?: CreatorStage[];
  primary_platforms?: CreatorPlatform[];
  last_touch_range?: CreatorTimestampRange | null;
  updated_range?: CreatorTimestampRange | null;
  sorting?: CreatorListSort[];
  include_facets?: boolean;
  group?: CreatorGroupFilter | null;
  include_archived?: boolean;
};

export type CreatorTimestampRange = {
  from?: number | null;
  to_exclusive?: number | null;
};

export type CreatorListSort = {
  field: "display_name" | "followers" | "score" | "last_touch_at" | "updated_at" | "created_at";
  direction: "asc" | "desc";
};

export type CreatorListFacets = {
  total: number;
  stages: { value: CreatorStage; count: number }[];
  primary_platforms: { value: CreatorPlatform; count: number }[];
};

export type CreatorListPage = {
  items: CreatorSummary[];
  next_cursor: string | null;
  facets: CreatorListFacets | null;
};

export type CreatorExportResult = {
  destination: "system_desktop";
  format: CreatorTransferFileFormat;
  file_name: string;
  creator_count: number;
  content_sha256: string;
  reused_existing: boolean;
};

export type CreatorTransferFileFormat = "xlsx";

export type CreatorImportImpact = {
  creator_inputs: number;
  creators_created: number;
  creators_merged_by_public_identity: number;
  creator_scores_filled: number;
  follower_counts_filled: number;
  platform_accounts_created: number;
  claimed_links_created: number;
  confirmed_links_created: number;
  rejected_links_created: number;
  links_promoted_to_confirmed: number;
  profile_observations_created: number;
  contact_routes_created: number;
  creators_unchanged: number;
  creator_tags_filled: number;
};

export type CreatorImportReceipt = {
  content_sha256: string;
  format_version: number;
  creator_count: number;
  impact: CreatorImportImpact;
  imported_at: number;
};

export type CreatorImportConflict = {
  kind:
    | "incoming_account_claimed_by_multiple_creators"
    | "account_owned_by_another_creator"
    | "possible_handle_match"
    | "rejected_link_would_be_revived"
    | "incoming_rejected_link_conflicts_with_active_local_link"
    | "contact_route_owned_by_another_creator"
    | "possible_contact_match"
    | "incoming_creator_resolves_to_multiple_local_creators"
    | "creator_tags_conflict";
  incoming_creator_key: string;
  account?: {
    platform: CreatorPlatform;
    target_kind: TargetKind;
    external_id: string;
  } | null;
  local_creator_ids: string[];
};

/** Portable Creator transfer DTO mirror. These keys are bundle-local facts,
 * not receiver database identifiers. */
export type CreatorTransferCreator = {
  semantic_sha256: string;
  record_key: string;
  display_name: string;
  tags: string[];
  score: number | null;
  follower_counts: Partial<Record<CreatorPlatform, number>>;
  pinned: boolean;
  relationship: { stage: CreatorStage };
  accounts: CreatorTransferAccount[];
  contact_routes: CreatorTransferContactRoute[];
};

export type CreatorTransferAccount = {
  capture_order: number;
  platform: CreatorPlatform;
  target_kind: TargetKind;
  external_id: string;
  link_state: "claimed" | "confirmed" | "rejected";
  confidence: Confidence;
  source_kind: SourceKind;
  latest_profile: {
    handle: string | null;
    display_name: string | null;
    profile_url: string | null;
    avatar_url: string | null;
  } | null;
};

export type CreatorTransferContactRoute = {
  save_order: number;
  channel: ContactChannel;
  locator_kind: LocatorKind;
  normalized_value: string;
  display_value: string;
  confidence: Confidence;
  source_kinds: SourceKind[];
};

export type CreatorImportFilePrepareResult =
  | { status: "cancelled" }
  | { status: "already_imported"; file_name: string; receipt: CreatorImportReceipt }
  | {
      status: "blocked";
      file_name: string;
      content_sha256: string;
      conflict_count: number;
      conflicts: CreatorImportConflict[];
    }
  | {
      status: "confirmation_required";
      file_name: string;
      preview_rows: Array<{record_key: string; display_name: string; notes: string | null; contact_routes: CreatorTransferContactRoute[]}>;
      operation_id: string;
      content_sha256: string;
      impact: CreatorImportImpact;
      expires_at: number;
    };

export type ConfirmCreatorImportResult =
  | { status: "imported"; operation_id: string; receipt: CreatorImportReceipt }
  | { status: "unchanged"; operation_id: string; receipt: CreatorImportReceipt }
  | { status: "stale"; operation_id: string; next_action: string }
  | { status: "expired"; operation_id: string; next_action: string }
  | { status: "cancelled"; operation_id: string };

export type CommunityAdministratorSource =
  | { kind: "operator_account"; operator_account_ref_id: string }
  | { kind: "creator_contact_route"; source_creator_id: string; source_contact_route_id: string | null; operator_account_ref_id: string; source_resolved_target_id: string };

export type CommunityReference = {
  external_id: string;
  title: string;
  handle?: string | null;
  target_kind: CommunityTargetKind;
};

/** How a Creator entered the workbench: an agent platform search through an
 * operating account, a Telegram community administrator extraction, a human
 * manual entry, or a portable import. */
export type CreatorOrigin =
  | {
    kind: "platform_search";
    id: string;
    discovery_id: string;
    account_ref_id: string;
    creator_id: string;
    source_id: string;
    discovered_at: number;
  }
  | {
    kind: "community_administrator";
    id: string;
    discovery_id: string;
    creator_id: string;
    source_id: string;
    role: CommunityAdministratorRole;
    community: CommunityReference;
    source: CommunityAdministratorSource;
    discovered_at: number;
  }
  | {
    kind: "manual_entry";
    id: string;
    creator_id: string;
    source_id: string;
    created_at: number;
  }
  | {
    kind: "creator_import";
    id: string;
    creator_id: string;
    source_id: string;
    content_sha256: string;
    imported_creator_id: string;
    imported_at: number;
  };

export const PLATFORM_LABEL: Record<CreatorPlatform, string> = {
  x: "X",
  instagram: "Instagram",
  telegram: "Telegram",
  youtube: "YouTube",
  tiktok: "TikTok",
};

export type ContactLocatorInput = {
  channel: ContactChannel;
  kind: LocatorKind;
  normalized_value: string;
  display_value: string;
};

export type Creator = {
  id: string;
  display_name: string;
  tags: string[];
  notes?: string | null;
  /** Agent quality score 0-100; empty for manually entered creators. */
  score?: number | null;
  /** Agent explanation of the score. */
  assessment?: string | null;
  /** Latest known follower/subscriber count per platform. */
  follower_counts: Partial<Record<CreatorPlatform, number>>;
  created_at: number;
  updated_at: number;
  archived_at?: number | null;
};

/** Direct patch of one creator's own facts; `follower_counts` merges per platform. */
export type UpdateCreatorInput = {
  creator_id: string;
  /** Omission leaves tags unchanged; [] clears; provided values replace. */
  tags?: string[] | null;
  display_name?: string | null;
  notes?: string | null;
  score?: number | null;
  assessment?: string | null;
  follower_counts?: Partial<Record<CreatorPlatform, number>> | null;
};

export type CreatorRelationship = {
  creator_id: string;
  stage: CreatorStage;
  last_touch_at?: number | null;
  pinned_at?: number | null;
  created_at: number;
  updated_at: number;
};

export type PlatformAccount = {
  id: string;
  platform: CreatorPlatform;
  target_kind: TargetKind;
  external_id: string;
  created_at: number;
  updated_at: number;
};

export type PlatformAccountObservation = {
  id: string;
  platform_account_id: string;
  source_id: string;
  handle?: string | null;
  display_name?: string | null;
  profile_url?: string | null;
  public_data: Record<string, unknown>;
  observed_at: number;
};

export type ObservationSource = {
  id: string;
  kind: SourceKind;
  platform?: CreatorPlatform | null;
  source_url?: string | null;
  external_ref?: string | null;
  account_ref_id?: string | null;
  metadata: Record<string, unknown>;
  observed_at: number;
};

export type ContactLocator = ContactLocatorInput & {
  id: string;
  created_at: number;
  updated_at: number;
};

export type CreatorContactRoute = {
  id: string;
  creator_id: string;
  locator_id: string;
  confidence: Confidence;
  created_at: number;
  updated_at: number;
};

export type LocatorObservation = {
  id: string;
  locator_id: string;
  source_id: string;
  creator_id?: string | null;
  platform_account_id?: string | null;
  observed_value: string;
  observed_at: number;
};

export type ContactRouteObservationRecord = {
  observation: LocatorObservation;
  source: ObservationSource;
};

export type CreatorContactRouteRecord = {
  route: CreatorContactRoute;
  locator: ContactLocator;
  observations: ContactRouteObservationRecord[];
  allowed_actions: CreatorContactRouteAction[];
};

export type CreatorRecord = {
  creator: Creator;
  group: CreatorGroupRef | null;
  relationship: CreatorRelationship;
  platform_accounts: PlatformAccount[];
  account_links: Array<{
    id: string;
    creator_id: string;
    platform_account_id: string;
    state: "claimed" | "confirmed" | "rejected";
    confidence: Confidence;
    created_at: number;
    updated_at: number;
  }>;
  account_observations: PlatformAccountObservation[];
  observation_sources: ObservationSource[];
  origins: CreatorOrigin[];
  contact_routes: CreatorContactRouteRecord[];
};

export function emailContactRoutes(record: CreatorRecord | null | undefined) {
  return record?.contact_routes.filter((route) => route.locator.channel === "email") ?? [];
}

export type MailboxIssue = "unavailable" | "recorded_failure";

export type MailboxDetail = {
  mailbox_ref_id: string;
  provider: "resend" | "gmail";
  label: string;
  email_address: string;
  display_name?: string | null;
  capabilities: EmailCapability[];
  available: boolean;
  issues: MailboxIssue[];
  created_at: number;
  updated_at: number;
};

export type EmailThread = {
  id: string;
  mailbox_ref_id: string;
  creator_id?: string | null;
  contact_route_id?: string | null;
  provider_thread_key: string;
  subject: string;
  unread_count: number;
  folder: EmailThreadFolder;
  created_at: number;
  updated_at: number;
};

export type EmailMessageView = {
  id: string;
  thread_id: string;
  provider_message_id: string;
  internet_message_id?: string | null;
  direction: EmailDirection;
  from: string;
  to: string[];
  cc: string[];
  reply_to: string[];
  in_reply_to?: string | null;
  references: string[];
  subject: string;
  text?: string | null;
  html?: string | null;
  delivery_status: string;
  occurred_at: number;
};

export type EmailMessagePreview = {
  id: string;
  direction: EmailDirection;
  from: string;
  to: string[];
  reply_to: string[];
  subject: string;
  preview_text?: string | null;
  has_html: boolean;
  delivery_status: string;
  occurred_at: number;
};

export type EmailThreadSummary = {
  thread: EmailThread;
  latest_message?: EmailMessagePreview | null;
  message_count: number;
  has_outbound: boolean;
  creator_display_name?: string | null;
  /** Agent 对本线程最新一条来信的判读,没判读过就没有。 */
  triage?: ReplyTriageBrief | null;
};

export type EmailThreadView = {
  mailbox: MailboxDetail;
  thread: EmailThread;
  messages: EmailMessageView[];
  next_message_cursor?: string | null;
  creator_display_name?: string | null;
};

export type PrepareEmailInput = {
  creator_id: string;
  contact_route_id: string;
  mailbox_ref_id: string;
  reply_to_email_id?: string | null;
};

export type EmailPreparation = {
  creator: CreatorRecord;
  route: CreatorContactRouteRecord;
  mailbox: MailboxDetail;
  thread?: EmailThread | null;
  reply_to?: EmailMessageView | null;
  confirmation_required: boolean;
  confirmation_reason: string;
};

export type Touchpoint = {
  id: string;
  creator_id: string;
  kind: string;
  message_id?: string | null;
  email_id?: string | null;
  account_relationship_id?: string | null;
  occurred_at: number;
  metadata: Record<string, unknown>;
};

export type AuditEvent = {
  id: string;
  kind: string;
  creator_id?: string | null;
  account_ref_id?: string | null;
  resolved_target_id?: string | null;
  mailbox_ref_id?: string | null;
  contact_locator_id?: string | null;
  email_id?: string | null;
  payload: Record<string, unknown>;
  created_at: number;
};

export type SendEmailInput = PrepareEmailInput & {
  subject: string;
  text: string;
  html?: string | null;
  idempotency_key: string;
};

export type EmailActionResult = {
  preparation: EmailPreparation;
  thread: EmailThread;
  email: EmailMessageView;
  touchpoint: Touchpoint;
  audit_event: AuditEvent;
  replayed: boolean;
};

export type PrepareManualEmailInput = {
  mailbox_ref_id: string;
  to: string;
};

export type ManualEmailPreparation = {
  mailbox: MailboxDetail;
  to: string;
  confirmation_required: boolean;
  confirmation_reason: string;
};

export type SendManualEmailInput = PrepareManualEmailInput & {
  subject: string;
  text: string;
  html?: string | null;
  idempotency_key: string;
};

export type ManualEmailActionResult = {
  preparation: ManualEmailPreparation;
  thread: EmailThread;
  email: EmailMessageView;
  audit_event: AuditEvent;
  replayed: boolean;
};

/** 唯一的公开同步意图:邮箱 + 显式模式,provider 游标永不过边界。 */
export type SyncMailboxRequest = {
  mailbox_ref_id: string;
  mode: EmailSyncRunMode;
};

export type EmailSyncRunMode = "incremental";

/** 一次同步里某个方向留下的、可安全展示的失败状态。 */
export type EmailSyncFailure = {
  missing_provider_message_ids: string[];
  error_class: string;
  message: string;
};

/** 某方向持久同步状态的公开投影(不含 provider 分页位置)。 */
export type EmailSyncStateView = {
  backfill_complete: boolean;
  last_page_at?: number | null;
  last_full_sync_at?: number | null;
  failure?: EmailSyncFailure | null;
};

export type MailboxDirectionOutcome = {
  pages_listed: number;
  summaries_listed: number;
  details_fetched: number;
  skipped_complete: number;
  imported: number;
  reconciled_statuses: number;
  merged_threads: number;
  missing_provider_message_ids: string[];
  backfill_complete: boolean;
  last_full_sync_at?: number | null;
  failure?: EmailSyncFailure | null;
};

/** 进行中同步的实时计数:当前方向 + 本方向已列出/取回/导入多少。 */
export type MailboxSyncProgress = {
  direction: "inbound" | "outbound";
  pages_listed: number;
  summaries_listed: number;
  details_fetched: number;
  imported: number;
  skipped_complete: number;
  /** 回补时间窗已覆盖的估算百分比(0-100);provider 无总数,按时间轴估算。 */
  estimated_percent?: number | null;
};

export type MailboxSyncStatus = {
  mailbox: MailboxDetail;
  running: boolean;
  /** 仅 running 时存在:本次运行的实时计数。 */
  progress?: MailboxSyncProgress | null;
  inbound?: EmailSyncStateView | null;
  outbound?: EmailSyncStateView | null;
};

/** 一条还欠着判断的入站消息:收进来了,但没人说过它是什么意思。 */
export type PendingReplyTriage = {
  channel: ReplyChannel;
  email_id?: string | null;
  message_id?: string | null;
  creator_id?: string | null;
  from: string;
  subject?: string | null;
  excerpt: string;
  occurred_at: number;
};

/** 一个收件箱欠下的判断:有界清单加真实总数。 */
export type PendingReplyTriageDebt = {
  items: PendingReplyTriage[];
  total: number;
};

export type MailboxSyncOutcome = {
  mailbox: MailboxDetail;
  inbound: MailboxDirectionOutcome;
  outbound: MailboxDirectionOutcome;
  changed_thread_ids: string[];
  pending_triage: PendingReplyTriageDebt;
};

export type EmailFolderSummary = {
  folder: EmailFolder;
  count: number;
};

export type ListEmailFoldersInput = {
  mailbox_ref_id: string | null;
};

export type ListEmailThreadsInput = {
  mailbox_ref_id: string | null;
  folder: EmailFolder;
  sort: EmailThreadSort;
  limit?: number | null;
  cursor?: string | null;
};
export type EmailThreadSort = "newest" | "oldest";
export type EmailThreadGetInput = {
  thread_id: string;
  limit?: number | null;
  cursor?: string | null;
};
export type UpdateEmailThreadDispositionInput = {
  thread_id: string;
  action: EmailThreadDispositionAction;
};
export type EmailThreadMutationInput = {
  thread_id: string;
};

export type EmailTemplate = {
  id: string;
  name: string;
  subject: string;
  text: string;
  html: string | null;
  variables: string[];
  revision: number;
  created_at: number;
  updated_at: number;
};

export type EmailTemplateSummary = {
  id: string;
  name: string;
  subject: string;
  variables: string[];
  revision: number;
  created_at: number;
  updated_at: number;
};

export type ListEmailTemplatesInput = {
  limit?: number | null;
  cursor?: string | null;
};

export type SaveEmailTemplateInput = {
  id: string | null;
  name: string;
  subject: string;
  text: string;
  html: string | null;
  expected_revision: number | null;
};

export type DeleteEmailTemplateInput = {
  template_id: string;
  expected_revision: number;
};

export type RenderEmailTemplateInput = {
  template_id: string;
  creator_id: string | null;
  variables: Record<string, string>;
};

export type RenderedEmailTemplate = {
  template_id: string;
  revision: number;
  subject: string;
  text: string;
  html: string | null;
  unresolved: string[];
};

export type EmailDraftState = "draft" | "trashed";

export type EmailDraft = {
  id: string;
  mailbox_ref_id: string;
  to: string;
  creator_id: string | null;
  contact_route_id: string | null;
  reply_to_email_id: string | null;
  subject: string;
  text: string;
  html: string | null;
  template_id: string | null;
  template_variables: Record<string, string>;
  state: EmailDraftState;
  sending: boolean;
  pending_send_idempotency_key: string | null;
  revision: number;
  created_at: number;
  updated_at: number;
};

export type EmailDraftSummary = {
  id: string;
  mailbox_ref_id: string;
  to: string;
  creator_id: string | null;
  contact_route_id: string | null;
  subject: string;
  preview_text: string;
  has_html: boolean;
  state: EmailDraftState;
  sending: boolean;
  revision: number;
  created_at: number;
  updated_at: number;
};

export type ListEmailDraftsInput = {
  mailbox_ref_id: string | null;
  state: EmailDraftState | null;
  limit?: number | null;
  cursor?: string | null;
};

export type EmailDraftGetInput = {
  draft_id: string;
};

export type SaveEmailDraftInput = {
  id: string | null;
  mailbox_ref_id: string;
  to: string;
  creator_id: string | null;
  contact_route_id: string | null;
  reply_to_email_id: string | null;
  subject: string;
  text: string;
  html: string | null;
  template_id: string | null;
  template_variables: Record<string, string>;
  expected_revision: number | null;
};

export type EmailDraftMutationInput = {
  draft_id: string;
  expected_revision: number;
};

export type SendEmailDraftInput = EmailDraftMutationInput & {
  idempotency_key: string;
};

export type EmailDraftSendResult = {
  draft_id: string;
  thread: EmailThread;
  email: EmailMessageView;
  touchpoint: Touchpoint | null;
  audit_event: AuditEvent;
  replayed: boolean;
};

export type AccountRef = {
  id: string;
  platform: CreatorPlatform;
  kind: "profile" | "service_credential";
  label: string;
  active: boolean;
  availability: "available" | "unavailable";
  created_at: number;
  updated_at: number;
};

export type ResolvedTarget = {
  id: string;
  account_ref_id: string;
  platform: CreatorPlatform;
  target_kind: TargetKind;
  external_target_id: string;
  platform_account_id?: string | null;
  metadata: Record<string, unknown>;
  created_at: number;
  updated_at: number;
};

export type LocatorResolution = {
  id: string;
  locator_id: string;
  account_ref_id: string;
  resolved_target_id: string;
  state: ResolutionState;
  resolved_at: number;
  invalidated_at?: number | null;
};

export type ResolvedLocatorTarget = {
  resolution: LocatorResolution;
  target: ResolvedTarget;
};

export type IdentityConflict = {
  id: string;
  conflict_key: string;
  kind: string;
  creator_id?: string | null;
  platform_account_id?: string | null;
  locator_id?: string | null;
  resolved_target_id?: string | null;
  evidence: Record<string, unknown>;
  state: ConflictState;
  created_at: number;
  resolved_at?: number | null;
};

export type ResolveContactTargetInput = {
  creator_id: string;
  contact_route_id: string;
  operator_account_ref_id: string;
  refresh: boolean;
};

export type ContactTargetResolutionResult = {
  creator_id: string;
  route: CreatorContactRouteRecord;
  operator_account: AccountRef;
  status: TargetResolutionStatus;
  resolved?: ResolvedLocatorTarget | null;
  conflict?: IdentityConflict | null;
  allowed_actions: ContactAction[];
  browser_url?: string | null;
  reason?: string | null;
};

export type ContactTargetPolicy = {
  status: TargetResolutionStatus;
  targetKind: TargetKind | null;
  canFollow: boolean;
  canDirectMessage: boolean;
  canExtractAdministrators: boolean;
  canOpenBrowser: boolean;
  browserUrl: string | null;
  conflict: IdentityConflict | null;
  reason: string | null;
};

/** Never infer target capabilities from a platform or locator shape. */
export function contactTargetPolicy(result: ContactTargetResolutionResult): ContactTargetPolicy {
  const actions = new Set(result.allowed_actions);
  return {
    status: result.status,
    targetKind: result.resolved?.target.target_kind ?? null,
    canFollow: result.status === "resolved" && actions.has("follow"),
    canDirectMessage: result.status === "resolved" && actions.has("direct_message"),
    canExtractAdministrators: result.status === "resolved" && actions.has("extract_administrators"),
    canOpenBrowser: Boolean(result.browser_url)
      && (actions.has("open_profile") || actions.has("open_community") || actions.has("read_public")),
    browserUrl: result.browser_url ?? null,
    conflict: result.conflict ?? null,
    reason: result.reason ?? null,
  };
}

export function contactTargetResolveInput(
  creatorId: string,
  contactRouteId: string,
  operatorAccountRefId: string,
  refresh = false,
): ResolveContactTargetInput {
  return {
    creator_id: creatorId,
    contact_route_id: contactRouteId,
    operator_account_ref_id: operatorAccountRefId,
    refresh,
  };
}

export function relationshipContextInput(
  creatorId: string,
  contactRouteId: string,
  resolvedTargetId: string,
  operatorAccountRefId: string,
): PrepareDirectMessageInput {
  return {
    creator_id: creatorId,
    contact_route_id: contactRouteId,
    resolved_target_id: resolvedTargetId,
    operator_account_ref_id: operatorAccountRefId,
  };
}

export function conversationSyncInput(
  context: PrepareDirectMessageInput,
  externalThreadId: string | null,
  limit: number,
): ConversationSyncInput {
  return {
    ...context,
    external_thread_id: externalThreadId,
    limit,
  };
}

export function updateRelationshipInput(
  context: PrepareDirectMessageInput,
  action: RelationshipMutation,
  idempotencyKey: string,
): UpdateRelationshipInput {
  return {
    ...context,
    action,
    idempotency_key: idempotencyKey,
  };
}

export function sendDirectMessageInput(
  context: PrepareDirectMessageInput,
  body: string,
  allowUnfollowedRecipient: boolean,
  idempotencyKey: string,
): SendDirectMessageInput {
  return {
    ...context,
    body,
    allow_unfollowed_recipient: allowUnfollowedRecipient,
    idempotency_key: idempotencyKey,
  };
}

export type AccountRelationship = {
  id: string;
  actor_account_ref_id: string;
  target_platform_account_id: string;
  kind: RelationshipKind;
  value: RelationshipValue;
  checked_at: number;
  updated_at: number;
};

export type RelationshipSnapshot = {
  following: AccountRelationship;
  can_message: AccountRelationship;
};

export type DirectMessagePreparation = {
  creator_id: string;
  operator_account: AccountRef;
  target: ResolvedTarget;
  relationship: RelationshipSnapshot;
  capability: DirectMessageCapability;
  warnings: string[];
  confirmation_required: boolean;
  unfollowed_confirmation_required: boolean;
  confirmation_reason: string;
};

export type DirectMessageCapability = {
  state: DirectMessageCapabilityState;
  scope: DirectMessageCapabilityScope;
  reason_code: DirectMessageCapabilityReasonCode | null;
  detail: string | null;
};

export type ContactRouteLastContactAccountInput = {
  creator_id: string;
  contact_route_id: string;
};

export type ContactRouteLastContactAccount = {
  contact_route_id: string;
  account_ref: AccountRef;
  channel: ContactChannel;
  last_contact_at: number;
};

export type Conversation = {
  id: string;
  account_ref_id: string;
  resolved_target_id: string;
  channel: ContactChannel;
  provider_thread_id: string;
  cursor?: string | null;
  unread_count: number;
  created_at: number;
  updated_at: number;
};

export type Message = {
  id: string;
  conversation_id: string;
  external_message_id?: string | null;
  sender_target_id?: string | null;
  direction: string;
  body: string;
  status: string;
  public_data: Record<string, unknown>;
  sent_at: number;
};

export type ConversationView = {
  creator_id: string;
  operator_account: AccountRef;
  target: ResolvedTarget;
  conversation: Conversation;
  messages: Message[];
  pending_triage: PendingReplyTriageDebt;
};

export type CommitRelationshipActionResult = {
  relationship: AccountRelationship;
  can_message: AccountRelationship;
  touchpoint: Touchpoint;
  audit_event: AuditEvent;
};

export type CommitMessageActionResult = {
  conversation: Conversation;
  message: Message;
  touchpoint: Touchpoint;
  audit_event: AuditEvent;
};

export type RelationshipContextInput = {
  creator_id: string;
  resolved_target_id: string;
  operator_account_ref_id: string;
  refresh: boolean;
};

export type PrepareDirectMessageInput = Omit<RelationshipContextInput, "refresh"> & {
  contact_route_id: string;
};

export type PrepareRelationshipActionInput = PrepareDirectMessageInput & {
  action: RelationshipMutation;
};

export type UpdateRelationshipInput = PrepareRelationshipActionInput & {
  idempotency_key: string;
};

export type SendDirectMessageInput = PrepareDirectMessageInput & {
  body: string;
  allow_unfollowed_recipient: boolean;
  idempotency_key: string;
};

export type ConversationSyncInput = PrepareDirectMessageInput & {
  external_thread_id?: string | null;
  limit: number;
};

export type ConversationGetInput = { conversation_id: string };

export type UpdateCreatorStageInput = { creator_id: string; stage: CreatorStage };

export type ManualCreatorCreateInput = {
  display_name: string;
  tags?: string[] | null;
  notes: string | null;
  contact_routes: Array<{ channel: ContactChannel; value: string; source_url: string | null }>;
};
export type ManualCreatorCreateResult = {
  creator_id: string;
  display_name: string;
  tags: string[];
  stage: CreatorStage;
  created_at: number;
  contact_routes: CreatorContactRouteRecord[];
};
export type ManualCreatorContactRouteAddInput = {
  creator_id: string;
  channel: ContactChannel;
  value: string;
  source_url: string | null;
};
export type ManualCreatorContactRouteAddResult = {
  status: "added" | "already_exists";
  creator_id: string;
  route_id: string;
  channel: ContactChannel;
  locator_kind: LocatorKind;
  normalized_value: string;
  display_value: string;
  confidence: Confidence;
  created_at: number;
};
export type ManualCreatorContactRouteUpdateInput = {
  creator_id: string;
  contact_route_id: string;
  channel: ContactChannel;
  value: string;
  source_url: string | null;
};
export type ManualCreatorContactRouteUpdateResult = {
  status: "updated" | "unchanged" | "replacement_required";
  creator_id: string;
  route_id: string;
  route: CreatorContactRouteRecord;
};
export type CreatorContactRouteDeletePrepareInput = {
  creator_id: string;
  contact_route_id: string;
};
export type CreatorContactRouteDeleteDependency = {
  creator_id: string;
  contact_route_id: string;
  dependency_kind: "link_hub" | "community_administrator";
  channel: ContactChannel;
  display_value: string;
};
export type CreatorContactRouteDeleteBlockingArtifact = {
  kind:
    | "in_flight_email"
    | "in_flight_direct_message"
    | "in_flight_relationship_mutation"
    | "ambiguous_direct_message_history"
    | "shared_email_history"
    | "shared_direct_message_history"
    | "shared_contact_locator"
    | "ambiguous_local_audit_history";
  count: number;
};
export type CreatorContactRouteDeleteImpact = {
  dependencies: number;
  contact_routes: number;
  email_threads: number;
  email_messages: number;
  email_drafts: number;
  email_recipients: number;
  direct_message_conversations: number;
  direct_messages: number;
  outreach_touchpoints: number;
  contact_locators: number;
  observation_sources: number;
  resolved_targets: number;
  creator_origins_detached: number;
  audit_events: number;
  idempotency_records: number;
  target_resolutions: number;
  identity_conflicts: number;
  observations: number;
  local_records_total: number;
};
export type CreatorContactRouteDeletePrepareResult =
  | {
    status: "blocked";
    creator_id: string;
    contact_route_id: string;
    reason:
      | "archived_creator"
      | "root_initial_platform_account"
      | "has_derived_contact_routes"
      | "in_flight_external_action"
      | "ambiguous_legacy_history"
      | "shared_local_history";
    children: CreatorContactRouteDeleteDependency[];
    blocking_artifacts: CreatorContactRouteDeleteBlockingArtifact[];
  }
  | {
    status: "confirmation_required";
    operation_id: string;
    creator_id: string;
    contact_route_id: string;
    impact: CreatorContactRouteDeleteImpact;
    remote_not_deleted: boolean;
    expires_at: number;
    next_tool: string;
  };
export type CreatorContactRouteDeleteConfirmInput = { operation_id: string };
export type CreatorContactRouteDeleteConfirmResult =
  | {
    status: "deleted";
    operation_id: string;
    creator_id: string;
    contact_route_id: string;
    deleted: CreatorContactRouteDeleteImpact;
    remote_not_deleted: boolean;
  }
  | {
    status: "stale";
    operation_id: string;
    reason: "plan_changed";
    next_tool: string;
  }
  | {
    status: "expired";
    operation_id: string;
    next_tool: string;
  };

export type CreatorAccountView = {
  platform: CreatorPlatform;
  target_kind: TargetKind;
  external_id: string;
  handle?: string | null;
  display_name: string;
  profile_url?: string | null;
  public_data: Record<string, unknown>;
  /** The latest observation's source kind — how this account identity was
   * established (initial capture, channel identification, …). */
  origin: SourceKind;
};

export function primaryCreatorAccount(record: CreatorRecord): CreatorAccountView | null {
  return creatorAccountViews(record)[0] ?? null;
}

export function creatorAccountViews(record: CreatorRecord): CreatorAccountView[] {
  const sourceKinds = new Map(record.observation_sources.map((source) => [source.id, source.kind]));
  return record.platform_accounts.map((account) => {
    const observation = record.account_observations
      .filter((item) => item.platform_account_id === account.id)
      .sort((left, right) => right.observed_at - left.observed_at)[0];
    return {
      platform: account.platform,
      target_kind: account.target_kind,
      external_id: account.external_id,
      handle: observation?.handle,
      display_name: observation?.display_name ?? record.creator.display_name,
      profile_url: observation?.profile_url,
      public_data: observation?.public_data ?? {},
      origin: (observation && sourceKinds.get(observation.source_id)) ?? "initial_platform_account",
    };
  });
}

/** New observations use canonical avatar_url. Older v36 observations retain
 * their original provider keys and remain readable without rewriting data. */
export function creatorRecordAvatarUrl(record: CreatorRecord): string | null {
  const account = primaryCreatorAccount(record);
  if (!account) return null;
  const canonical = account.public_data.avatar_url;
  if (typeof canonical === "string" && canonical.trim()) return canonical;
  const keys = ({
    x: ["profile_image_url"],
    instagram: ["profile_pic_url", "profile_pic_url_hd"],
    telegram: ["photo_url", "profile_photo_url"],
    youtube: ["thumbnail_url"],
    tiktok: ["profile_image"],
  } as Record<CreatorPlatform, string[]>)[account.platform];
  for (const key of keys) {
    const value = account.public_data[key];
    if (typeof value === "string" && value) return value;
  }
  return null;
}

export async function creatorInvoke<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  if (!isRuntimeAvailable()) {
    const message = appI18n.isInitialized
      ? appI18n.t("runtimeUnavailable.description", { ns: "common" })
      : RUNTIME_UNAVAILABLE_MESSAGE;
    throw new Error(message);
  }
  return invoke<T>(command, args);
}

export async function createManualCreator(
  input: ManualCreatorCreateInput,
): Promise<ManualCreatorCreateResult> {
  return creatorInvoke<ManualCreatorCreateResult>("onecrew_creator_manual_create", { input });
}

export async function updateCreator(input: UpdateCreatorInput): Promise<CreatorRecord> {
  return creatorInvoke<CreatorRecord>("onecrew_creator_update", { input });
}

/** Human-facing Desktop export is XLSX-only. Internal and MCP callers retain
 * explicit access to the canonical JSON representation through Rust owners. */
export async function exportCreators(): Promise<CreatorExportResult> {
  return creatorInvoke<CreatorExportResult>("onecrew_creator_export", { input: { format: "xlsx" } });
}

export async function prepareCreatorImportFile(): Promise<CreatorImportFilePrepareResult> {
  const file = await new Promise<File | null>((resolve) => {
    const picker = document.createElement("input");
    picker.type = "file";
    picker.accept = ".xlsx";
    picker.hidden = true;
    const settle = (file: File | null) => { picker.remove(); resolve(file); };
    picker.addEventListener("change", () => settle(picker.files?.[0] ?? null), { once: true });
    picker.addEventListener("cancel", () => settle(null), { once: true });
    document.body.append(picker);
    picker.click();
  });
  if (!file) return { status: "cancelled" };
  return runtimeRequest<CreatorImportFilePrepareResult>("/api/creator-import", file, {
    "Content-Type": "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
    "X-OneCrew-File-Name": encodeURIComponent(file.name),
  });
}

export async function confirmCreatorImport(
  operationId: string,
): Promise<ConfirmCreatorImportResult> {
  return creatorInvoke<ConfirmCreatorImportResult>("onecrew_creator_import_confirm", {
    input: { operation_id: operationId },
  });
}

export async function cancelCreatorImport(
  operationId: string,
): Promise<ConfirmCreatorImportResult> {
  return creatorInvoke<ConfirmCreatorImportResult>("onecrew_creator_import_cancel", {
    input: { operation_id: operationId },
  });
}

export async function addManualCreatorContactRoute(
  input: ManualCreatorContactRouteAddInput,
): Promise<ManualCreatorContactRouteAddResult> {
  return creatorInvoke<ManualCreatorContactRouteAddResult>("onecrew_creator_contact_route_add", { input });
}

export async function updateManualCreatorContactRoute(
  input: ManualCreatorContactRouteUpdateInput,
): Promise<ManualCreatorContactRouteUpdateResult> {
  return creatorInvoke<ManualCreatorContactRouteUpdateResult>("onecrew_creator_contact_route_update", { input });
}

export async function prepareCreatorContactRouteDelete(
  input: CreatorContactRouteDeletePrepareInput,
): Promise<CreatorContactRouteDeletePrepareResult> {
  return creatorInvoke<CreatorContactRouteDeletePrepareResult>("onecrew_creator_contact_route_delete_prepare", { input });
}

export async function confirmCreatorContactRouteDelete(
  input: CreatorContactRouteDeleteConfirmInput,
): Promise<CreatorContactRouteDeleteConfirmResult> {
  return creatorInvoke<CreatorContactRouteDeleteConfirmResult>("onecrew_creator_contact_route_delete_confirm", { input });
}

export async function updateCreatorStage(
  input: UpdateCreatorStageInput,
): Promise<CreatorRelationship> {
  return creatorInvoke<CreatorRelationship>("onecrew_creator_update_stage", { input });
}

export interface CreatorDeleteResult {
  creator_id: string;
  removed_contact_routes: number;
  removed_touchpoints: number;
  removed_origins: number;
  removed_account_links: number;
  cleared_references: number;
}

export async function deleteCreator(creatorId: string): Promise<CreatorDeleteResult> {
  return creatorInvoke<CreatorDeleteResult>("onecrew_creator_delete", {
    input: { creator_id: creatorId },
  });
}

export async function archiveCreator(creatorId: string): Promise<Creator> {
  return creatorInvoke<Creator>("onecrew_creator_archive", { creatorId });
}

function formattingLocale(explicitLocale?: string): string {
  if (explicitLocale) return explicitLocale;
  if (!appI18n.isInitialized) return DEFAULT_LOCALE;
  return appI18n.resolvedLanguage ?? DEFAULT_LOCALE;
}

/** Formats an epoch-second timestamp in the user's local time zone. */
export function formatDate(epoch: number | null | undefined, locale?: string) {
  if (!epoch) return "—";
  return new Intl.DateTimeFormat(formattingLocale(locale), {
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
    hourCycle: "h23",
  }).format(new Date(epoch * 1000));
}

export function formatCount(value: unknown, locale?: string) {
  const number = typeof value === "number" ? value : Number(value ?? 0);
  if (!Number.isFinite(number)) return "0";
  return new Intl.NumberFormat(formattingLocale(locale), {
    maximumFractionDigits: 1,
    notation: "compact",
  }).format(number);
}

// ===== Reply triage and inbox attention =====

/** Agent 对一条来信的判读:意图 + 理由 + 可采用的草拟话术。 */
export type ReplyIntent =
  | "interested"
  | "question"
  | "negotiating"
  | "declined"
  | "irrelevant"
  | "other";

export type ReplyChannel = "email" | "dm";

/** 需要人看一眼的意图。已拒绝和无关是答案,不是请求。 */
export const REPLY_INTENT_NEEDS_ATTENTION: Record<ReplyIntent, boolean> = {
  interested: true,
  question: true,
  negotiating: true,
  declined: false,
  irrelevant: false,
  other: false,
};

/** 收件箱行上渲染的那一份:够画一个标签和一段 hover。 */
export type ReplyTriageBrief = {
  intent: ReplyIntent;
  summary: string;
  suggested_reply?: string | null;
  created_at: number;
  acknowledged: boolean;
};

export type ReplyTriage = {
  id: string;
  channel: ReplyChannel;
  email_id?: string | null;
  message_id?: string | null;
  intent: ReplyIntent;
  summary: string;
  suggested_reply?: string | null;
  created_at: number;
  acknowledged_at?: number | null;
};

export type ReplyTriageView = ReplyTriage & {
  creator_id?: string | null;
  creator_name?: string | null;
  email_thread_id?: string | null;
  conversation_id?: string | null;
  excerpt: string;
};

export type ListReplyTriageInput = {
  limit?: number;
  cursor?: string | null;
  include_acknowledged?: boolean;
};

/** 两个收件箱各有多少条在等人。两侧语义不同,而且是故意的:
 *  邮件有真正的未读态;私信没有(provider 不给未读数),所以私信数的是
 *  「最后一条是对方发的」——外联真正关心的是球在谁手里。
 *
 *  它跟意向标签正交:人工同步和 agent 同步都会带来新消息(于是计数),
 *  但只有 agent 判读过才会有标签。两者都没有的行才是「没事」。 */
export type InboxAttention = {
  conversations: number;
  mail: number;
};

export const INBOX_ATTENTION_QUERY_KEY = ["creator-inbox-attention"] as const;

export const REPLY_TRIAGE_QUERY_KEY = ["creator-reply-triage"] as const;

export async function setCreatorPinned(creatorId: string, pinned: boolean): Promise<void> {
  return creatorInvoke("onecrew_creator_pin", { input: { creator_id: creatorId, pinned } });
}
