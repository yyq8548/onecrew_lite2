import { FilePenLine, Inbox, Reply } from "lucide-react";
import type { TFunction } from "i18next";
import { useTranslation } from "react-i18next";

import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Empty, EmptyDescription, EmptyHeader, EmptyMedia, EmptyTitle } from "@/components/ui/empty";
import { ScrollArea } from "@/components/ui/scroll-area";
import { formatDate, type EmailDraftSummary, type EmailThreadSummary } from "@/lib/creator";
import { ReplyIntentTag } from "@/components/reply-intent-tag";
import { cn } from "@/lib/utils";

type MailListItem =
  | { kind: "thread"; id: string; value: EmailThreadSummary }
  | { kind: "draft"; id: string; value: EmailDraftSummary };

function matchesQuery(item: MailListItem, normalized: string) {
  if (!normalized) return true;
  if (item.kind === "draft") {
    return [item.value.to, item.value.subject, item.value.preview_text]
      .join(" ")
      .toLocaleLowerCase()
      .includes(normalized);
  }
  const { thread, latest_message: message, creator_display_name } = item.value;
  return [thread.subject, creator_display_name, message?.from, ...(message?.to ?? []), message?.preview_text]
    .filter(Boolean)
    .join(" ")
    .toLocaleLowerCase()
    .includes(normalized);
}

export function mailHistoryIndicator(
  messageCount: number,
  hasOutbound: boolean,
  t: TFunction<"mail">,
) {
  if (messageCount <= 1) return null;
  return {
    countLabel: messageCount > 99 ? "99+" : String(messageCount),
    accessibleLabel: hasOutbound
      ? t("list.historyWithSent", { count: messageCount })
      : t("list.history", { count: messageCount }),
    showReply: hasOutbound,
  };
}

export function MailItemList({
  threads,
  drafts,
  selectedThreadId,
  query,
  emptyDescription,
  onSelectThread,
  onSelectDraft,
}: {
  threads: EmailThreadSummary[];
  drafts: EmailDraftSummary[];
  selectedThreadId: string | null;
  query: string;
  emptyDescription: string;
  onSelectThread: (threadId: string) => void;
  onSelectDraft: (draftId: string) => void;
}) {
  const { t } = useTranslation("mail");
  const normalized = query.trim().toLocaleLowerCase();
  const items: MailListItem[] = [
    ...drafts.map((draft): MailListItem => ({ kind: "draft", id: draft.id, value: draft })),
    ...threads.map((thread): MailListItem => ({ kind: "thread", id: thread.thread.id, value: thread })),
  ]
    .filter((item) => matchesQuery(item, normalized));

  return (
    <ScrollArea className="h-full">
      <div className="flex flex-col gap-2 p-4 pt-0">
        {items.map((item) => {
          if (item.kind === "draft") {
            const draft = item.value;
            return (
              <Button
                key={`draft:${draft.id}`}
                variant="outline"
                className="h-auto w-full flex-col items-stretch gap-2 whitespace-normal p-3 text-left font-normal"
                onClick={() => onSelectDraft(draft.id)}
              >
                <span className="flex min-w-0 items-center gap-2">
                  <FilePenLine className="size-4 shrink-0 text-muted-foreground" />
                  <span className="truncate font-semibold">{draft.to || t("list.recipientMissing")}</span>
                  <Badge variant={draft.state === "trashed" ? "outline" : "secondary"} className="ml-auto shrink-0">
                    {draft.sending ? t("list.sending") : draft.state === "trashed" ? t("list.deletedDraft") : t("list.draft")}
                  </Badge>
                </span>
                <span className="flex min-w-0 items-center gap-2 text-xs">
                  <span className="truncate font-medium">{draft.subject || t("common.noSubject")}</span>
                  <span className="ml-auto shrink-0 text-muted-foreground">{formatDate(draft.updated_at)}</span>
                </span>
                <span className="line-clamp-2 w-full text-xs leading-5 text-muted-foreground">
                  {draft.preview_text || t("list.bodyMissing")}
                </span>
              </Button>
            );
          }

          const { thread, latest_message: message, creator_display_name } = item.value;
          const history = mailHistoryIndicator(item.value.message_count, item.value.has_outbound, t);
          // Unread has to be legible from the row itself, not from a dot alone:
          // a dot that never changes weight reads as decoration, so opening a
          // thread visibly settles the sender and subject back to normal ink.
          const unread = thread.unread_count > 0;
          return (
            <Button
              key={`thread:${thread.id}`}
              data-thread-id={thread.id}
              variant="outline"
              className={cn(
                "h-auto w-full flex-col items-stretch gap-2 whitespace-normal p-3 text-left font-normal transition-colors hover:bg-accent",
                selectedThreadId === thread.id && "bg-muted",
              )}
              onClick={() => onSelectThread(thread.id)}
            >
              <span className="flex w-full min-w-0 items-center gap-2">
                <span className={cn("truncate", unread ? "font-semibold" : "font-medium")}>
                  {creator_display_name
                    ?? (message?.direction === "outbound" ? message.to[0] : message?.from)
                    ?? t("list.contactMissing")}
                </span>
                {unread && (
                  <span
                    aria-label={t("common.unread")}
                    className="size-2 shrink-0 rounded-full bg-[var(--brand-accent)]"
                  />
                )}
                <span className="ml-auto shrink-0 text-xs text-muted-foreground">{formatDate(message?.occurred_at ?? thread.updated_at)}</span>
              </span>
              <span className="flex w-full min-w-0 items-center gap-2">
                <span
                  className={cn(
                    "min-w-0 flex-1 truncate text-xs",
                    unread ? "font-semibold" : "font-normal text-muted-foreground",
                  )}
                >
                  {thread.subject || t("common.noSubject")}
                </span>
                {history && (
                  <span
                    role="img"
                    aria-label={history.accessibleLabel}
                    className="ml-auto flex shrink-0 items-center gap-1 text-muted-foreground"
                  >
                    {history.showReply && <Reply aria-hidden="true" className="size-3.5" />}
                    <span className="min-w-5 rounded bg-muted px-1.5 text-center text-[11px] leading-5">
                      {history.countLabel}
                    </span>
                  </span>
                )}
              </span>
              <span className="line-clamp-2 w-full text-xs leading-5 text-muted-foreground">
                {message?.preview_text ?? (message?.has_html ? t("common.htmlEmail") : t("common.noBody"))}
              </span>
              {/* Agent 对这条来信的判读:标签给结论,hover 给理由和草稿。 */}
              {item.value.triage && (
                <span className="flex w-full">
                  <ReplyIntentTag triage={item.value.triage} />
                </span>
              )}
            </Button>
          );
        })}
        {!items.length && (
          <Empty className="py-14">
            <EmptyHeader>
              <EmptyMedia variant="icon"><Inbox /></EmptyMedia>
              <EmptyTitle>{normalized ? t("list.noMatchesTitle") : t("list.emptyFolderTitle")}</EmptyTitle>
              <EmptyDescription>{normalized ? t("list.noMatchesDescription") : emptyDescription}</EmptyDescription>
            </EmptyHeader>
          </Empty>
        )}
      </div>
    </ScrollArea>
  );
}
