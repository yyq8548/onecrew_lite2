import { beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import { QueryClient } from "@tanstack/react-query";
import i18next, { type TFunction } from "i18next";

const creatorInvokeMock = vi.fn();
vi.mock("@/lib/creator", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/creator")>()),
  creatorInvoke: (...args: unknown[]) => creatorInvokeMock(...args),
}));

import {
  EMAIL_TEMPLATE_LIST_QUERY_KEY,
  EMAIL_TEMPLATE_PAGINATION_LIMIT_CODE,
  createAutoSyncTrigger,
  deriveSyncUiState as deriveSyncUiStateWithTranslation,
  duplicateEmailTemplate as duplicateEmailTemplateWithFormatter,
  summarizeSyncProgress as summarizeSyncProgressWithTranslation,
  emailDraftListKey,
  emailFolderListKey,
  emailThreadKey,
  emailThreadListKey,
  invalidateEmailTemplateQueries,
  invalidateMailboxMailQueries,
  isEmailTemplatePaginationLimitError,
  listAllEmailTemplates,
  summarizeSyncOutcome as summarizeSyncOutcomeWithTranslation,
} from "@/components/mail/queries";
import zhCNMail from "@/i18n/locales/zh-CN/mail";
import type { EmailTemplate, MailboxSyncOutcome, MailboxSyncStatus } from "@/lib/creator";

let mailT: TFunction<"mail">;

beforeAll(async () => {
  const instance = i18next.createInstance();
  await instance.init({ lng: "zh-CN", resources: { "zh-CN": { mail: zhCNMail } } });
  mailT = instance.getFixedT("zh-CN", "mail");
});

const summarizeSyncOutcome = (value: MailboxSyncOutcome) => (
  summarizeSyncOutcomeWithTranslation(value, mailT)
);
const summarizeSyncProgress = (value: Parameters<typeof summarizeSyncProgressWithTranslation>[0]) => (
  summarizeSyncProgressWithTranslation(value, mailT)
);
const deriveSyncUiState = (
  value: MailboxSyncStatus | undefined,
  pending: boolean,
) => deriveSyncUiStateWithTranslation(value, pending, mailT);
const duplicateEmailTemplate = (templateId: string, existingNames: readonly string[]) => (
  duplicateEmailTemplateWithFormatter(
    templateId,
    existingNames,
    (name, index) => index === 1 ? `${name} 副本` : `${name} 副本 ${index}`,
  )
);

function direction(
  overrides: Partial<MailboxSyncOutcome["inbound"]> = {},
): MailboxSyncOutcome["inbound"] {
  return {
    pages_listed: 1,
    summaries_listed: 0,
    details_fetched: 0,
    skipped_complete: 0,
    imported: 0,
    reconciled_statuses: 0,
    merged_threads: 0,
    missing_provider_message_ids: [],
    backfill_complete: true,
    last_full_sync_at: null,
    failure: null,
    ...overrides,
  };
}

function outcome(
  inbound: Partial<MailboxSyncOutcome["inbound"]>,
  outbound: Partial<MailboxSyncOutcome["outbound"]>,
): MailboxSyncOutcome {
  return {
    mailbox: {
      mailbox_ref_id: "mailbox-1",
      provider: "resend",
      label: "Mail",
      email_address: "mail@example.com",
      capabilities: ["inbox", "sent_readback"],
      available: true,
      issues: [],
      created_at: 1,
      updated_at: 1,
    },
    inbound: direction(inbound),
    outbound: direction(outbound),
    changed_thread_ids: [],
    pending_triage: { items: [], total: 0 },
  };
}

function status(
  inbound: MailboxSyncStatus["inbound"],
  outbound: MailboxSyncStatus["outbound"],
  running = false,
): MailboxSyncStatus {
  return {
    mailbox: outcome({}, {}).mailbox,
    running,
    inbound,
    outbound,
  };
}

describe("summarizeSyncOutcome", () => {
  it("reports both directions plus skips, reconciliations, and missing work", () => {
    const text = summarizeSyncOutcome(outcome(
      { imported: 3, skipped_complete: 2 },
      {
        imported: 1,
        skipped_complete: 4,
        reconciled_statuses: 2,
        missing_provider_message_ids: ["a"],
      },
    ));
    expect(text).toContain("收件 3 封");
    expect(text).toContain("发件 1 封");
    expect(text).toContain("跳过已同步 6 封");
    expect(text).toContain("更新投递状态 2 条");
    expect(text).toContain("1 封暂未取回");
  });

  it("stays terse when nothing extra happened", () => {
    expect(summarizeSyncOutcome(outcome({}, {}))).toBe("收件 0 封，发件 0 封");
  });
});

