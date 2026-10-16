import { useMemo, useState, type Ref } from "react";
import {
  Archive,
  ArrowLeft,
  Inbox,
  MailOpen,
  MoreHorizontal,
  Trash2,
} from "lucide-react";
import { useTranslation } from "react-i18next";
import { EmailChainTurns } from "@/components/mail/email-chain";
import {
  shouldShowArchiveAction,
  shouldShowRestoreAction,
  shouldShowUnarchiveAction,
} from "@/components/mail/disposition-policy";
import { parseEmailChain, quoteSummary, splitQuotedBody } from "@/components/mail/quoted-body";
import { Button } from "@/components/ui/button";
import { Empty, EmptyDescription, EmptyHeader, EmptyMedia, EmptyTitle } from "@/components/ui/empty";
import { ScrollArea } from "@/components/ui/scroll-area";
import { Separator } from "@/components/ui/separator";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import {
  formatDate,
  type EmailFolder,
  type EmailMessageView,
  type EmailThreadDispositionAction,
  type EmailThreadFolder,
  type MailboxDetail,
} from "@/lib/creator";

export function mailboxCanSend(mailbox: MailboxDetail | null) {
  return Boolean(mailbox?.available
    && mailbox.capabilities.includes("send")
    && mailbox.capabilities.includes("sent_readback"));
}

export function mailboxCanReceive(mailbox: MailboxDetail | null) {
  return Boolean(mailbox?.available && mailbox.capabilities.includes("inbox"));
}

export function MailboxUnavailable({ mailbox }: { mailbox: MailboxDetail | null }) {
  const { t } = useTranslation("mail");
  if (!mailbox) {
    return (
      <div className="border-b border-amber-500/30 bg-amber-500/5 px-4 py-3 text-xs text-amber-700 dark:text-amber-300">
        {t("mailbox.banners.disconnected")}
      </div>
    );
  }
  if (!mailbox.available) {
    return (
      <div className="border-b border-destructive/30 bg-destructive/5 px-4 py-3 text-xs text-destructive">
        {t("mailbox.banners.unavailable")}
      </div>
    );
  }
  if (!mailbox.capabilities.includes("inbox")) {
    return (
      <div className="border-b border-amber-500/30 bg-amber-500/5 px-4 py-3 text-xs text-amber-700 dark:text-amber-300">
        {t("mailbox.banners.cannotReceive")}
      </div>
    );
  }
  if (mailbox.provider === "gmail") {
    return <div className="border-b px-4 py-2 text-xs text-muted-foreground">{t("mailbox.banners.gmailInbox")}</div>;
  }
  return null;
}
export function EmailMessageDisplay({
  messages,
  selectedId,
  onBack,
  backButtonRef,
  threadFolder,
  viewFolder,
  dispositionPending = false,
  onDispositionAction,
}: {
  messages: EmailMessageView[];
  selectedId?: string | null;
  onBack?: () => void;
  backButtonRef?: Ref<HTMLButtonElement>;
  threadFolder?: EmailThreadFolder;
  viewFolder?: EmailFolder;
  dispositionPending?: boolean;
  onDispositionAction?: (action: EmailThreadDispositionAction) => void;
}) {
  const { t } = useTranslation("mail");
  // 面板内部的展开选择;调用方给的 selectedId 只是初始聚焦。
  const [openedId, setOpenedId] = useState<string | null>(null);
  const focused =
    messages.find((message) => message.id === (openedId ?? selectedId))
    ?? messages[0]
    ?? null;

  if (!focused) {
    return (
      <Empty className="h-full">
        <EmptyHeader>
          <EmptyMedia variant="icon"><MailOpen /></EmptyMedia>
          <EmptyTitle>{t("message.emptyTitle")}</EmptyTitle>
          <EmptyDescription>{t("message.emptyDescription")}</EmptyDescription>
        </EmptyHeader>
      </Empty>
    );
  }
  return (
    <div className="flex h-full min-h-0 flex-col">
      <div className="flex h-14 shrink-0 items-center gap-1 border-b px-2">
        {onBack && <Button ref={backButtonRef} variant="ghost" size="icon" onClick={onBack} aria-label={t("message.backToList")}><ArrowLeft /></Button>}
        {onDispositionAction && shouldShowArchiveAction(viewFolder, threadFolder) && (
          <Tooltip>
            <TooltipTrigger asChild>
              <Button variant="ghost" size="icon" disabled={dispositionPending} onClick={() => onDispositionAction("archive")} aria-label={t("message.actions.archive")}>
                <Archive />
              </Button>
            </TooltipTrigger>
            <TooltipContent>{t("message.actions.archiveTooltip")}</TooltipContent>
          </Tooltip>
        )}
        {onDispositionAction && shouldShowUnarchiveAction(viewFolder, threadFolder) && (
          <Tooltip>
            <TooltipTrigger asChild>
              <Button variant="ghost" size="icon" disabled={dispositionPending} onClick={() => onDispositionAction("unarchive")} aria-label={t("message.actions.unarchive")}>
                <Inbox />
              </Button>
            </TooltipTrigger>
            <TooltipContent>{t("message.actions.unarchiveTooltip")}</TooltipContent>
          </Tooltip>
        )}
        {onDispositionAction && shouldShowRestoreAction(viewFolder, threadFolder) && (
          <Tooltip>
            <TooltipTrigger asChild>
              <Button variant="ghost" size="icon" disabled={dispositionPending} onClick={() => onDispositionAction("restore")} aria-label={t("message.actions.restore")}>
                <Inbox />
              </Button>
            </TooltipTrigger>
            <TooltipContent>{t("message.actions.restoreTooltip")}</TooltipContent>
          </Tooltip>
        )}
        {onDispositionAction && threadFolder !== "deleted" && (
          <Tooltip>
            <TooltipTrigger asChild>
              <Button variant="ghost" size="icon" disabled={dispositionPending} onClick={() => onDispositionAction("delete")} aria-label={t("message.actions.delete")}>
                <Trash2 />
              </Button>
            </TooltipTrigger>
            <TooltipContent>{t("message.actions.deleteTooltip")}</TooltipContent>
          </Tooltip>
        )}
      </div>
      <ScrollArea className="min-h-0 flex-1">
        <div className="divide-y">
          {messages.map((message) =>
            message.id === focused.id ? (
              <div key={message.id}>
                <EmailMessageHeader message={message} />
                <Separator />
                <EmailMessageBody message={message} />
              </div>
            ) : (
              <button
                key={message.id}
                type="button"
                onClick={() => setOpenedId(message.id)}
                className="flex w-full items-center gap-3 px-4 py-2.5 text-left transition-colors hover:bg-muted/50"
              >
                <span className="grid size-7 shrink-0 place-items-center rounded-full bg-muted text-xs font-semibold text-muted-foreground">
                  {(message.direction === "outbound" ? t("common.me") : message.from).slice(0, 1).toUpperCase()}
                </span>
                <span className="min-w-0 flex-1">
                  <span className="block truncate text-sm font-medium">
                    {message.direction === "outbound" ? t("common.me") : message.from}
                  </span>
                  <span className="block truncate text-xs text-muted-foreground">
                    {splitQuotedBody(message.text).body.split("\n").find((line) => line.trim())
                      ?? t("common.emptyBody")}
                  </span>
                </span>
                <span className="shrink-0 text-xs text-muted-foreground">
                  {formatDate(message.occurred_at)}
                </span>
              </button>
            ),
          )}
        </div>
      </ScrollArea>
    </div>
  );
}

