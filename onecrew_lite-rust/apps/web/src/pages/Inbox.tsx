// 私聊收件箱:左会话列表 + 右聊天线程(Mail.tsx 的双栏先例,shadcn
// chat 形态)。列表来自 onecrew_conversation_list(纯本地筛选读,newest first);
// 右栏复用 message-workspace 的 MessageWorkspace —— 线程由 (creator,
// route, operator) 三元定位,本地会话读另按会话 ID 取:已有的行两者都
// 直接携带,手动发起从「新建对话」走搜索 Creator → 选通道 → 选平台账号
// (Telegram 未识别路由先经 TelegramTargetDialog 识别),会话 ID 要等首次
// 同步之后才有。单线程刷新仍是 workspace 里的显式 sync。

import { useCallback, useEffect, useMemo, useRef, useState, type RefObject } from "react";
import { flushSync } from "react-dom";
import { useSize } from "radix-ui/internal";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { isRuntimeAvailable } from "@/lib/local-runtime";
import { useTranslation } from "react-i18next";
import {
  ArrowLeft,
  Loader2,
  MessageCircle,
  PenSquare,
  Search,
} from "lucide-react";

import { Input } from "@/components/ui/input";
import { Button } from "@/components/ui/button";
import { RefreshButton } from "@/components/ui/refresh-button";
import { Badge } from "@/components/ui/badge";
import { ScrollArea } from "@/components/ui/scroll-area";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import {
  Command,
  CommandEmpty,
  CommandGroup,
  CommandInput,
  CommandItem,
  CommandList,
} from "@/components/ui/command";
import {
  ResizableHandle,
  ResizablePanel,
  ResizablePanelGroup,
} from "@/components/ui/resizable";
import { CreatorIdentityAvatar, PlatformIcon } from "@/components/platform";
import {
  MessageWorkspace,
  TelegramTargetDialog,
  socialChannels,
  type MessageIdentityStatus,
  type OutreachChannel,
} from "@/components/message-workspace";
import { ReplyIntentTag } from "@/components/reply-intent-tag";
import { cn } from "@/lib/utils";
import { useCanvasPortal } from "@/lib/canvas-portal";
import { creatorDetailQueryOptions } from "@/lib/runtime-query";
import { connectedProfilesQueryOptions } from "@/lib/connected-profiles";
import {
  creatorRecordAvatarUrl,
  creatorInvoke,
  PLATFORM_LABEL,
  type ContactRouteLastContactAccount,
  type CreatorRecord,
  type CreatorSummary,
  type CreatorPlatform,
} from "@/lib/creator";
import type { Account, Platform } from "@/types";
import { DM_PLATFORMS, isDmPlatform } from "@/lib/dm-platforms";
import {
  conversationRowIsOpen,
  openThreadForConversation,
  openThreadForDraft,
  threadAfterConversationOpened,
  type OpenThread,
} from "@/pages/inbox-thread";
import {
  CONVERSATION_LIST_QUERY_KEY,
  conversationListInput,
  conversationListQueryKey,
  listConversations,
  type ConversationListStatus,
  type ConversationSummary,
} from "@/lib/conversations";

const SELECTED_IDENTITY: MessageIdentityStatus = {
  kind: "selected",
  message: "",
};

export const NEW_CONVERSATION_DIALOG_CLASS =
  "flex max-h-[min(36rem,calc(100%-2rem))] w-[min(32rem,calc(100%-2rem))] max-w-[calc(100%-2rem)] min-w-0 flex-col gap-4 overflow-hidden sm:max-w-lg";
export const NEW_CONVERSATION_RESULTS_CLASS =
  "h-64 min-h-0 max-h-64 overflow-x-hidden overflow-y-auto";
export const NEW_CONVERSATION_RESULT_CLASS =
  "grid min-w-0 grid-cols-[auto_minmax(0,1fr)_auto] items-center gap-2.5 px-2.5 py-2";

// Below this Inbox width the list and the thread cannot sit side by side: at
// 860px the list (30%) keeps 258px, which is the 112px status select plus a
// 114px search field either side of the 8px gap and the 24px padding, while
// the thread keeps 602px for a title beside its 96px follow button.
const INBOX_SPLIT_MIN_WIDTH = 860;

