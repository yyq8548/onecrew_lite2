import type { QueryClient } from "@tanstack/react-query";
import type { TFunction } from "i18next";

import { duplicateEmailTemplateName } from "@/components/mail/template-library-model";
import {
  creatorInvoke,
  type DeleteEmailTemplateInput,
  type EmailDraft,
  type EmailDraftGetInput,
  type EmailDraftSummary,
  type EmailDraftState,
  type EmailFolder,
  type EmailFolderSummary,
  type EmailTemplate,
  type EmailTemplateSummary,
  type EmailThreadMutationInput,
  type EmailThreadSort,
  type EmailThreadView,
  type EmailThreadGetInput,
  type ListEmailDraftsInput,
  type ListEmailFoldersInput,
  type ListEmailTemplatesInput,
  type ListEmailThreadsInput,
  type EmailSyncFailure,
  type MailboxSyncOutcome,
  type MailboxSyncProgress,
  type MailboxSyncStatus,
  type RenderedEmailTemplate,
  type RenderEmailTemplateInput,
  type SaveEmailTemplateInput,
  type SyncMailboxRequest,
  type UpdateEmailThreadDispositionInput,
  type Page,
} from "@/lib/creator";

export const MAILBOX_QUERY_KEY = ["creator-email-mailboxes"] as const;
export const emailFolderListKey = (mailboxId: string | null) => ["creator-email-folders", mailboxId] as const;
export const emailThreadListRootKey = (mailboxId: string | null) => ["creator-email-threads", mailboxId] as const;
export const emailThreadListKey = (
  mailboxId: string | null,
  folder: EmailFolder,
  sort: EmailThreadSort,
) => (
  [...emailThreadListRootKey(mailboxId), folder, sort] as const
);
export const emailThreadKey = (threadId: string | null) => ["creator-email-thread", threadId] as const;
export const EMAIL_TEMPLATE_LIST_QUERY_KEY = ["creator-email-templates"] as const;
export const EMAIL_TEMPLATE_PAGINATION_LIMIT_CODE = "email_template_pagination_limit_exceeded" as const;

export class EmailTemplatePaginationLimitError extends Error {
  readonly code = EMAIL_TEMPLATE_PAGINATION_LIMIT_CODE;

  constructor() {
    super(EMAIL_TEMPLATE_PAGINATION_LIMIT_CODE);
    this.name = "EmailTemplatePaginationLimitError";
  }
}

export function isEmailTemplatePaginationLimitError(
  error: unknown,
): error is EmailTemplatePaginationLimitError {
  return error instanceof EmailTemplatePaginationLimitError
    || (typeof error === "object"
      && error !== null
      && "code" in error
      && error.code === EMAIL_TEMPLATE_PAGINATION_LIMIT_CODE);
}

export const emailDraftListRootKey = (mailboxId: string | null) => ["creator-email-drafts", mailboxId] as const;
export const emailDraftListKey = (mailboxId: string | null, state: EmailDraftState | null) => (
  [...emailDraftListRootKey(mailboxId), state] as const
);
export const emailDraftKey = (draftId: string | null) => ["creator-email-draft", draftId] as const;

export const listMailboxes = <T,>() => creatorInvoke<T>("onecrew_email_mailbox_list");
export const listEmailFolders = (input: ListEmailFoldersInput) => (
  creatorInvoke<EmailFolderSummary[]>("onecrew_email_folder_list", { input })
);
export const listEmailThreads = <T,>(input: ListEmailThreadsInput) => (
  creatorInvoke<T>("onecrew_email_thread_list", { input })
);
export const getEmailThread = <T,>(input: EmailThreadGetInput) => (
  creatorInvoke<T>("onecrew_email_thread_get", { input })
);
export const syncMailbox = (input: SyncMailboxRequest) => (
  creatorInvoke<MailboxSyncOutcome>("onecrew_email_mailbox_sync", { input })
);
export const getMailboxSyncStatus = (mailboxRefId: string) => (
  creatorInvoke<MailboxSyncStatus>("onecrew_email_mailbox_sync_status", {
    input: { mailbox_ref_id: mailboxRefId },
  })
);
export const mailboxSyncStatusKey = (mailboxId: string | null) => (
  ["creator-email-sync-status", mailboxId] as const
);
export const updateEmailThreadDisposition = (input: UpdateEmailThreadDispositionInput) => (
  creatorInvoke<EmailThreadView>("onecrew_email_thread_update_disposition", { input })
);
export const markEmailThreadRead = (input: EmailThreadMutationInput) => (
  creatorInvoke<EmailThreadView>("onecrew_email_thread_mark_read", { input })
);
export const listEmailTemplates = (input: ListEmailTemplatesInput = {}) => (
  creatorInvoke<Page<EmailTemplateSummary>>("onecrew_email_template_list", { input })
);
/** Load every template summary page; the picker and library need the full set. */
export async function listAllEmailTemplates(): Promise<EmailTemplateSummary[]> {
  const items: EmailTemplateSummary[] = [];
  let cursor: string | null = null;
  for (let page = 0; page < 50; page += 1) {
    const result: Page<EmailTemplateSummary> = await listEmailTemplates({ limit: 100, cursor });
    items.push(...result.items);
    if (!result.next_cursor) return items;
    cursor = result.next_cursor;
  }
  throw new EmailTemplatePaginationLimitError();
}
export const getEmailTemplate = (input: { template_id: string }) => (
  creatorInvoke<EmailTemplate>("onecrew_email_template_get", { input })
);
export const saveEmailTemplate = (input: SaveEmailTemplateInput) => (
  creatorInvoke<EmailTemplate>("onecrew_email_template_save", { input })
);
export const deleteEmailTemplate = (input: DeleteEmailTemplateInput) => (
  creatorInvoke<boolean>("onecrew_email_template_delete", { input })
);
export const renderEmailTemplate = (input: RenderEmailTemplateInput) => (
  creatorInvoke<RenderedEmailTemplate>("onecrew_email_template_render", { input })
);
/**
 * 复制一份模板：先读全文（列表摘要没有正文），再以派生出的新名字另存。
 * 新建走 id/expected_revision 均为 null。
 */