describe("summarizeSyncProgress", () => {
  it("names the walking direction with page and item counters", () => {
    const text = summarizeSyncProgress({
      direction: "inbound",
      pages_listed: 3,
      summaries_listed: 240,
      details_fetched: 12,
      imported: 12,
      skipped_complete: 40,
    });
    expect(text).toContain("收件");
    expect(text).toContain("第 3 页");
    expect(text).toContain("已扫描 240 封");
    expect(text).toContain("导入 12 封");
    expect(text).toContain("跳过已同步 40 封");
  });

  it("appends the time-window estimate when the runtime provides one", () => {
    const text = summarizeSyncProgress({
      direction: "outbound",
      pages_listed: 52,
      summaries_listed: 85,
      details_fetched: 85,
      imported: 85,
      skipped_complete: 0,
      estimated_percent: 42,
    });
    expect(text).toContain("约 42%");
  });

  it("stays terse while a walk has matched nothing yet", () => {
    const text = summarizeSyncProgress({
      direction: "outbound",
      pages_listed: 8,
      summaries_listed: 800,
      details_fetched: 0,
      imported: 0,
      skipped_complete: 0,
    });
    expect(text).toBe("已发送 · 第 8 页 · 已扫描 800 封");
  });
});

describe("deriveSyncUiState", () => {
  it("is idle before any durable state exists", () => {
    const state = deriveSyncUiState(status(null, null), false);
    expect(state.kind).toBe("idle");
    expect(state.retentionWarning).toBe(false);
  });

  it("runs when the runtime reports an in-flight operation, even from another surface", () => {
    expect(deriveSyncUiState(status(null, null, true), false).kind).toBe("running");
    expect(deriveSyncUiState(undefined, true).kind).toBe("running");
  });

  it("passes live counters through only while running", () => {
    const progress = {
      direction: "outbound" as const,
      pages_listed: 2,
      summaries_listed: 150,
      details_fetched: 9,
      imported: 9,
      skipped_complete: 0,
    };
    const running = deriveSyncUiState({ ...status(null, null, true), progress }, false);
    expect(running.kind).toBe("running");
    expect(running.progress).toEqual(progress);
    // A local mutation that started before the first status refresh has no
    // counters yet; the panel shows its preparing copy instead.
    expect(deriveSyncUiState(undefined, true).progress).toBeNull();
    expect(deriveSyncUiState(status(null, null), false).progress).toBeNull();
  });

  it("identifies Gmail throttling without asking for new authorization", () => {
    const failure = {error_class:"detail_retrieval_failed", message:"gmail_read_http_403_rateLimitExceeded", missing_provider_message_ids:["fixture"]};
    const result = deriveSyncUiState(status(direction({failure}), null), false);
    expect(result.kind).toBe("partial");
    expect(result.label).toContain("限流");
    expect(result.label).toContain("无需重新授权");
  });

  it("presents page-budget exhaustion as an in-progress multi-round backfill, not a failure", () => {
    const budget = {
      missing_provider_message_ids: [],
      error_class: "page_budget_exhausted",
      message: "stopped after 200 provider pages in one run",
    };
    const state = deriveSyncUiState(
      status(direction({ failure: budget, backfill_complete: false }), null),
      false,
    );
    expect(state.kind).toBe("partial");
    expect(state.label).toContain("断点继续");
    expect(state.retentionWarning).toBe(true);

    // Mixed with a real retrieval failure, the ordinary failure surfaces.
    const detail = {
      missing_provider_message_ids: ["m-1"],
      error_class: "detail_retrieval_failed",
      message: "scripted",
    };
    const mixed = deriveSyncUiState(
      status(
        direction({ failure: budget, backfill_complete: false }),
        direction({ failure: detail, backfill_complete: false }),
      ),
      false,
    );
    expect(mixed.kind).toBe("partial");
    expect(mixed.label).toContain("1 封待重试");
  });

  it("surfaces partial state with the missing count and completion with the newest success", () => {
    const partial = deriveSyncUiState(
      status(
        {
          backfill_complete: true,
          last_full_sync_at: 100,
          failure: {
            missing_provider_message_ids: ["x", "y"],
            error_class: "detail_retrieval_failed",
            message: "429",
          },
        },
        { backfill_complete: true, last_full_sync_at: 200, failure: null },
      ),
      false,
    );
    expect(partial.kind).toBe("partial");
    expect(partial.label).toContain("2 封待重试");

    const done = deriveSyncUiState(
      status(
        { backfill_complete: true, last_full_sync_at: 100, failure: null },
        { backfill_complete: true, last_full_sync_at: 200, failure: null },
      ),
      false,
    );
    expect(done.kind).toBe("completed");
    expect(done.lastSuccessAt).toBe(200);
  });

  it("warns about provider retention while the initial backfill is unfinished", () => {
    const state = deriveSyncUiState(
      status({ backfill_complete: false, last_page_at: 1, last_full_sync_at: null, failure: null }, null),
      false,
    );
    expect(state.retentionWarning).toBe(true);
  });
});