const CONVERSATION_STATUS_OPTIONS = ["all", "waiting_reply", "replied"] as const;

const CONVERSATION_CHANNEL_OPTIONS = ["all", ...DM_PLATFORMS] as const;

function formatWhen(timestamp: number, locale: string): string {
  const date = new Date(timestamp * 1000);
  const now = new Date();
  const sameDay = date.toDateString() === now.toDateString();
  return sameDay
    ? date.toLocaleTimeString(locale, { hour: "2-digit", minute: "2-digit" })
    : date.toLocaleDateString(locale, { month: "2-digit", day: "2-digit" });
}

function ConversationRow({
  summary,
  active,
  onSelect,
}: {
  summary: ConversationSummary;
  active: boolean;
  onSelect: () => void;
}) {
  const { i18n, t } = useTranslation("inbox");
  const preview = summary.last_message?.preview ?? t("conversations.noMessages");
  // 球在我们手里:对方最后发言、我们还没回。与侧边栏数字同一条判据。
  // 它与意向标签正交——人工同步和 agent 同步都会带来新消息,但只有
  // agent 判读过才会有标签。
  const waiting = summary.last_message?.direction === "inbound";
  return (
    <button
      type="button"
      data-conversation-id={summary.conversation_id}
      onClick={onSelect}
      className={cn(
        "flex w-full items-start gap-2.5 rounded-lg px-3 py-2.5 text-left transition-colors",
        active ? "bg-muted" : "hover:bg-muted/50",
      )}
    >
      <CreatorIdentityAvatar
        platform={summary.channel as CreatorPlatform}
        name={summary.creator_display_name}
        src={summary.creator_avatar_url}
        className="mt-0.5 size-9"
      />
      <span className="min-w-0 flex-1">
        <span className="flex items-baseline justify-between gap-2">
          <span
            className={cn(
              "truncate text-sm",
              waiting ? "font-semibold" : "font-medium",
            )}
          >
            {summary.creator_display_name}
          </span>
          {waiting && (
            <span
              aria-label={t("conversations.waitingReplyAria")}
              className="size-2 shrink-0 self-center rounded-full bg-[var(--brand-accent)]"
            />
          )}
          <span className="ml-auto shrink-0 text-[11px] text-muted-foreground">
            {formatWhen(summary.updated_at, i18n.resolvedLanguage ?? i18n.language)}
          </span>
        </span>
        <span className="mt-0.5 block truncate text-xs text-muted-foreground">
          {summary.last_message?.direction === "outbound"
            ? t("conversations.outboundPrefix")
            : ""}
          {preview}
        </span>
        <span className="mt-1 flex items-center gap-1.5">
          {/* 时间下方的意向标签:agent 的判读就长在它判读的那条来信旁边。 */}
          {summary.triage && <ReplyIntentTag triage={summary.triage} />}
          <span className="min-w-0 truncate text-[11px] text-muted-foreground/70">
            {t("conversations.via", { account: summary.operator_account_label })}
          </span>
        </span>
      </span>
    </button>
  );
}

