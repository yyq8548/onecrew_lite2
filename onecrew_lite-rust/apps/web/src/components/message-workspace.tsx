import { OutreachActionReview } from "@/components/outreach-action-review";
import { prepareOutreach, type CreatorOutreachRecord, type OutreachRecordIds, type OutreachReviewState } from "@/lib/outreach-review";
import type { OwnerReviewSnapshot } from "@/lib/owner-review";
import { useEffect, useMemo, useRef, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import {
  ArrowDown,
  Check,
  ChevronsUpDown,
  ExternalLink,
  Loader2,
  RefreshCw,
  Send,
  UserCheck,
  UserPlus,
} from "lucide-react";
import { toast } from "sonner";
import { useTranslation } from "react-i18next";

import { CreatorIdentityAvatar, PlatformIcon } from "@/components/platform";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
  Command,
  CommandEmpty,
  CommandGroup,
  CommandInput,
  CommandItem,
  CommandList,
} from "@/components/ui/command";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover";
import { Textarea } from "@/components/ui/textarea";
import {
  contactTargetPolicy,
  contactTargetResolveInput,
  conversationSyncInput,
  creatorRecordAvatarUrl,
  creatorInvoke,
  relationshipContextInput,
  sendDirectMessageInput,
  updateRelationshipInput,
  type ContactTargetResolutionResult,
  type ConversationView,
  type CreatorRecord,
  CREATORS_QUERY_KEY,
  PLATFORM_LABEL,
  type DirectMessagePreparation,
} from "@/lib/creator";
import {
  CREATOR_LIST_ROOT_QUERY_KEY,
  creatorDetailQueryKey,
} from "@/lib/runtime-query";
import { openExternalUrl } from "@/lib/open-external";
import {
  directMessageCapabilityFact,
  followUiState,
  preparationAfterFollow,
  type OutreachIdentitySource,
} from "@/lib/outreach-message";
import {
  telegramAdministratorError,
} from "@/lib/telegram-administrator-error";
import {
  prepareTelegramAdministratorSave,
  readTelegramCommunityAdministrators,
  type TelegramAdministratorBlockedReason,
  type TelegramAdministrator,
} from "@/lib/telegram-candidate";
import { TelegramSaveReview } from "@/components/telegram-save-review";
import { cn } from "@/lib/utils";
import { MessageScroller } from "@shadcn/react/message-scroller";
import { buildThreadItems } from "@/lib/chat-thread";
import { diagnosticText, presentProviderError } from "@/lib/provider-error";
import { ProviderErrorAlert } from "@/components/provider-error-alert";
import { CONVERSATION_LIST_QUERY_KEY } from "@/lib/conversations";
import type { Account, Platform } from "@/types";
import { isDmPlatform } from "@/lib/dm-platforms";

const administratorEligibilityCopy = {
  bot: "outreach.telegram.eligibility.bot",
  missing_public_handle: "outreach.telegram.eligibility.missing_public_handle",
  missing_profile_url: "outreach.telegram.eligibility.missing_profile_url",
  not_telegram_user: "outreach.telegram.eligibility.not_telegram_user",
} as const satisfies Record<TelegramAdministratorBlockedReason, string>;

function formatOutreachDate(
  epoch: number | null | undefined,
  locale: string,
): string {
  if (!epoch) return "—";
  return new Intl.DateTimeFormat(locale, {
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
    hour12: false,
  }).format(new Date(epoch * 1000));
}