export async function duplicateEmailTemplate(
  templateId: string,
  existingNames: readonly string[],
  formatName: (name: string, index: number) => string,
): Promise<EmailTemplate> {
  const source = await getEmailTemplate({ template_id: templateId });
  return saveEmailTemplate({
    id: null,
    name: duplicateEmailTemplateName(source.name, existingNames, formatName),
    subject: source.subject,
    text: source.text,
    html: source.html,
    expected_revision: null,
  });
}
export const listEmailDrafts = (input: ListEmailDraftsInput) => (
  creatorInvoke<Page<EmailDraftSummary>>("onecrew_email_draft_list", { input })
);
export const getEmailDraft = (input: EmailDraftGetInput) => (
  creatorInvoke<EmailDraft>("onecrew_email_draft_get", { input })
);

/** 模板 query 族唯一的失效入口（UI-05）：库、选择器、写信浮层都从这里刷新。 */
export async function invalidateEmailTemplateQueries(queryClient: QueryClient): Promise<void> {
  await queryClient.invalidateQueries({ queryKey: EMAIL_TEMPLATE_LIST_QUERY_KEY });
}

export async function invalidateMailboxMailQueries(
  queryClient: QueryClient,
  mailboxId: string | null,
  threadId?: string | null,
) {
  await Promise.all([
    queryClient.invalidateQueries({ queryKey: emailFolderListKey(mailboxId) }),
    queryClient.invalidateQueries({ queryKey: emailThreadListRootKey(mailboxId) }),
    queryClient.invalidateQueries({ queryKey: emailDraftListRootKey(mailboxId) }),
    threadId
      ? queryClient.invalidateQueries({ queryKey: emailThreadKey(threadId) })
      : Promise.resolve(),
  ]);
}

/**
 * Run the complete pull-only sync for one mailbox. One call does the whole
 * bidirectional run; the runtime coalesces concurrent callers per mailbox,
 * so the Mail page and MCP can never race duplicate provider walks — the
 * frontend keeps no in-flight registry of its own.
 */
export function runMailboxSync(mailboxRefId: string): Promise<MailboxSyncOutcome> {
  return syncMailbox({ mailbox_ref_id: mailboxRefId, mode: "incremental" });
}

/** 同步结果的人话摘要,给成功 toast 用。 */
export function summarizeSyncOutcome(outcome: MailboxSyncOutcome, t: TFunction<"mail">): string {
  const parts: string[] = [
    t("page.sync.outcome.inbound", { count: outcome.inbound.imported }),
    t("page.sync.outcome.outbound", { count: outcome.outbound.imported }),
  ];
  const skipped = outcome.inbound.skipped_complete + outcome.outbound.skipped_complete;
  if (skipped > 0) parts.push(t("page.sync.outcome.skipped", { count: skipped }));
  if (outcome.outbound.reconciled_statuses > 0) {
    parts.push(t("page.sync.outcome.reconciled", { count: outcome.outbound.reconciled_statuses }));
  }
  const missing = outcome.inbound.missing_provider_message_ids.length
    + outcome.outbound.missing_provider_message_ids.length;
  if (missing > 0) parts.push(t("page.sync.outcome.missing", { count: missing }));
  return parts.join(t("page.sync.outcome.separator"));
}