/** 右栏:按线程的 creator 拉全量记录,把会话交给 MessageWorkspace。 */
function ConversationThread({ thread, onConversationOpened, onBack, backButtonRef }: {
  thread: OpenThread;
  onConversationOpened: (conversationId: string) => void;
  /** 只有单栏布局给:线程盖住列表,得有路回去。同时让工作区顶栏收窄。 */
  onBack?: () => void;
  backButtonRef?: (node: HTMLButtonElement) => void;
}) {
  const { t } = useTranslation("inbox");
  const detail = useQuery(creatorDetailQueryOptions(thread.creatorId, isRuntimeAvailable()));
  const record = detail.data ?? null;
  const channel = useMemo(
    () =>
      socialChannels(record).find(
        (candidate) => candidate.contactRouteId === thread.contactRouteId,
      ) ?? null,
    [record, thread.contactRouteId],
  );

  // 读取中、失败和正常三种状态各自渲染同一个返回控件,焦点才能从离开的那个
  // 交到到来的那个 —— 与 Mail 的单栏阅读器同一做法。
  const backButton = onBack ? (
    <Button
      ref={backButtonRef}
      variant="ghost"
      size="icon"
      className="-ml-1 size-8 shrink-0"
      aria-label={t("conversations.backToList")}
      onClick={onBack}
    >
      <ArrowLeft className="size-4" />
    </Button>
  ) : null;

  if (detail.isLoading) {
    return (
      <div className="flex h-full min-h-0 flex-col">
        {backButton && <div className="flex h-12 shrink-0 items-center border-b px-4">{backButton}</div>}
        <div className="grid min-h-0 flex-1 place-items-center text-muted-foreground">
          <Loader2 className="size-5 animate-spin" />
        </div>
      </div>
    );
  }
  if (detail.error) {
    return (
      <div className="flex h-full min-h-0 flex-col">
        {backButton && <div className="flex h-12 shrink-0 items-center border-b px-4">{backButton}</div>}
        <div className="grid min-h-0 flex-1 place-items-center p-6">
          <p className="max-w-md break-words text-center text-sm text-destructive">
            {t("conversations.creatorLoadFailed", { error: String(detail.error) })}
          </p>
        </div>
      </div>
    );
  }
  if (!record || !channel) {
    return (
      <div className="flex h-full min-h-0 flex-col">
        {backButton && <div className="flex h-12 shrink-0 items-center border-b px-4">{backButton}</div>}
        <div className="grid min-h-0 flex-1 place-items-center p-6 text-sm text-muted-foreground">
          {t("conversations.routeMissing")}
        </div>
      </div>
    );
  }
  return (
    <div className="flex h-full min-h-0 flex-col">
      <div className="flex h-12 shrink-0 items-center gap-2 border-b px-4">
        {backButton}
        <CreatorIdentityAvatar
          platform={channel.platform}
          name={record.creator.display_name}
          src={creatorRecordAvatarUrl(record)}
          className="size-7"
        />
        <span className="truncate text-sm font-medium">{record.creator.display_name}</span>
        <span className="truncate text-xs text-muted-foreground">
          {t("conversations.threadIdentity", {
            handle: channel.handle,
            account: thread.accountLabel,
          })}
        </span>
      </div>
      <div className="min-h-0 flex-1">
        <MessageWorkspace
          key={thread.key}
          record={record}
          channel={channel}
          operatorId={thread.operatorId}
          identityStatus={SELECTED_IDENTITY}
          conversationId={thread.conversationId}
          onConversationOpened={onConversationOpened}
          narrow={onBack != null}
        />
      </div>
    </div>
  );
}

/** 一条建联通道:平台图标 + handle 与可用账号横排在同一行,上次联系过
 * 的账号带徽标并排在最前(与消息工作区的身份选择同语义)。 */
function ChannelRow({
  creatorId,
  channel,
  accounts,
  onStart,
}: {
  creatorId: string;
  channel: OutreachChannel;
  accounts: Account[];
  onStart: (channel: OutreachChannel, operatorId: string, label: string) => void;
}) {
  const { t } = useTranslation("inbox");
  const lastContact = useQuery({
    queryKey: ["creator-contact-route-last-contact-account", creatorId, channel.contactRouteId],
    queryFn: () =>
      creatorInvoke<ContactRouteLastContactAccount | null>("onecrew_contact_route_last_contact_account", {
        input: { creator_id: creatorId, contact_route_id: channel.contactRouteId },
      }),
    enabled: isRuntimeAvailable(),
    retry: false,
    staleTime: 30_000,
  });
  const recentId = lastContact.data?.account_ref.id ?? null;
  const candidates = useMemo(() => {
    const compatible = accounts.filter((account) => account.platform === channel.platform);
    return [...compatible].sort(
      (a, b) => Number(b.id === recentId) - Number(a.id === recentId),
    );
  }, [accounts, channel.platform, recentId]);

  return (
    <div className="flex flex-wrap items-center gap-x-2.5 gap-y-1.5 rounded-lg border px-3 py-2.5">
      <span className="flex min-w-0 shrink-0 items-center gap-1.5 text-sm font-medium">
        <PlatformIcon platform={channel.platform} className="size-4" />
        <span className="truncate">@{channel.handle}</span>
        {channel.requiresIdentification && (
          <Badge variant="secondary" className="shrink-0 text-[10px]">
            {t("conversations.channel.needsIdentification")}
          </Badge>
        )}
      </span>
      <span className="ml-auto flex flex-wrap items-center justify-end gap-1.5">
        {candidates.length === 0 ? (
          <span className="text-xs text-muted-foreground">
            {t("conversations.channel.noConnectedAccount", {
              platform: PLATFORM_LABEL[channel.platform],
            })}
          </span>
        ) : (
          candidates.map((account) => (
            <Button
              key={account.id}
              size="sm"
              variant={account.id === recentId ? "default" : "outline"}
              className="h-7"
              onClick={() => onStart(channel, account.id, `@${account.username}`)}
            >
              @{account.username}
              {account.id === recentId && (
                <Badge
                  variant="secondary"
                  className="ml-1 shrink-0 px-1 text-[9px] font-normal"
                >
                  {t("conversations.channel.lastContact")}
                </Badge>
              )}
            </Button>
          ))
        )}
      </span>
    </div>
  );
}