function handleValue(value: string) {
  const trimmed = value.trim().replace(/^@/, "");
  const url = trimmed.match(/^(?:https?:\/\/)?(?:www\.)?(?:x\.com|twitter\.com|instagram\.com|t\.me)\/([^/?#]+)/i);
  return (url?.[1] ?? trimmed).replace(/\/$/, "");
}

/** One DM-capable contact route on a Creator, as the inbox addresses it: the
 * platform, the route it came from, the handle to show, and whether the route
 * still needs a resolved target before a chat can open. */
export type OutreachChannel = {
  platform: Platform;
  contactRouteId: string;
  handle: string;
  requiresIdentification: boolean;
};

export function socialChannels(record: CreatorRecord | null): OutreachChannel[] {
  if (!record) return [];
  const seen = new Set<string>();
  return record.contact_routes.flatMap((route) => {
    const platform = route.locator.channel;
    if (!isDmPlatform(platform)) return [];
    const handle = handleValue(route.locator.display_value);
    const key = `${platform}:${handle.toLocaleLowerCase()}`;
    if (!handle || seen.has(key)) return [];
    seen.add(key);
    const linkedTelegramUser = platform === "telegram" && route.observations.some((observation) => {
      const platformAccountId = observation.observation.platform_account_id;
      return platformAccountId && record.platform_accounts.some((account) => (
        account.id === platformAccountId && account.platform === "telegram" && account.target_kind === "user"
      ));
    });
    return [{
      platform,
      contactRouteId: route.route.id,
      handle,
      requiresIdentification: platform === "telegram" && !linkedTelegramUser,
    }];
  });
}

function IdentitySelect({ platform, accounts, value, loading, error, recentLookupLoading = false, recentLookupError = null, selectionSource = null, recentContactAt = null, onRetry, onChange }: {
  platform: OutreachChannel["platform"];
  accounts: Account[];
  value: string | null;
  loading: boolean;
  error: Error | null;
  recentLookupLoading?: boolean;
  recentLookupError?: unknown;
  selectionSource?: OutreachIdentitySource;
  recentContactAt?: number | null;
  onRetry: () => void;
  onChange: (value: string) => void;
}) {
  const { i18n, t } = useTranslation("inbox");
  const [open, setOpen] = useState(false);
  const compatible = accounts.filter((item) => item.platform === platform);
  const selected = compatible.find((item) => item.id === value) ?? null;
  const emptyLabel = loading
    ? t("outreach.identity.loadingAccounts")
    : error
      ? t("outreach.identity.accountsLoadFailed")
      : compatible.length === 0
        ? t("outreach.identity.noPlatformAccounts", {
            platform: PLATFORM_LABEL[platform],
          })
        : recentLookupLoading
          ? t("outreach.identity.recentLoading")
          : recentLookupError
            ? t("outreach.identity.recentLoadFailed")
            : t("outreach.identity.chooseExisting");
  return (
    <Popover open={open} onOpenChange={setOpen}>
      <PopoverTrigger asChild>
        <Button variant="outline" role="combobox" disabled={loading || (!error && compatible.length === 0)} className="h-9 min-w-0 max-w-[240px] shrink justify-between bg-background px-2.5 font-normal">
          {selected ? <span className="flex min-w-0 items-center gap-2"><PlatformIcon platform={platform} className="size-4" /><span className="truncate">@{selected.username}</span><Badge variant="outline" className="shrink-0 px-1.5 text-[9px]">{selectionSource === "recent" ? t("outreach.identity.lastContact") : t("outreach.identity.selected")}</Badge></span> : <span className={cn("truncate text-muted-foreground", Boolean(error || recentLookupError) && "text-destructive")}>{emptyLabel}</span>}
          <ChevronsUpDown className="ml-2 size-3.5 shrink-0 opacity-50" />
        </Button>
      </PopoverTrigger>
      <PopoverContent align="end" className="w-[300px] p-0">
        <Command>
          <CommandInput placeholder={t("outreach.identity.searchPlaceholder")} />
          <CommandList>
            {error ? (
              <div className="space-y-2 p-3 text-sm">
                <p className="text-destructive">{t("outreach.identity.loadFailed")}</p>
                <p className="line-clamp-2 text-xs text-muted-foreground">{error.message}</p>
                <Button variant="outline" size="sm" className="w-full" onClick={onRetry}><RefreshCw className="size-3.5" />{t("common.retry")}</Button>
              </div>
            ) : <CommandEmpty>{compatible.length ? t("outreach.identity.noMatch") : t("outreach.identity.noConnected", { platform: PLATFORM_LABEL[platform] })}</CommandEmpty>}
            {selected && <div className="border-b px-3 py-2 text-[11px] text-muted-foreground">{selectionSource === "recent" ? t("outreach.identity.recentAt", { date: formatOutreachDate(recentContactAt, i18n.resolvedLanguage ?? i18n.language) }) : t("outreach.identity.selectedForSession")}</div>}
            <CommandGroup heading={t("outreach.identity.chooseHeading")}>
              {compatible.map((account) => (
                <CommandItem key={account.id} value={`${account.username} ${PLATFORM_LABEL[account.platform]} ${account.profile_name}`} onSelect={() => { onChange(account.id); setOpen(false); }}>
                  <PlatformIcon platform={account.platform} className="size-4" />
                  <span className="min-w-0 flex-1"><span className="block truncate">@{account.username}</span><span className="block truncate text-[11px] text-muted-foreground">{PLATFORM_LABEL[account.platform]} · {account.profile_name || t("outreach.identity.localIdentity")}</span></span>
                  <Check className={cn("size-4", selected?.id === account.id ? "opacity-100" : "opacity-0")} />
                </CommandItem>
              ))}
            </CommandGroup>
          </CommandList>
        </Command>
      </PopoverContent>
    </Popover>
  );
}

export function TelegramTargetDialog({ open, record, channel, accounts, operatorId, accountsLoading, accountsError, onRetryAccounts, onOperatorChange, onOpenChange, onStartChat, restoreFocusRef }: {
  open: boolean;
  record: CreatorRecord | null;
  channel: OutreachChannel | null;
  accounts: Account[];
  operatorId: string | null;
  accountsLoading: boolean;
  accountsError: Error | null;
  onRetryAccounts: () => void;
  onOperatorChange: (value: string) => void;
  onOpenChange: (open: boolean) => void;
  onStartChat: (record: CreatorRecord, channel: OutreachChannel) => void;
  restoreFocusRef: React.RefObject<HTMLElement | null>;
}) {
  const { t } = useTranslation("inbox");
  const queryClient = useQueryClient();
  const administratorErrorMessage = (error: unknown) => {
    const presentation = telegramAdministratorError(error);
    return presentation.kind === "operator_admin_required"
      ? t("outreach.telegram.operatorAdminRequired")
      : presentation.message;
  };
  const resolve = useMutation({
    mutationFn: () => creatorInvoke<ContactTargetResolutionResult>("onecrew_contact_target_resolve", { input: contactTargetResolveInput(record!.creator.id, channel!.contactRouteId, operatorId!, false) }),
    onSuccess: async (result) => {
      queryClient.setQueryData(["creator-outreach-target", record!.creator.id, channel!.contactRouteId, operatorId], result);
      const policy = contactTargetPolicy(result);
      if (!policy.canDirectMessage) return;
      const refreshed = await creatorInvoke<CreatorRecord>("onecrew_creator_get", { input: { creator_id: record!.creator.id } });
      queryClient.setQueryData<CreatorRecord>(creatorDetailQueryKey(refreshed.creator.id), refreshed);
      await queryClient.invalidateQueries({ queryKey: CREATOR_LIST_ROOT_QUERY_KEY });
      onOpenChange(false);
      onStartChat(refreshed, { ...channel!, requiresIdentification: false });
    },
    onError: (error) => toast.error(t("outreach.telegram.identifyFailed"), { description: String(error) }),
  });
  const policy = resolve.data ? contactTargetPolicy(resolve.data) : null;
  const [review, setReview] = useState<OwnerReviewSnapshot | null>(null);
  const activeContext = `${open}:${record?.creator.id}:${channel?.contactRouteId}:${operatorId}`;
  const contextRef = useRef(activeContext);
  contextRef.current = activeContext;
  const extract = useMutation({
    mutationFn: (_context: string) => readTelegramCommunityAdministrators({ source_creator_id: record!.creator.id, source_contact_route_id: channel!.contactRouteId, operator_account_ref_id: operatorId! }),
    onError: (error) => toast.error(t("outreach.telegram.extractFailed"), { description: administratorErrorMessage(error) }),
  });
  const administrators = extract.variables === activeContext ? extract.data : undefined;
  const prepareAdministrator = useMutation({
    mutationFn: ({ administrator }: { administrator: TelegramAdministrator; context: string }) => prepareTelegramAdministratorSave(
      administrator, administrators!.community,
      { source_creator_id: record!.creator.id, source_contact_route_id: channel!.contactRouteId, operator_account_ref_id: operatorId! },
    ),
    onSuccess: (snapshot, variables) => { if (variables.context === contextRef.current) setReview(snapshot); },
    onError: (error) => toast.error(t("outreach.telegram.administratorSaveFailed"), { description: String(error) }),
  });
  const administratorSaved = (administrator: CreatorRecord) => {
      queryClient.setQueryData<CreatorRecord>(creatorDetailQueryKey(administrator.creator.id), administrator);
      void queryClient.invalidateQueries({ queryKey: CREATOR_LIST_ROOT_QUERY_KEY });
      setReview(null);
      const telegram = socialChannels(administrator).find((item) => item.platform === "telegram" && !item.requiresIdentification);
      if (!telegram) {
        toast.success(t("outreach.telegram.administratorSaved"), { description: t("outreach.telegram.administratorSavedNoDm") });
        return;
      }
      onOpenChange(false);
      onStartChat(administrator, telegram);
  };

  useEffect(() => {
    resolve.reset();
    extract.reset();
    prepareAdministrator.reset();
    setReview(null);
  }, [open, operatorId, channel?.contactRouteId]);

  return <><Dialog open={open} onOpenChange={onOpenChange}>
    <DialogContent
      className="flex max-h-[85vh] flex-col overflow-hidden sm:max-w-xl"
      onCloseAutoFocus={(event) => {
        const target = restoreFocusRef.current;
        if (!target?.isConnected) return;
        event.preventDefault();
        target.focus({ preventScroll: true });
      }}
    >
      <DialogHeader><DialogTitle>{t("outreach.telegram.title")}</DialogTitle><DialogDescription>{t("outreach.telegram.description")}</DialogDescription></DialogHeader>
      <div className="min-h-0 flex-1 space-y-4 overflow-y-auto pr-1">
        <div className="rounded-lg border p-3"><div className="text-xs text-muted-foreground">{t("outreach.telegram.originalRoute")}</div><div className="mt-1 font-medium">@{channel?.handle}</div></div>
        <div className="flex items-center gap-2"><IdentitySelect platform="telegram" accounts={accounts} value={operatorId} loading={accountsLoading} error={accountsError} onRetry={onRetryAccounts} onChange={onOperatorChange} /><Button className="flex-1" disabled={!operatorId || resolve.isPending} onClick={() => resolve.mutate()}>{resolve.isPending && <Loader2 className="animate-spin" />}{t("outreach.telegram.identifyAction")}</Button></div>
        {policy && <div className="space-y-3 rounded-lg border bg-muted/20 p-3"><div className="text-sm font-medium">{t("outreach.telegram.result", { result: policy.targetKind ?? t(`targetStatus.${policy.status}`) })}</div><p className="text-xs leading-5 text-muted-foreground">{policy.canDirectMessage ? t("outreach.telegram.userResolved") : policy.canExtractAdministrators ? t("outreach.telegram.communityResolved") : policy.targetKind === "bot" ? t("outreach.telegram.botUnsupported") : policy.targetKind === "group" ? t("outreach.telegram.groupUnsupported") : policy.reason ?? t("outreach.telegram.browserOnly")}</p><div className="flex gap-2">{policy.canOpenBrowser && policy.browserUrl && <Button size="sm" variant="outline" onClick={() => void openExternalUrl(policy.browserUrl!)}><ExternalLink />{t("common.browserOpen")}</Button>}{policy.canExtractAdministrators && <Button size="sm" disabled={extract.isPending} onClick={() => extract.mutate(activeContext)}>{extract.isPending && <Loader2 className="animate-spin" />}{t("outreach.telegram.extractAdministrators")}</Button>}</div></div>}
        {extract.error && <Alert variant="destructive"><AlertDescription>{administratorErrorMessage(extract.error)}</AlertDescription></Alert>}
        {administrators && <div className="space-y-2">
          <div className="text-sm font-medium">{t("outreach.telegram.selectAdministrator")}</div>
          <p className="text-xs text-muted-foreground">{t("outreach.telegram.administratorPage", { count: administrators.page_limit })}</p>
          {administrators.administrators.map((administrator) => {
            const bot = administrator.user.is_bot;
            const eligibility = administrator.eligibility;
            const unavailable = eligibility.status !== "ready";
            const eligibilityLabel = eligibility.status === "ready"
              ? t("outreach.telegram.reviewAction")
              : t(administratorEligibilityCopy[eligibility.reason]);
            const pending = prepareAdministrator.isPending && prepareAdministrator.variables?.administrator.user.id === administrator.user.id;
            return <div key={administrator.user.id} className="flex items-center gap-3 rounded-lg border p-3">
              <div className="min-w-0 flex-1">
                <div className="flex min-w-0 flex-wrap items-center gap-1.5">
                  <div className="truncate text-sm font-medium">{[administrator.user.first_name, administrator.user.last_name].filter(Boolean).join(" ") || administrator.user.username || administrator.user.id}</div>
                  <Badge variant="secondary">{t(administrators.community.kind === "channel" ? "outreach.telegram.badges.channel" : "outreach.telegram.badges.supergroup")}</Badge>
                  <Badge variant="secondary">{t(administrator.role === "owner" ? "outreach.telegram.badges.owner" : "outreach.telegram.badges.administrator")}</Badge>
                  {bot && <Badge variant="destructive">{t("outreach.telegram.badges.bot")}</Badge>}
                </div>
                <div className="truncate text-xs text-muted-foreground">{administrator.user.username ? `@${administrator.user.username}` : administrator.user.id}</div>
              </div>
              <Button size="sm" disabled={unavailable || prepareAdministrator.isPending} onClick={() => prepareAdministrator.mutate({ administrator, context: activeContext })}>
                {pending ? <Loader2 className="animate-spin" /> : <UserPlus />}
                {eligibilityLabel}
              </Button>
            </div>;
          })}
          {!administrators.administrators.length && <div className="rounded-lg border p-4 text-center text-sm text-muted-foreground">{t("outreach.telegram.noCandidates")}</div>}
        </div>}
      </div>
    </DialogContent>
  </Dialog><TelegramSaveReview snapshot={review} onSnapshot={setReview} onClose={() => setReview(null)} onSaved={administratorSaved} /></>;
}

export type MessageIdentityStatus = {
  kind: "selected" | "accounts_loading" | "accounts_error" | "no_accounts" | "recent_loading" | "recent_error" | "unselected";
  message: string;
};

export function MessageWorkspace({ record, channel, operatorId, identityStatus, conversationId, onConversationOpened, narrow = false }: {
  record: CreatorRecord;
  channel: OutreachChannel;
  operatorId: string | null;
  identityStatus: MessageIdentityStatus;
  /** 已有会话行的 ID(收件箱的行自带);从 Creator 侧新开的线程还没有。 */
  conversationId: string | null;
  /** 首次同步或首次发送把会话行建出来时报给宿主,让收件箱那侧也认得这条
   * 线程。工作区不因此重挂载:挂载键仍由宿主决定。 */
  onConversationOpened?: (conversationId: string) => void;
  /** 宿主正在单栏显示:顶栏的带文字按钮收成图标。宿主知道自己的布局,所以
   * 这里不再量一次宽度。 */
  narrow?: boolean;
}) {
  const { i18n, t } = useTranslation("inbox");
  const queryClient = useQueryClient();
  const [body, setBody] = useState("");
  const [warningOpen, setWarningOpen] = useState(false);
  const [outreachReview, setOutreachReview] = useState<OutreachReviewState | null>(null);
  const [outreachReviewOpen, setOutreachReviewOpen] = useState(false);
  const currentContext = `${record.creator.id}:${channel.contactRouteId}:${operatorId}`;
  const currentContextRef = useRef(currentContext);
  currentContextRef.current = currentContext;
  // Preparation is read-only. After confirmation is attempted, the frozen
  // review retains this exact input/key until a typed recorded/refused outcome.
  const followKeyRef = useRef<string | null>(null);
  const sendKeyRef = useRef<string | null>(null);
  const target = useQuery({
    queryKey: ["creator-outreach-target", record.creator.id, channel.contactRouteId, operatorId],
    queryFn: () => creatorInvoke<ContactTargetResolutionResult>("onecrew_contact_target_resolve", { input: contactTargetResolveInput(record.creator.id, channel.contactRouteId, operatorId!, false) }),
    enabled: Boolean(operatorId), retry: false, staleTime: Infinity,
  });
  const targetPolicy = target.data ? contactTargetPolicy(target.data) : null;
  const resolvedTargetId = target.data?.status === "resolved" ? target.data.resolved?.target.id ?? null : null;
  const preparationQueryKey = ["creator-message-preparation", record.creator.id, channel.contactRouteId, resolvedTargetId, operatorId] as const;
  const preparation = useQuery({
    queryKey: preparationQueryKey,
    queryFn: () => creatorInvoke<DirectMessagePreparation>("onecrew_outreach_message_prepare", { input: relationshipContextInput(record.creator.id, channel.contactRouteId, resolvedTargetId!, operatorId!) }),
    enabled: Boolean(operatorId && resolvedTargetId && targetPolicy?.canDirectMessage), retry: false,
    // 权限探测同样要走 provider:同一新鲜度窗口,避免每次挂载重跑。
    staleTime: 60_000,
  });
  // 本地会话读按会话 ID 定位。收件箱的行自带 ID;从 Creator 侧新开的线程
  // 要等首次同步或首次发送把会话行建出来才知道。组件按线程 key 重挂载,
  // 所以这份状态不会跨线程串。
  const [openConversationId, setOpenConversationId] = useState<string | null>(conversationId);
  const adoptConversation = (id: string) => {
    setOpenConversationId(id);
    onConversationOpened?.(id);
  };
  const conversation = useQuery({
    queryKey: ["creator-conversation", openConversationId],
    // 纯本地读。同步是显式动作(见 syncThread),绝不能放在 query 里:
    // runtime 同步后会发 change 事件失效这个 key,若 queryFn 自己就是
    // sync,就成了「同步→失效→再同步」的死循环,一路把账号打到限流。
    queryFn: () =>
      creatorInvoke<ConversationView | null>("onecrew_conversation_get", {
        input: { conversation_id: openConversationId! },
      }),
    enabled: Boolean(openConversationId), retry: false, refetchOnWindowFocus: false,
  });
  // 向平台拉取线程:打开线程时跑一次,刷新按钮再跑;完成后让本地读重取。
  const syncThread = useMutation({
    mutationFn: () =>
      creatorInvoke<ConversationView | null>("onecrew_conversation_sync", {
        input: conversationSyncInput(
          relationshipContextInput(record.creator.id, channel.contactRouteId, resolvedTargetId!, operatorId!),
          null,
          100,
        ),
      }),
    onSuccess: (view) => { if (view) adoptConversation(view.conversation.id); },
    onSettled: () => queryClient.invalidateQueries({ queryKey: ["creator-conversation"] }),
  });
  const canSyncThread = Boolean(operatorId && resolvedTargetId && targetPolicy?.canDirectMessage);
  const syncThreadRef = useRef(syncThread);
  syncThreadRef.current = syncThread;
  useEffect(() => {
    // 每次打开线程(组件按线程 key 重挂载)自动同步一次。
    if (canSyncThread) syncThreadRef.current.mutate();
  }, [canSyncThread]);
  const threadItems = useMemo(
    () => buildThreadItems(conversation.data?.messages ?? []),
    [conversation.data?.messages],
  );
  // 本地会话读是纯本地读,不等 provider 探测:收件箱的行自带会话 ID,它的
  // 历史必须立刻出来。
  const threadLoading = conversation.isFetching || syncThread.isPending;
  // 空态要多等一步:还没有会话 ID 而目标仍在解析时,首次同步马上会把会话建
  // 出来,此刻不能先宣称线程是空的 —— 解析中的提示已经在上面说了。
  const threadMayStillFill = threadLoading || (!openConversationId && target.isFetching);
  const peerAvatarUrl = useMemo(() => creatorRecordAvatarUrl(record), [record]);
  // Each read keeps its own failure scope. Similar diagnostics do not prove
  // that preparation and thread sync share one failure occurrence.
  const follow = useMutation({
    mutationFn: async () => {
      const intent = { kind: "relationship" as const, input: updateRelationshipInput(
        relationshipContextInput(record.creator.id, channel.contactRouteId, resolvedTargetId!, operatorId!),
        "follow", (followKeyRef.current ??= crypto.randomUUID()),
      ) };
      return { intent, snapshot: await prepareOutreach(intent) };
    },
    onSuccess: (result) => {
      setOutreachReview({ ...result, confirmationAttempted: false });
      setOutreachReviewOpen(true);
    },
    onError: (error) => toast.error(t("outreach.message.followFailed"), { description: diagnosticText(presentProviderError(error)) }),
  });
  const send = useMutation({
    mutationFn: async (allowUnfollowed: boolean) => {
      const intent = { kind: "direct_message" as const, input: sendDirectMessageInput(
        relationshipContextInput(record.creator.id, channel.contactRouteId, resolvedTargetId!, operatorId!),
        body.trim(), allowUnfollowed, (sendKeyRef.current ??= crypto.randomUUID()),
      ) };
      return { intent, snapshot: await prepareOutreach(intent) };
    },
    onSuccess: (result) => {
      setOutreachReview({ ...result, confirmationAttempted: false });
      setOutreachReviewOpen(true);
    },
    onError: (error) => toast.error(t("outreach.message.sendFailed"), { description: diagnosticText(presentProviderError(error)) }),
  });

  function finishReview() {
    if (!outreachReview) return;
    if (outreachReview.intent.kind === "relationship") followKeyRef.current = null;
    else sendKeyRef.current = null;
    setOutreachReview(null); setOutreachReviewOpen(false);
  }

  async function adoptRecordedConversation(conversationId: string) {
    if (!outreachReview || outreachReview.intent.kind !== "direct_message") return;
    const input = outreachReview.intent.input;
    if (`${input.creator_id}:${input.contact_route_id}:${input.operator_account_ref_id}` === currentContextRef.current) adoptConversation(conversationId);
    setBody("");
    await Promise.all([
      queryClient.invalidateQueries({ queryKey: ["creator-conversation", conversationId], exact: true }),
      queryClient.invalidateQueries({ queryKey: CREATORS_QUERY_KEY }),
      queryClient.invalidateQueries({ queryKey: CONVERSATION_LIST_QUERY_KEY }),
    ]);
  }

  async function outreachRecorded(recorded: CreatorOutreachRecord) {
    if (!outreachReview) return;
    if (recorded.kind === "relationship" && outreachReview.intent.kind === "relationship") {
      const input = outreachReview.intent.input;
      const key = ["creator-message-preparation", input.creator_id, input.contact_route_id, input.resolved_target_id, input.operator_account_ref_id];
      await queryClient.cancelQueries({ queryKey: key, exact: true });
      queryClient.setQueryData<DirectMessagePreparation>(key, current => preparationAfterFollow(current, recorded.record));
      toast.success(t("outreach.message.followed", { handle: channel.handle }));
      await queryClient.invalidateQueries({ queryKey: CREATORS_QUERY_KEY });
    } else if (recorded.kind === "direct_message" && outreachReview.intent.kind === "direct_message") {
      await adoptRecordedConversation(recorded.record.conversation.id);
      toast.success(t("outreach.message.sent", { handle: channel.handle }));
    } else throw new Error(t("outreach.review.unverified"));
    finishReview();
  }

  async function outreachRecordedStatus(recorded: OutreachRecordIds) {
    if (!outreachReview) return;
    if (recorded.kind === "direct_message" && outreachReview.intent.kind === "direct_message") {
      await adoptRecordedConversation(recorded.conversation_id);
      toast.success(t("outreach.message.sent", { handle: channel.handle }));
    } else if (recorded.kind === "relationship" && outreachReview.intent.kind === "relationship") {
      const input = outreachReview.intent.input;
      await queryClient.invalidateQueries({ queryKey: ["creator-message-preparation", input.creator_id, input.contact_route_id, input.resolved_target_id, input.operator_account_ref_id], exact: true });
      await queryClient.invalidateQueries({ queryKey: CREATORS_QUERY_KEY });
      toast.success(t("outreach.message.followed", { handle: channel.handle }));
    } else throw new Error(t("outreach.review.unverified"));
    finishReview();
  }

  const supportsFollow = targetPolicy?.canFollow === true;
  const followState = followUiState({
    preparation: preparation.data,
    preparationFetching: preparation.isFetching,
    preparationError: preparation.error,
    mutationPending: follow.isPending,
    mutationError: follow.error,
  });
  const followDisabled = Boolean(outreachReview) || send.isPending || !resolvedTargetId
    || preparation.isFetching
    || followState === "checking"
    || followState === "updating"
    || followState === "yes"
    || followState === "error";
  const followLabel = {
    checking: t("outreach.message.followStates.checking"),
    yes: t("outreach.message.followStates.yes"),
    no: t("outreach.message.followStates.no"),
    unknown: t("outreach.message.followStates.unknown"),
    updating: t("outreach.message.followStates.updating"),
    error: t("outreach.message.followStates.error"),
  }[followState];
  const followIcon = followState === "checking" || followState === "updating"
    ? <Loader2 className="animate-spin" />
    : followState === "yes" ? <UserCheck /> : <UserPlus />;
  // 单栏时这两个按钮只留图标,文字进 aria-label 和 title。它们带着文字时各自
  // 占住自己的最小宽度,把标题那一列压到一个字宽,标题就会一个字一行地折。
  const browserUrl = targetPolicy?.canOpenBrowser ? targetPolicy.browserUrl : null;
  const headerActions = <>
    {browserUrl && (narrow
      ? <Button size="icon" variant="outline" className="size-8 shrink-0" aria-label={t("common.browserOpen")} title={t("common.browserOpen")} onClick={() => void openExternalUrl(browserUrl)}><ExternalLink /></Button>
      : <Button size="sm" variant="outline" onClick={() => void openExternalUrl(browserUrl)}><ExternalLink />{t("common.browserOpen")}</Button>)}
    {supportsFollow && (narrow
      ? <Button size="icon" className="relative z-10 size-8 shrink-0" variant={followState === "yes" ? "secondary" : "outline"} disabled={followDisabled} aria-label={followLabel} title={followLabel} onClick={() => follow.mutate()}>{followIcon}</Button>
      : <Button size="sm" className="relative z-10 min-w-[96px]" variant={followState === "yes" ? "secondary" : "outline"} disabled={followDisabled} onClick={() => follow.mutate()}>{followIcon}{followLabel}</Button>)}
  </>;
  const requestSend = () => {
    if (outreachReview || follow.isPending || send.isPending) { setOutreachReviewOpen(true); return; }
    if (preparation.data?.unfollowed_confirmation_required) setWarningOpen(true);
    else send.mutate(false);
  };
  const capabilityFact = preparation.data
    ? directMessageCapabilityFact(
        preparation.data.capability,
        PLATFORM_LABEL[channel.platform],
        t,
      )
    : null;
  const capabilityUnavailable = preparation.data?.capability.state === "unavailable";
  const capabilityUnknown = preparation.data?.capability.state === "unknown";
  const targetUnavailable = targetPolicy?.status === "conflict"
    ? t("outreach.message.targetConflict", {
        detail: targetPolicy.conflict
          ? t("outreach.message.conflictDetail", {
              kind: targetPolicy.conflict.kind,
              id: targetPolicy.conflict.id,
            })
          : "",
      })
    : targetPolicy && targetPolicy.status !== "resolved"
      ? t("outreach.message.cannotDirectMessage", {
          status: t(`targetStatus.${targetPolicy.status}`),
        })
      : targetPolicy?.status === "resolved" && !targetPolicy.canDirectMessage
        ? targetPolicy.canExtractAdministrators
          ? t("outreach.message.communityTarget")
          : targetPolicy.targetKind === "bot"
            ? t("outreach.message.botTarget")
            : targetPolicy.targetKind === "group"
              ? t("outreach.message.groupTarget")
              : targetPolicy.targetKind === "business"
                ? t("outreach.message.businessTarget")
                : t("outreach.message.unsupportedTarget")
        : null;
  const messageReady = identityStatus.kind === "selected"
    && Boolean(operatorId && resolvedTargetId && targetPolicy?.canDirectMessage && preparation.data)
    && !target.isFetching
    && !preparation.isFetching
    && !preparation.error
    && !capabilityUnavailable;
  const composerPlaceholder = identityStatus.kind !== "selected"
    ? identityStatus.message
    : target.isFetching || (!target.data && !target.error)
      ? t("outreach.message.resolvingPlaceholder")
      : target.error
        ? presentProviderError(target.error).summary
        : targetUnavailable
          ? targetUnavailable
          : preparation.isFetching || (!preparation.data && !preparation.error)
            ? t("outreach.message.checkingPermissionPlaceholder")
            : preparation.error
              ? presentProviderError(preparation.error).summary
              : capabilityUnavailable
                ? capabilityFact ?? t("outreach.message.cannotSend")
                : t("outreach.message.composePlaceholder", { handle: channel.handle });
  const headerStatus = identityStatus.kind !== "selected"
    ? identityStatus.message
    : target.isFetching || (!target.data && !target.error)
      ? t("outreach.message.resolvingStatus")
      : target.error
        ? presentProviderError(target.error).summary
        : preparation.isFetching || (targetPolicy?.canDirectMessage && !preparation.data && !preparation.error)
          ? t("outreach.message.checkingPermissionStatus")
          : capabilityUnavailable
            ? t("outreach.message.unavailableStatus")
            : capabilityUnknown
              ? t("outreach.message.revalidateStatus")
              : targetPolicy
                ? t(`targetStatus.${targetPolicy.status}`)
                : t("outreach.message.awaitingIdentity");
  const refreshWorkspace = async () => {
    follow.reset();
    await Promise.all([preparation.refetch(), syncThread.mutateAsync().catch(() => null)]);
  };

  useEffect(() => {
    if (conversation.dataUpdatedAt > 0) {
      void queryClient.invalidateQueries({ queryKey: CREATORS_QUERY_KEY });
    }
  }, [conversation.dataUpdatedAt, queryClient]);

  return <>
    {/* 贴底跟随交给 shadcn 的 MessageScroller。换聊天对象就连 key 一起换,
        让它重新挂载并落到最新一条。 */}
    <MessageScroller.Provider key={`${record.creator.id}:${channel.contactRouteId}`} autoScroll>
    <div className="flex h-full min-h-0 flex-col">
      <div className="flex shrink-0 items-center gap-2 border-b bg-muted/20 px-4 py-3"><PlatformIcon platform={channel.platform} className="size-5 shrink-0" /><div className="min-w-0 flex-1"><div className="truncate text-sm font-medium">{t("outreach.message.chatTitle", { platform: PLATFORM_LABEL[channel.platform] })}</div><div className="truncate text-xs text-muted-foreground">{narrow ? headerStatus : <>@{channel.handle} · {headerStatus}</>}</div></div>{headerActions}<Button variant="ghost" size="icon" className="shrink-0" disabled={!targetPolicy?.canDirectMessage || preparation.isFetching || syncThread.isPending} onClick={() => void refreshWorkspace()} aria-label={t("outreach.message.refreshAria")}><RefreshCw className={cn((preparation.isFetching || syncThread.isPending) && "animate-spin")} /></Button></div>
      <MessageScroller.Root className="relative flex min-h-0 flex-1 flex-col">
      {/* 不用 ScrollArea:Radix 会把子节点包进一层无高度的 wrapper,
          min-h-full / justify-end / flex-1 在里面全部失效。 */}
      <MessageScroller.Viewport className="min-h-0 flex-1 overflow-y-auto bg-muted/10">
        <MessageScroller.Content className="mx-auto flex min-h-full w-full min-w-0 max-w-2xl flex-col justify-end gap-1 p-4">
          {threadItems.map((item) => {
            if (item.kind === "date") {
              return <div key={item.id} className="my-3 flex items-center gap-3"><span className="h-px flex-1 bg-border" /><span className="text-[11px] text-muted-foreground">{formatOutreachDate(item.at, i18n.resolvedLanguage ?? i18n.language)}</span><span className="h-px flex-1 bg-border" /></div>;
            }
            const { message, outgoing, startsGroup, endsGroup } = item;
            return (
              <MessageScroller.Item key={item.id} messageId={item.id} className={cn("flex items-end gap-2", startsGroup && "mt-2", outgoing ? "justify-end" : "justify-start")}>
                {!outgoing && (endsGroup
                  ? <CreatorIdentityAvatar platform={channel.platform} name={record.creator.display_name} src={peerAvatarUrl} className="size-7" />
                  : <span className="size-7 shrink-0" aria-hidden />)}
                <div className={cn("max-w-[78%] rounded-2xl px-3.5 py-2 text-sm", outgoing ? "bg-primary text-primary-foreground" : "bg-muted", startsGroup && endsGroup ? "" : outgoing ? (startsGroup ? "rounded-br-md" : endsGroup ? "rounded-tr-md" : "rounded-r-md") : (startsGroup ? "rounded-bl-md" : endsGroup ? "rounded-tl-md" : "rounded-l-md"))}>
                  <div className="whitespace-pre-wrap break-words">{message.body}</div>
                  {endsGroup && <div className={cn("mt-1 text-[10px]", outgoing ? "text-primary-foreground/70" : "text-muted-foreground")}>{formatOutreachDate(message.sent_at, i18n.resolvedLanguage ?? i18n.language)}</div>}
                </div>
              </MessageScroller.Item>
            );
          })}
          {identityStatus.kind !== "selected" && <Alert><AlertTitle>{t("outreach.message.operatorIdentity")}</AlertTitle><AlertDescription>{identityStatus.message}</AlertDescription></Alert>}
          {identityStatus.kind === "selected" && target.isLoading && <div className="grid flex-1 place-items-center gap-2 text-sm text-muted-foreground"><Loader2 className="animate-spin" />{t("outreach.message.resolvingStatus")}</div>}
          {target.error && <ProviderErrorAlert error={target.error} scope={t("outreach.message.targetResolveFailed")} onRetry={() => void target.refetch()} />}
          {targetUnavailable && <Alert><AlertTitle>{t("outreach.message.cannotStart")}</AlertTitle><AlertDescription>{targetUnavailable}</AlertDescription></Alert>}
          {preparation.error && <ProviderErrorAlert error={preparation.error} scope={t("outreach.message.permissionCheckFailed")} onRetry={() => void preparation.refetch()} />}
          {capabilityFact && <Alert variant={capabilityUnavailable ? "destructive" : "warning"}><AlertTitle>{capabilityUnavailable ? t("outreach.message.cannotSendTitle") : t("outreach.message.permissionPendingTitle")}</AlertTitle><AlertDescription className="text-current/90">{capabilityFact}</AlertDescription></Alert>}
          {threadLoading && !threadItems.length && <div className="grid flex-1 place-items-center"><Loader2 className="animate-spin text-muted-foreground" /></div>}
          {conversation.error && <ProviderErrorAlert error={conversation.error} scope={t("outreach.message.localHistoryLoadFailed")} onRetry={() => void conversation.refetch()} />}
          {syncThread.error != null && <ProviderErrorAlert error={syncThread.error} scope={t("outreach.message.syncFailed")} onRetry={() => void syncThread.mutate()} />}
          {!threadMayStillFill && !conversation.error && !threadItems.length && <div className="grid flex-1 place-items-center text-sm text-muted-foreground">{t("outreach.message.noHistory", { creator: record.creator.display_name })}</div>}
        </MessageScroller.Content>
      </MessageScroller.Viewport>
      <MessageScroller.Button
        direction="end"
        render={
          <Button
            size="icon"
            variant="secondary"
            aria-label={t("outreach.message.latestAria")}
            className="absolute bottom-3 left-1/2 z-10 size-8 -translate-x-1/2 rounded-full border shadow-md data-[active=false]:hidden"
          >
            <ArrowDown className="size-4" />
          </Button>
        }
      />
      </MessageScroller.Root>
      <div className="shrink-0 border-t bg-background p-3">{outreachReview && <Button variant="outline" className="mb-2" onClick={() => setOutreachReviewOpen(true)}>{t("outreach.review.openPending")}</Button>}<div className="flex items-end gap-2"><Textarea value={body} onChange={(event) => { if (outreachReview) return; sendKeyRef.current = null; setBody(event.target.value); }} placeholder={composerPlaceholder} disabled={!messageReady || send.isPending || follow.isPending || Boolean(outreachReview)} className="min-h-20 resize-none" /><Button size="icon" className="size-10" aria-label={t("outreach.message.sendAria")} disabled={!messageReady || !body.trim() || send.isPending || follow.isPending || Boolean(outreachReview)} onClick={requestSend}>{send.isPending ? <Loader2 className="animate-spin" /> : <Send />}</Button></div><div className="mt-2 text-[11px] text-muted-foreground">{capabilityUnknown ? t("outreach.message.capabilityUnknownHelp") : t("outreach.message.threadHelp")}</div></div>
    </div>
    </MessageScroller.Provider>
    <OutreachActionReview open={outreachReviewOpen} review={outreachReview} onOpenChange={setOutreachReviewOpen}
      onSnapshot={snapshot => setOutreachReview(current => current ? { ...current, snapshot } : null)}
      onConfirmationAttempted={() => setOutreachReview(current => current ? { ...current, confirmationAttempted: true } : null)}
      onRecorded={outreachRecorded} onRecordedStatus={outreachRecordedStatus}
      onRejected={guidance => { toast.error(t("outreach.review.rejected"), { description: guidance }); finishReview(); }}
      onCanceled={() => { if (!outreachReview?.confirmationAttempted) finishReview(); else setOutreachReviewOpen(false); }} />
    <Dialog open={warningOpen} onOpenChange={setWarningOpen}><DialogContent><DialogHeader><DialogTitle>{t("outreach.message.warningTitle")}</DialogTitle><DialogDescription>{t("outreach.message.warningDescription", { handle: channel.handle })}</DialogDescription></DialogHeader><DialogFooter><Button variant="outline" onClick={() => setWarningOpen(false)}>{t("outreach.message.backToCheck")}</Button><Button onClick={() => { setWarningOpen(false); send.mutate(true); }}>{t("outreach.message.sendAnyway")}</Button></DialogFooter></DialogContent></Dialog>
  </>;
}