function EmailMessageHeader({ message }: { message: EmailMessageView }) {
  const { t } = useTranslation("mail");
  return (
    <div className="flex shrink-0 items-start gap-3 p-4">
      <span className="grid size-10 shrink-0 place-items-center rounded-full bg-primary/10 text-sm font-semibold text-primary">
        {(message.direction === "outbound" ? t("common.me") : message.from).slice(0, 1).toUpperCase()}
      </span>
      <div className="min-w-0 flex-1">
        <div className="flex items-start gap-2">
          <div className="min-w-0 flex-1">
            <div className="truncate text-sm font-semibold">{message.direction === "outbound" ? t("common.me") : message.from}</div>
            <div className="truncate text-xs text-muted-foreground">
              {message.direction === "outbound"
                ? t("message.sentTo", { recipients: message.to.join(", ") })
                : t("message.replyTo", { address: message.reply_to[0] ?? message.from })}
            </div>
          </div>
          <span className="shrink-0 text-xs text-muted-foreground">{formatDate(message.occurred_at)}</span>
        </div>
        <div className="mt-2 text-sm font-medium">{message.subject || t("common.noSubject")}</div>
      </div>
    </div>
  );
}

/**
 * A reply repeats the whole conversation. Shown as one block of text, the
 * sentences this message actually adds are indistinguishable from the ones
 * already read, so the quoted history is folded away behind a control and
 * rendered as a quote when opened — never dropped.
 */
function EmailMessageBody({ message }: { message: EmailMessageView }) {
  const { t } = useTranslation("mail");
  const quoted = useMemo(() => splitQuotedBody(message.text), [message.text]);
  // 第 0 轮就是这封信自己说的话,上面已经渲染过了;引文区从第 1 轮起。
  const history = useMemo(() => parseEmailChain(message.text).slice(1), [message.text]);
  const [quoteOpen, setQuoteOpen] = useState(false);

  if (!message.text) {
    return (
      <p className="p-5 text-sm text-muted-foreground">
        {message.html ? t("message.htmlOnly") : t("message.bodyMissing")}
      </p>
    );
  }
  return (
    <div className="max-w-[72ch] space-y-3 p-5">
      {quoted.body ? (
        <div className="whitespace-pre-wrap text-sm leading-6">{quoted.body}</div>
      ) : (
        <p className="text-sm text-muted-foreground">{t("message.quoteOnly")}</p>
      )}
      {quoted.quote && (
        <div>
          <Button
            variant="ghost"
            size="sm"
            aria-expanded={quoteOpen}
            onClick={() => setQuoteOpen((open) => !open)}
            className="h-7 max-w-full gap-1.5 px-2 text-xs text-muted-foreground"
          >
            <MoreHorizontal className="shrink-0" />
            <span className="truncate">
              {quoteOpen ? t("message.hideQuote") : quoteSummary(quoted, t("message.quoteFallback"))}
            </span>
          </Button>
          {quoteOpen && (
            <blockquote className="mt-2 space-y-3 border-l-2 border-border pl-4">
              {/* 撰写区和阅读区共用同一套还原:每轮一张抬头卡加正文,而
                  不是把累积下来的 `>` 原样倒出来。 */}
              <EmailChainTurns turns={history} />
            </blockquote>
          )}
        </div>
      )}
    </div>
  );
}