/** 手动发起:搜索 Creator → 选私聊通道 → 选平台账号。Telegram 未识别
 * 路由由调用方转交 TelegramTargetDialog。 */
function NewConversationDialog({
  open,
  onOpenChange,
  onStart,
  onIdentifyTelegram,
  restoreFocusRef,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onStart: (record: CreatorRecord, channel: OutreachChannel, operatorId: string, accountLabel: string) => void;
  onIdentifyTelegram: (record: CreatorRecord, channel: OutreachChannel, operatorId: string, accountLabel: string) => void;
  restoreFocusRef: RefObject<HTMLButtonElement | null>;
}) {
  const { i18n, t } = useTranslation("inbox");
  const runtimeAvailable = isRuntimeAvailable();
  const canvas = useCanvasPortal();
  const [query, setQuery] = useState("");
  const [creatorId, setCreatorId] = useState<string | null>(null);

  const creators = useQuery({
    queryKey: ["creator-inbox-creator-search", query],
    queryFn: () =>
      creatorInvoke<{ items: CreatorSummary[] }>("onecrew_creator_list", {
        input: { query: query.trim() || null, limit: 20 },
      }),
    enabled: runtimeAvailable && open && creatorId == null,
    retry: false,
    staleTime: 15_000,
  });
  const detail = useQuery(creatorDetailQueryOptions(creatorId, runtimeAvailable && open));
  const accounts = useQuery(connectedProfilesQueryOptions(runtimeAvailable && open));

  const record = detail.data ?? null;
  const channels = useMemo(() => socialChannels(record), [record]);

  const close = (next: boolean) => {
    onOpenChange(next);
    if (!next) {
      setQuery("");
      setCreatorId(null);
    }
  };

  const start = (channel: OutreachChannel, operatorId: string, label: string) => {
    if (!record) return;
    close(false);
    if (channel.requiresIdentification) {
      onIdentifyTelegram(record, channel, operatorId, label);
    } else {
      onStart(record, channel, operatorId, label);
    }
  };

  return (
    <Dialog open={open} onOpenChange={close}>
      <DialogContent
        container={canvas}
        className={NEW_CONVERSATION_DIALOG_CLASS}
        onCloseAutoFocus={(event) => {
          const target = restoreFocusRef.current;
          if (!target?.isConnected) return;
          event.preventDefault();
          target.focus();
        }}
      >
        <DialogHeader>
          <DialogTitle>{t("conversations.newDialog.title")}</DialogTitle>
          <DialogDescription>
            {creatorId == null
              ? t("conversations.newDialog.selectCreatorDescription")
              : t("conversations.newDialog.selectChannelDescription")}
          </DialogDescription>
        </DialogHeader>
        {creatorId == null ? (
          <Command shouldFilter={false} className="min-h-0 min-w-0 rounded-md border">
            <CommandInput
              autoFocus
              value={query}
              onValueChange={setQuery}
              placeholder={t("conversations.newDialog.searchPlaceholder")}
              aria-label={t("conversations.newDialog.searchAria")}
            />
            <CommandList className={NEW_CONVERSATION_RESULTS_CLASS}>
              {creators.isLoading ? (
                <div
                  className="grid place-items-center py-8 text-muted-foreground"
                  role="status"
                  aria-label={t("conversations.newDialog.loadingAria")}
                >
                  <Loader2 className="size-4 animate-spin" />
                </div>
              ) : creators.error ? (
                <div className="grid place-items-center gap-3 px-4 py-8 text-center" role="alert">
                  <p className="text-sm text-destructive">
                    {t("conversations.newDialog.creatorLoadFailed")}
                  </p>
                  <Button type="button" size="sm" variant="outline" onClick={() => void creators.refetch()}>
                    {t("common.reload")}
                  </Button>
                </div>
              ) : (
                <>
                  <CommandEmpty>{t("conversations.newDialog.noMatch")}</CommandEmpty>
                  <CommandGroup>
                    {(creators.data?.items ?? []).map((summary) => {
                      // 可私聊的通道 = 已保存的建联通道里 X / Instagram /
                      // Telegram 那几个,先摆出来再选人,免得进去才发现没通道。
                      const dmChannels = summary.channels.filter(isDmPlatform);
                      const channelLabel = dmChannels.length === 0
                        ? t("conversations.newDialog.noDmChannel")
                        : new Intl.ListFormat(
                            i18n.resolvedLanguage ?? i18n.language,
                            { style: "short", type: "conjunction" },
                          ).format(dmChannels.map((channel) => PLATFORM_LABEL[channel]));
                      return (
                        <CommandItem
                          key={summary.id}
                          value={summary.id}
                          onSelect={() => setCreatorId(summary.id)}
                          className={NEW_CONVERSATION_RESULT_CLASS}
                          aria-label={t("conversations.newDialog.resultAria", {
                            name: summary.display_name,
                            handle: summary.primary_handle
                              ? t("conversations.newDialog.resultHandle", {
                                  handle: summary.primary_handle,
                                })
                              : "",
                            channels: channelLabel,
                          })}
                        >
                          <CreatorIdentityAvatar
                            platform={summary.primary_platform}
                            name={summary.display_name}
                            src={summary.avatar_url}
                            className="size-8"
                          />
                          <span className="flex min-w-0 flex-col" aria-hidden="true">
                            <span className="truncate font-medium">{summary.display_name}</span>
                            {summary.primary_handle && (
                              <span className="truncate text-xs text-muted-foreground">
                                @{summary.primary_handle}
                              </span>
                            )}
                          </span>
                          <span className="flex shrink-0 items-center gap-1" aria-hidden="true">
                            {dmChannels.length === 0 ? (
                              <span className="text-[11px] text-muted-foreground/70">
                                {t("conversations.newDialog.noDmChannel")}
                              </span>
                            ) : (
                              dmChannels.map((channel) => (
                                <span
                                  key={channel}
                                  title={PLATFORM_LABEL[channel]}
                                  className="grid size-5 place-items-center rounded bg-muted"
                                >
                                  <PlatformIcon platform={channel} className="size-3" />
                                </span>
                              ))
                            )}
                          </span>
                        </CommandItem>
                      );
                    })}
                  </CommandGroup>
                </>
              )}
            </CommandList>
          </Command>
        ) : (
          <div className="min-h-0 space-y-3 overflow-x-hidden overflow-y-auto pr-1">
            <Button variant="ghost" size="sm" className="-ml-2" onClick={() => setCreatorId(null)}>
              <ArrowLeft className="size-4" />
              {t("conversations.newDialog.selectAgain")}
            </Button>
            {detail.isLoading ? (
              <div className="grid place-items-center py-8 text-muted-foreground">
                <Loader2 className="size-4 animate-spin" />
              </div>
            ) : detail.error ? (
              <div className="grid place-items-center gap-3 rounded-lg border border-destructive/30 px-4 py-8 text-center" role="alert">
                <p className="text-sm text-destructive">
                  {t("conversations.newDialog.detailLoadFailed")}
                </p>
                <Button type="button" size="sm" variant="outline" onClick={() => void detail.refetch()}>
                  {t("common.reload")}
                </Button>
              </div>
            ) : channels.length === 0 ? (
              <p className="text-sm text-muted-foreground">
                {t("conversations.newDialog.noChannels", {
                  platforms: new Intl.ListFormat(
                    i18n.resolvedLanguage ?? i18n.language,
                    { style: "short", type: "disjunction" },
                  ).format(DM_PLATFORMS.map((platform) => PLATFORM_LABEL[platform])),
                })}
              </p>
            ) : accounts.isLoading ? (
              <div className="grid place-items-center py-8 text-muted-foreground">
                <Loader2 className="size-4 animate-spin" />
              </div>
            ) : accounts.error ? (
              <div className="grid place-items-center gap-3 rounded-lg border border-destructive/30 px-4 py-8 text-center" role="alert">
                <p className="text-sm text-destructive">
                  {t("conversations.newDialog.accountsLoadFailed")}
                </p>
                <Button type="button" size="sm" variant="outline" onClick={() => void accounts.refetch()}>
                  {t("common.reload")}
                </Button>
              </div>
            ) : (
              <div className="space-y-2">
                {channels.map((channel) => (
                  <ChannelRow
                    key={channel.contactRouteId}
                    creatorId={creatorId}
                    channel={channel}
                    accounts={accounts.data ?? []}
                    onStart={start}
                  />
                ))}
              </div>
            )}
          </div>
        )}
      </DialogContent>
    </Dialog>
  );
}