describe("createAutoSyncTrigger", () => {
  it("fires once per interval per mailbox and never for ineligible mailboxes", () => {
    let clock = 0;
    const run = vi.fn();
    const trigger = createAutoSyncTrigger(run, () => clock);

    expect(trigger(null, true)).toBe(false);
    expect(trigger("mailbox-1", false)).toBe(false);
    expect(trigger("mailbox-1", true)).toBe(true);
    expect(trigger("mailbox-1", true)).toBe(false);

    clock += 5 * 60 * 1000;
    expect(trigger("mailbox-1", true)).toBe(true);
    // A different mailbox has its own budget.
    expect(trigger("mailbox-2", true)).toBe(true);
    expect(run).toHaveBeenCalledTimes(3);
  });
});

describe("invalidateMailboxMailQueries", () => {
  it("invalidates folders, threads, drafts, and the open thread", async () => {
    const queryClient = new QueryClient();
    const spy = vi.spyOn(queryClient, "invalidateQueries").mockResolvedValue();

    await invalidateMailboxMailQueries(queryClient, "mailbox-1", "thread-9");

    const keys = spy.mock.calls.map(([filters]) => JSON.stringify(filters?.queryKey));
    expect(keys).toContain(JSON.stringify(emailFolderListKey("mailbox-1")));
    expect(keys).toContain(
      JSON.stringify(emailThreadListKey("mailbox-1", "all", "newest").slice(0, 2)),
    );
    expect(keys).toContain(JSON.stringify(emailDraftListKey("mailbox-1", null).slice(0, 2)));
    expect(keys).toContain(JSON.stringify(emailThreadKey("thread-9")));
  });
});

describe("invalidateEmailTemplateQueries", () => {
  it("invalidates the template list family", async () => {
    const queryClient = new QueryClient();
    const spy = vi.spyOn(queryClient, "invalidateQueries").mockResolvedValue();

    await invalidateEmailTemplateQueries(queryClient);

    expect(spy).toHaveBeenCalledTimes(1);
    expect(spy.mock.calls[0][0]?.queryKey).toEqual(EMAIL_TEMPLATE_LIST_QUERY_KEY);
  });
});

describe("duplicateEmailTemplate", () => {
  beforeEach(() => {
    creatorInvokeMock.mockReset();
  });

  it("reads the full template and saves a fresh copy under a derived name", async () => {
    const source: EmailTemplate = {
      id: "tpl-1",
      name: "Intro",
      subject: "Hello {{first_name}}",
      text: "Hi {{first_name}}",
      html: "<p>Hi {{first_name}}</p>",
      variables: ["first_name"],
      revision: 4,
      created_at: 1,
      updated_at: 2,
    };
    const saved: EmailTemplate = { ...source, id: "tpl-2", name: "Intro 副本 2", revision: 1 };
    creatorInvokeMock.mockResolvedValueOnce(source).mockResolvedValueOnce(saved);

    const result = await duplicateEmailTemplate("tpl-1", ["Intro", "Intro 副本"]);

    expect(result).toBe(saved);
    expect(creatorInvokeMock).toHaveBeenCalledTimes(2);
    expect(creatorInvokeMock.mock.calls[0]).toEqual([
      "onecrew_email_template_get",
      { input: { template_id: "tpl-1" } },
    ]);
    expect(creatorInvokeMock.mock.calls[1]).toEqual([
      "onecrew_email_template_save",
      {
        input: {
          id: null,
          name: "Intro 副本 2",
          subject: source.subject,
          text: source.text,
          html: source.html,
          expected_revision: null,
        },
      },
    ]);
  });

  it("does not save anything when the source read fails", async () => {
    creatorInvokeMock.mockRejectedValueOnce(new Error("missing"));

    await expect(duplicateEmailTemplate("tpl-1", [])).rejects.toThrow("missing");
    expect(creatorInvokeMock).toHaveBeenCalledTimes(1);
  });
});

describe("listAllEmailTemplates pagination guard", () => {
  beforeEach(() => {
    creatorInvokeMock.mockReset();
  });

  it("throws a stable typed semantic error instead of presentation copy", async () => {
    creatorInvokeMock.mockResolvedValue({ items: [], next_cursor: "next" });

    const error = await listAllEmailTemplates().catch((caught: unknown) => caught);

    expect(isEmailTemplatePaginationLimitError(error)).toBe(true);
    expect(error).toMatchObject({
      name: "EmailTemplatePaginationLimitError",
      code: EMAIL_TEMPLATE_PAGINATION_LIMIT_CODE,
      message: EMAIL_TEMPLATE_PAGINATION_LIMIT_CODE,
    });
    expect(creatorInvokeMock).toHaveBeenCalledTimes(50);
  });
});

describe("emailThreadListKey", () => {
  it("keeps sort direction in the cache identity", () => {
    expect(emailThreadListKey("mailbox-1", "inbox", "newest")).not.toEqual(
      emailThreadListKey("mailbox-1", "inbox", "oldest"),
    );
  });
});