export type MailboxSyncUiState = {
  kind: "idle" | "running" | "partial" | "failed" | "completed";
  label: string;
  /** 首次回补尚未到达 provider 保留期底部时提醒:旧邮件可能先过期。 */
  retentionWarning: boolean;
  lastSuccessAt: number | null;
  /** 仅 running 时非空:进行中运行的实时计数(runtime 逐页推送)。 */
  progress: MailboxSyncProgress | null;
};

/** 实时进度的人话摘要,给同步面板用;纯函数。 */
export function summarizeSyncProgress(progress: MailboxSyncProgress, t: TFunction<"mail">): string {
  const direction = progress.direction === "inbound"
    ? t("page.sync.progress.inbound")
    : t("page.sync.progress.outbound");
  const parts: string[] = [t("page.sync.progress.summary", {
    direction,
    page: progress.pages_listed,
    count: progress.summaries_listed,
  })];
  if (progress.imported > 0) parts.push(t("page.sync.progress.imported", { count: progress.imported }));
  if (progress.skipped_complete > 0) {
    parts.push(t("page.sync.progress.skipped", { count: progress.skipped_complete }));
  }
  if (typeof progress.estimated_percent === "number") {
    parts.push(t("page.sync.progress.estimated", { percent: progress.estimated_percent }));
  }
  return parts.join(t("page.sync.progress.separator"));
}

/** 从状态投影派生 UI 状态;纯函数,不碰网络。 */
export function deriveSyncUiState(
  status: MailboxSyncStatus | undefined,
  mutationPending: boolean,
  t: TFunction<"mail">,
): MailboxSyncUiState {
  const inbound = status?.inbound ?? null;
  const outbound = status?.outbound ?? null;
  const lastSuccessAt = [inbound?.last_full_sync_at, outbound?.last_full_sync_at]
    .filter((value): value is number => typeof value === "number")
    .reduce<number | null>((newest, value) => (newest === null || value > newest ? value : newest), null);
  const retentionWarning = Boolean(
    (inbound && !inbound.backfill_complete) || (outbound && !outbound.backfill_complete),
  );
  if (mutationPending || status?.running) {
    return {
      kind: "running",
      label: t("page.sync.states.running"),
      retentionWarning,
      lastSuccessAt,
      progress: status?.progress ?? null,
    };
  }
  const failures = [inbound?.failure, outbound?.failure].filter(
    (value): value is EmailSyncFailure => Boolean(value),
  );
  if (failures.length > 0) {
    if (failures.some(value => /^gmail_read_http_(403_(rateLimitExceeded|userRateLimitExceeded|retry_after)|429(?:_retry_after)?)$/.test(value.message))) {
      return { kind: "partial", label: t("page.sync.states.rateLimited"), retentionWarning, lastSuccessAt, progress: null };
    }
    // 预算撞顶不是失败:runtime 已把游标落盘,下一轮从断点续走。大团队的
    // 首轮回补就是分多轮完成的,按"进行中"呈现而不是吓人的"失败"。
    if (failures.every((value) => value.error_class === "page_budget_exhausted")) {
      return {
        kind: "partial",
        label: t("page.sync.states.backfill"),
        retentionWarning,
        lastSuccessAt,
        progress: null,
      };
    }
    const missing = (inbound?.failure?.missing_provider_message_ids.length ?? 0)
      + (outbound?.failure?.missing_provider_message_ids.length ?? 0);
    return missing > 0
      ? { kind: "partial", label: t("page.sync.states.partial", { count: missing }), retentionWarning, lastSuccessAt, progress: null }
      : { kind: "failed", label: t("page.sync.states.failed"), retentionWarning, lastSuccessAt, progress: null };
  }
  if (lastSuccessAt !== null) {
    return { kind: "completed", label: t("page.sync.states.completed"), retentionWarning, lastSuccessAt, progress: null };
  }
  return { kind: "idle", label: t("page.sync.states.idle"), retentionWarning, lastSuccessAt, progress: null };
}

const AUTO_SYNC_MIN_INTERVAL_MS = 5 * 60 * 1000;

/**
 * Bounded foreground freshness: app start, window focus, and network
 * recovery all funnel into one guarded trigger. The interval guard keeps
 * focus flapping from spamming the runtime; the runtime's per-mailbox
 * coalescing makes an extra call harmless anyway (UI-07: bounded polling
 * with an explicit reason — mail changes externally via MCP and provider).
 */
export function createAutoSyncTrigger(
  run: (mailboxRefId: string) => void,
  now: () => number = () => Date.now(),
) {
  const lastRun = new Map<string, number>();
  return (mailboxRefId: string | null, canReceive: boolean) => {
    if (!mailboxRefId || !canReceive) return false;
    const at = now();
    const previous = lastRun.get(mailboxRefId);
    if (previous !== undefined && at - previous < AUTO_SYNC_MIN_INTERVAL_MS) return false;
    lastRun.set(mailboxRefId, at);
    run(mailboxRefId);
    return true;
  };
}