export function Inbox() {
  const { t } = useTranslation("inbox");
  const runtimeAvailable = isRuntimeAvailable();
  const queryClient = useQueryClient();
  const [search, setSearch] = useState("");
  const [debouncedSearch, setDebouncedSearch] = useState("");
  const [status, setStatus] = useState<"all" | ConversationListStatus>("all");
  const [channel, setChannel] = useState<"all" | Platform>("all");
  const [openThread, setOpenThread] = useState<OpenThread | null>(null);
  // Inbox 自己的宽度决定布局,不是窗口的:侧栏和宿主面板(Codex 侧边面板)
  // 占掉的空间,视口断点看不见。首次测量之前先按双栏渲染。
  const [inboxRoot, setInboxRoot] = useState<HTMLDivElement | null>(null);
  const inboxWidth = useSize(inboxRoot)?.width;
  const singlePane = inboxWidth !== undefined && inboxWidth < INBOX_SPLIT_MIN_WIDTH;
  // 单栏时线程盖住列表。返回只把列表放回上层,不清掉 openThread:工作区一旦
  // 卸载,再打开就会重跑一次自动同步 —— 那是一次 provider 调用,而 Mail 的
  // 阅读器重读只是本地读。
  const [threadOnTop, setThreadOnTop] = useState(false);
  const listRef = useRef<HTMLDivElement>(null);
  const threadBackRef = useRef<HTMLButtonElement | null>(null);
  const threadBackHadFocus = useRef(false);
  // 读取中、失败和正常三种线程状态各渲染一个返回按钮,焦点从离开的那个交到
  // 到来的那个。
  const attachThreadBack = useCallback((node: HTMLButtonElement) => {
    threadBackRef.current = node;
    if (threadBackHadFocus.current) node.focus();
    threadBackHadFocus.current = false;
    return () => {
      threadBackHadFocus.current = document.activeElement === node;
      threadBackRef.current = null;
    };
  }, []);
  const [composeOpen, setComposeOpen] = useState(false);
  const composeButtonRef = useRef<HTMLButtonElement>(null);
  const [telegramTarget, setTelegramTarget] = useState<{
    record: CreatorRecord;
    channel: OutreachChannel;
    operatorId: string;
    accountLabel: string;
  } | null>(null);
  const accounts = useQuery(connectedProfilesQueryOptions(runtimeAvailable));
  useEffect(() => {
    const handle = setTimeout(() => setDebouncedSearch(search), 300);
    return () => clearTimeout(handle);
  }, [search]);

  const filters = useMemo(
    () => ({ query: debouncedSearch, status, channel }),
    [channel, debouncedSearch, status],
  );

  const conversations = useQuery({
    queryKey: conversationListQueryKey(filters),
    queryFn: () => listConversations(conversationListInput(filters)),
    enabled: runtimeAvailable,
    retry: false,
    staleTime: 15_000,
    refetchOnWindowFocus: true,
  });

  const rows = conversations.data?.items ?? [];
  const hasFilters = search.trim().length > 0 || status !== "all" || channel !== "all";

  const showThread = (thread: OpenThread) => {
    if (!singlePane) {
      setOpenThread(thread);
      setThreadOnTop(true);
      return;
    }
    // 线程盖住列表,焦点跟到线程的返回按钮。
    flushSync(() => {
      setOpenThread(thread);
      setThreadOnTop(true);
    });
    threadBackRef.current?.focus();
  };
  const backToList = () => {
    const openedId = openThread?.conversationId;
    flushSync(() => setThreadOnTop(false));
    threadBackHadFocus.current = false;
    // 列表保住了滚动位置,焦点回到刚打开的那一行。
    Array.from(listRef.current?.querySelectorAll<HTMLElement>("[data-conversation-id]") ?? [])
      .find((row) => row.dataset.conversationId === openedId)
      ?.focus();
  };
  const openExisting = (summary: ConversationSummary) => {
    showThread(openThreadForConversation(summary));
  };
  const adoptConversationId = (conversationId: string) => {
    setOpenThread((current) => threadAfterConversationOpened(current, conversationId));
  };
  const openDraft = (
    record: CreatorRecord,
    channel: OutreachChannel,
    operatorId: string,
    accountLabel: string,
  ) => {
    showThread(openThreadForDraft({
      creatorId: record.creator.id,
      contactRouteId: channel.contactRouteId,
      operatorId,
      accountLabel,
    }));
  };


  const listPane = (
    <div className="flex h-full min-h-0 flex-col">
      <div className="grid shrink-0 grid-cols-[minmax(0,1fr)_112px] gap-2 border-b p-3">
        <div className="relative min-w-0">
          <Search className="absolute left-2.5 top-1/2 size-3.5 -translate-y-1/2 text-muted-foreground" />
          <Input
            value={search}
            onChange={(event) => setSearch(event.target.value)}
            placeholder={t("conversations.filters.searchPlaceholder")}
            className="h-8 pl-8"
          />
        </div>
        <Select
          value={status}
          onValueChange={(value) => setStatus(value as "all" | ConversationListStatus)}
        >
          <SelectTrigger
            size="sm"
            className="w-full"
            aria-label={t("conversations.filters.statusAria")}
          >
            <SelectValue />
          </SelectTrigger>
          <SelectContent align="end" viewportClassName="h-auto">
            {CONVERSATION_STATUS_OPTIONS.map((option) => (
              <SelectItem key={option} value={option}>
                {t(`conversations.status.${option}`)}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
        <div className="col-span-2 flex items-center gap-2">
          <Select
            value={channel}
            onValueChange={(value) => setChannel(value as "all" | Platform)}
          >
            <SelectTrigger
              size="sm"
              className="w-28"
              aria-label={t("conversations.filters.channelAria")}
            >
              <SelectValue />
            </SelectTrigger>
            <SelectContent align="start" viewportClassName="h-auto">
              {CONVERSATION_CHANNEL_OPTIONS.map((option) => (
                <SelectItem key={option} value={option}>
                  {option === "all"
                    ? t("conversations.channelAll")
                    : PLATFORM_LABEL[option]}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
          <div className="ml-auto flex items-center gap-2">
            <Button
              ref={composeButtonRef}
              variant="outline"
              size="icon"
              className="size-8"
              aria-label={t("conversations.filters.newConversation")}
              title={t("conversations.filters.newConversation")}
              onClick={() => setComposeOpen(true)}
            >
              <PenSquare className="size-4" />
            </Button>
            <RefreshButton
              pending={conversations.isFetching}
              variant="outline"
              label={t("conversations.filters.refresh")}
              className="shrink-0"
              onClick={() => {
                void queryClient.invalidateQueries({ queryKey: CONVERSATION_LIST_QUERY_KEY });
              }}
            />
          </div>
        </div>
      </div>
      <ScrollArea className="min-h-0 flex-1">
        <div className="flex flex-col gap-0.5 p-2">
          {conversations.isLoading ? (
            <div className="grid place-items-center py-10 text-muted-foreground">
              <Loader2 className="size-5 animate-spin" />
            </div>
          ) : conversations.error ? (
            <p className="break-words p-3 text-sm text-destructive">
              {t("conversations.listLoadFailed", {
                error: String(conversations.error),
              })}
            </p>
          ) : rows.length === 0 ? (
            <div className="flex flex-col items-center gap-2 py-10 text-center text-sm text-muted-foreground">
              <MessageCircle className="size-6" />
              <p>
                {hasFilters
                  ? t("conversations.emptyFiltered")
                  : t("conversations.empty")}
              </p>
              {!hasFilters && (
                <p className="text-xs">
                  {t("conversations.emptyHelp")}
                </p>
              )}
            </div>
          ) : (
            rows.map((row) => (
              <ConversationRow
                key={row.conversation_id}
                summary={row}
                active={conversationRowIsOpen(openThread, row)}
                onSelect={() => openExisting(row)}
              />
            ))
          )}
        </div>
      </ScrollArea>
    </div>
  );
  const threadPane = openThread ? (
    <ConversationThread
      thread={openThread}
      onConversationOpened={adoptConversationId}
      onBack={singlePane ? backToList : undefined}
      backButtonRef={attachThreadBack}
    />
  ) : (
    <div className="grid h-full place-items-center text-sm text-muted-foreground">
      {t("conversations.noSelection")}
    </div>
  );
  const threadCovers = singlePane && threadOnTop && openThread != null;

  return (
    <div ref={setInboxRoot} className="flex h-full min-h-0 flex-col overflow-hidden">
      {/* 单栏时列表和线程都保持挂载,藏起来的那个用 visibility 而不是
          display:列表留住布局和滚动位置,线程留住它已经跑完的那次同步。 */}
      {singlePane ? (
        <div className="relative min-h-0 flex-1">
          <div ref={listRef} className={cn("absolute inset-0", threadCovers && "invisible")}>{listPane}</div>
          <div className={cn("absolute inset-0", !threadCovers && "invisible")}>{threadPane}</div>
        </div>
      ) : (
        <ResizablePanelGroup orientation="horizontal" className="min-h-0 flex-1">
          <ResizablePanel defaultSize="32%" minSize="30%" maxSize="45%" className="min-w-0">
            {listPane}
          </ResizablePanel>
          <ResizableHandle withHandle />
          <ResizablePanel minSize="30%" className="min-w-0">
            {threadPane}
          </ResizablePanel>
        </ResizablePanelGroup>
      )}
      <NewConversationDialog
        open={composeOpen}
        onOpenChange={setComposeOpen}
        onStart={openDraft}
        onIdentifyTelegram={(record, channel, operatorId, accountLabel) =>
          setTelegramTarget({ record, channel, operatorId, accountLabel })
        }
        restoreFocusRef={composeButtonRef}
      />
      <TelegramTargetDialog
        open={telegramTarget != null}
        record={telegramTarget?.record ?? null}
        channel={telegramTarget?.channel ?? null}
        accounts={accounts.data ?? []}
        operatorId={telegramTarget?.operatorId ?? null}
        accountsLoading={accounts.isLoading}
        accountsError={(accounts.error as Error | null) ?? null}
        onRetryAccounts={() => void accounts.refetch()}
        onOperatorChange={(value) =>
          setTelegramTarget((current) => (current ? { ...current, operatorId: value } : current))
        }
        onOpenChange={(next) => {
          if (!next) setTelegramTarget(null);
        }}
        onStartChat={(record, channel) => {
          const context = telegramTarget;
          setTelegramTarget(null);
          if (context) openDraft(record, channel, context.operatorId, context.accountLabel);
        }}
        restoreFocusRef={composeButtonRef}
      />
    </div>
  );
}
