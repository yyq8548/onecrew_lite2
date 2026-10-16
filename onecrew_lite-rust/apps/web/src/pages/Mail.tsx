import { forwardRef, useCallback, useEffect, useImperativeHandle, useMemo, useRef, useState } from "react";
import { flushSync } from "react-dom";
import { isRuntimeAvailable } from "@/lib/local-runtime";
import { useInfiniteQuery, useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { ArrowLeft, Folders, Info, Loader2, MailOpen, RefreshCw, Search } from "lucide-react";
import { useSize } from "radix-ui/internal";
import { usePanelRef } from "react-resizable-panels";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";

import {
  MailComposerSheet,
  type EmailComposerLaunch,
  type MailComposerSheetHandle,
} from "@/components/mail/composer-sheet";
import {
  ComposeButton,
  MailFolderNav,
  emailFolderCount,
  emailFolderLabel,
} from "@/components/mail/folder-nav";
import { MailItemList } from "@/components/mail/mail-item-list";
import { MailboxCombobox } from "@/components/mail/mailbox-combobox";
import { GmailBulk } from "@/components/mail/gmail-bulk";
import { GmailConnection } from "@/components/mail/gmail-connection";
import { resolveEmailReplyTarget } from "@/components/mail/reply-target";
import { MailTemplateLibrary } from "@/components/mail/template-library";
import {
  EmailMessageDisplay,
  MailboxUnavailable,
  mailboxCanReceive,
  mailboxCanSend,
} from "@/components/mail/mail-workspace";
import {
  MAILBOX_QUERY_KEY,
  emailDraftListKey,
  emailFolderListKey,
  emailThreadKey,
  emailThreadListKey,
  emailThreadListRootKey,
  getEmailThread,
  invalidateMailboxMailQueries,
  listEmailDrafts,
  listEmailFolders,
  listEmailThreads,
  listMailboxes,
  markEmailThreadRead,
  runMailboxSync,
  updateEmailThreadDisposition,
} from "@/components/mail/queries";
import { RuntimeUnavailable } from "@/components/runtime-unavailable";
import { Button } from "@/components/ui/button";
import { Empty, EmptyDescription, EmptyHeader, EmptyMedia, EmptyTitle } from "@/components/ui/empty";
import { Input } from "@/components/ui/input";
import { ResizableHandle, ResizablePanel, ResizablePanelGroup } from "@/components/ui/resizable";
import { ScrollArea } from "@/components/ui/scroll-area";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Separator } from "@/components/ui/separator";
import { Sheet, SheetContent, SheetDescription, SheetHeader, SheetTitle, SheetTrigger } from "@/components/ui/sheet";
import { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from "@/components/ui/tooltip";
import {
  emailContactRoutes,
  type EmailDraftState,
  type EmailFolder,
  type EmailMessageView,
  type EmailThreadDispositionAction,
  type Page,
  type EmailThreadSort,
  type EmailThreadSummary,
  type EmailThreadView,
  type MailboxDetail,
} from "@/lib/creator";
import { useCanvasPortal } from "@/lib/canvas-portal";
import { creatorDetailQueryOptions } from "@/lib/runtime-query";
import { cn } from "@/lib/utils";

// Below this Mail width the folders, list and reader cannot sit side by
// side: at 800px the list (32%) keeps about 84px for its title and count
// beside the 132px sort control; a 528px host panel left it 18px.
const MAIL_SPLIT_MIN_WIDTH = 800;

function threadRecipient(summary: EmailThreadSummary | undefined) {
  const message = summary?.latest_message;
  if (!message) return null;
  return message.direction === "inbound" ? message.reply_to[0] ?? message.from : message.to[0] ?? null;
}

export type MailNavigationGuardHandle = {
  requestLeave: (continueNavigation: () => void) => void;
};

export const Mail = forwardRef<MailNavigationGuardHandle, { onManageMailboxes?: () => void }>(function Mail({
  onManageMailboxes,
}, ref) {
  const { t } = useTranslation("mail");
  const runtimeAvailable = isRuntimeAvailable();
  const queryClient = useQueryClient();
  const composerRef = useRef<MailComposerSheetHandle>(null);
  const canvas = useCanvasPortal();
  const [mailboxId, setMailboxId] = useState<string | null>(null);
  const [folder, setFolder] = useState<EmailFolder>("sent");
  const [threadSort, setThreadSort] = useState<EmailThreadSort>("newest");
  const [isCollapsed, setIsCollapsed] = useState(false);
  const navPanelRef = usePanelRef();
  // Mail's own width picks the layout, not the window's: the shell sidebar
  // and host panels (the Codex side panel) take space a viewport breakpoint
  // cannot see. Until the first measurement the split layout renders.
  const [mailRoot, setMailRoot] = useState<HTMLDivElement | null>(null);
  const mailWidth = useSize(mailRoot)?.width;
  const singlePane = mailWidth !== undefined && mailWidth < MAIL_SPLIT_MIN_WIDTH;
  const [foldersOpen, setFoldersOpen] = useState(false);
  const listRef = useRef<HTMLDivElement>(null);
  const readerBackRef = useRef<HTMLButtonElement | null>(null);
  const readerBackHadFocus = useRef(false);
  // One pane: the loading, error and loaded reader states each render their
  // own back button, so focus follows from the one leaving to the one arriving.
  const attachReaderBack = useCallback((node: HTMLButtonElement) => {
    readerBackRef.current = node;
    if (readerBackHadFocus.current) node.focus();
    readerBackHadFocus.current = false;
    return () => {
      readerBackHadFocus.current = document.activeElement === node;
      readerBackRef.current = null;
    };
  }, []);
  const [search, setSearch] = useState("");
  const [selectedThreadId, setSelectedThreadId] = useState<string | null>(null);
  const [composerLaunch, setComposerLaunch] = useState<EmailComposerLaunch | null>(null);

  const mailboxes = useQuery({
    queryKey: MAILBOX_QUERY_KEY,
    queryFn: () => listMailboxes<MailboxDetail[]>(),
    enabled: runtimeAvailable,
    retry: false,
    staleTime: 30_000,
  });
  const mailboxOptions = useMemo(() => (mailboxes.data ?? []).filter(m => m.provider === "gmail"), [mailboxes.data]);
  const selectedMailbox = mailboxOptions.find((item) => item.mailbox_ref_id === mailboxId) ?? null;
  const receivingUnavailableDescription = mailboxCanSend(selectedMailbox)
    ? t("page.sync.receivingUnavailable")
    : t("page.connectMailbox");
  const folders = useQuery({
    queryKey: emailFolderListKey(mailboxId),
    queryFn: () => listEmailFolders({ mailbox_ref_id: mailboxId }),
    enabled: runtimeAvailable && Boolean(mailboxId),
    retry: false,
    refetchOnWindowFocus: false,
  });
  const threads = useInfiniteQuery({
    queryKey: emailThreadListKey(mailboxId, folder, threadSort),
    queryFn: ({ pageParam }) => listEmailThreads<Page<EmailThreadSummary>>({
      mailbox_ref_id: mailboxId,
      folder,
      sort: threadSort,
      limit: 100,
      cursor: pageParam,
    }),
    initialPageParam: null as string | null,
    getNextPageParam: (last) => last.next_cursor ?? null,
    enabled: runtimeAvailable && Boolean(mailboxId) && folder !== "drafts",
    retry: false,
    refetchOnWindowFocus: false,
  });
  const threadItems = useMemo(
    () => threads.data?.pages.flatMap((page) => page.items) ?? [],
    [threads.data],
  );
  const draftState: EmailDraftState | null = folder === "drafts"
    ? "draft"
    : folder === "deleted"
      ? "trashed"
      : null;
  const drafts = useInfiniteQuery({
    queryKey: emailDraftListKey(mailboxId, draftState),
    queryFn: ({ pageParam }) => listEmailDrafts({
      mailbox_ref_id: mailboxId, state: draftState, limit: 100, cursor: pageParam,
    }),
    initialPageParam: null as string | null,
    getNextPageParam: (last) => last.next_cursor ?? null,
    enabled: runtimeAvailable && Boolean(mailboxId) && Boolean(draftState),
    retry: false,
    refetchOnWindowFocus: false,
  });
  const draftItems = useMemo(
    () => drafts.data?.pages.flatMap((page) => page.items) ?? [],
    [drafts.data],
  );
  const selectedSummary = threadItems.find(({ thread }) => thread.id === selectedThreadId);
  const selectedThread = useQuery({
    queryKey: emailThreadKey(selectedThreadId),
    queryFn: () => getEmailThread<EmailThreadView>({ thread_id: selectedThreadId! }),
    enabled: runtimeAvailable && Boolean(selectedThreadId),
    retry: false,
  });
  // Thread messages arrive newest first, so the latest message is the head.
  const lastMessage: EmailMessageView | null = selectedThread.data?.messages[0] ?? null;
  const selectedRecipient = threadRecipient(selectedSummary);
  // Detail-on-demand: the thread already names its creator, so fetch that one
  // record instead of scanning the whole creator list for a reply route.
  const replyCreator = useQuery(creatorDetailQueryOptions(
    selectedSummary?.thread.creator_id ?? null,
    runtimeAvailable,
  ));
  const replyRoute = emailContactRoutes(replyCreator.data).find((route) => route.route.id === selectedSummary?.thread.contact_route_id);
  const creatorReplyTarget = selectedSummary?.thread.creator_id && replyRoute && selectedRecipient
    ? {
        creatorId: selectedSummary.thread.creator_id,
        contactRouteId: replyRoute.route.id,
        to: selectedRecipient,
      }
    : null;
  const selectedRuntimeThread = selectedThread.data?.thread;
  const replyTarget = resolveEmailReplyTarget({
    creatorTarget: creatorReplyTarget,
    displayedMessage: lastMessage,
    threadMessages: selectedThread.data?.messages ?? [],
    allowManualThreadReply: Boolean(
      selectedMailbox?.available
      && selectedMailbox.capabilities.includes("reply")
      && mailboxCanSend(selectedMailbox)
      && selectedRuntimeThread
      && !selectedRuntimeThread.creator_id
      && !selectedRuntimeThread.contact_route_id
    ),
  });

  const replySync = useMutation({
    mutationFn: (id: string) => runMailboxSync(id),
    onSuccess: async (result, id) => {
      await invalidateMailboxMailQueries(queryClient, id);
      await Promise.all(result.changed_thread_ids.map(threadId =>
        queryClient.invalidateQueries({ queryKey: emailThreadKey(threadId) })));
    },
    onError: () => toast.error(t("page.replySyncFailed")),
  });
  const syncReplies = replySync.mutate;
  useEffect(() => {
    if (!runtimeAvailable || !mailboxId) return;
    const refresh = () => {
      if (document.visibilityState === "visible") syncReplies(mailboxId);
    };
    refresh();
    const timer = window.setInterval(refresh, 60_000);
    return () => window.clearInterval(timer);
  }, [mailboxId, runtimeAvailable, syncReplies]);

  const invalidateMailbox = async () => {
    await invalidateMailboxMailQueries(queryClient, mailboxId, selectedThreadId);
  };
  // Opening a thread is what marks it read. The runtime leaves updated_at alone
  // so this never reorders the list, and the loaded pages are patched in place
  // instead of refetched, so the row the reader just clicked does not jump.
  const markRead = useMutation({
    mutationFn: (threadId: string) => markEmailThreadRead({ thread_id: threadId }),
    onSuccess: (result) => {
      queryClient.setQueryData(emailThreadKey(result.thread.id), result);
      queryClient.setQueriesData<{ pages: Page<EmailThreadSummary>[]; pageParams: unknown[] }>(
        { queryKey: emailThreadListRootKey(mailboxId) },
        (current) => (current
          ? {
              ...current,
              pages: current.pages.map((page) => ({
                ...page,
                items: page.items.map((item) => (item.thread.id === result.thread.id
                  ? { ...item, thread: { ...item.thread, unread_count: 0 } }
                  : item)),
              })),
            }
          : current),
      );
    },
    // Read state is local and self-healing: the next thread read reports the
    // truth, so a failure here must not interrupt reading with an error toast.
    onError: () => undefined,
  });
  const updateDisposition = useMutation({
    mutationFn: ({ threadId, action }: { threadId: string; action: EmailThreadDispositionAction }) => (
      updateEmailThreadDisposition({ thread_id: threadId, action })
    ),
    onSuccess: async (result) => {
      queryClient.setQueryData(emailThreadKey(result.thread.id), result);
      setSelectedThreadId(null);
      toast.success(t("page.toasts.dispositionSuccess", {
        folder: emailFolderLabel(result.thread.folder, t),
      }));
      await invalidateMailbox();
    },
    onError: (error) => toast.error(t("page.toasts.dispositionFailed"), { description: String(error) }),
  });

  const applyMailboxChange = (value: string | null) => {
    setMailboxId(value);
    setFolder("sent");
    setSelectedThreadId(null);
    setComposerLaunch(null);
  };
  const requestMailboxChange = (value: string | null) => {
    if (value === mailboxId) return;
    const change = () => applyMailboxChange(value);
    if (!composerLaunch) {
      change();
      return;
    }
    composerRef.current?.requestClose(change);
  };

  useImperativeHandle(ref, () => ({
    requestLeave: (continueNavigation) => {
      if (!composerLaunch) {
        continueNavigation();
        return;
      }
      composerRef.current?.requestClose(continueNavigation);
    },
  }), [composerLaunch]);

  useEffect(() => {
    if (mailboxId && mailboxOptions.some((mailbox) => mailbox.mailbox_ref_id === mailboxId)) return;
    requestMailboxChange(
      mailboxOptions.find((mailbox) => mailbox.available)?.mailbox_ref_id
        ?? mailboxOptions[0]?.mailbox_ref_id
        ?? null,
    );
  }, [mailboxId, mailboxOptions]);
  useEffect(() => {
    if (!selectedThreadId || selectedThread.isLoading) return;
    if (!selectedThread.data && selectedThread.error) setSelectedThreadId(null);
  }, [selectedThread.data, selectedThread.error, selectedThread.isLoading, selectedThreadId]);
  const openedThread = selectedThread.data?.thread;
  useEffect(() => {
    if (!openedThread || openedThread.unread_count === 0 || markRead.isPending) return;
    markRead.mutate(openedThread.id);
  }, [openedThread?.id, openedThread?.unread_count]);
  // Leaving one-pane mode unmounts the folder Sheet without onOpenChange;
  // clear its state so narrowing again does not reopen it unasked.
  useEffect(() => {
    if (!singlePane) setFoldersOpen(false);
  }, [singlePane]);

  const changeMailbox = (value: string) => {
    requestMailboxChange(value);
  };
  const changeFolder = (value: EmailFolder) => {
    setFoldersOpen(false);
    setFolder(value);
    setSelectedThreadId(null);
    void invalidateMailboxMailQueries(queryClient, mailboxId);
  };
  const openThread = (threadId: string) => {
    if (!singlePane) {
      setSelectedThreadId(threadId);
      return;
    }
    // The reader covers the list, so focus moves to the reader's back button.
    flushSync(() => setSelectedThreadId(threadId));
    readerBackRef.current?.focus();
  };
  const closeReader = () => {
    const openedId = selectedThreadId;
    flushSync(() => setSelectedThreadId(null));
    readerBackHadFocus.current = false;
    // The list kept its scroll position; return focus to the row that was open.
    Array.from(listRef.current?.querySelectorAll<HTMLElement>("[data-thread-id]") ?? [])
      .find((row) => row.dataset.threadId === openedId)
      ?.focus();
  };
  const beginNewEmail = (initialTemplateId?: string) => {
    setFoldersOpen(false);
    setComposerLaunch({
      key: crypto.randomUUID(),
      kind: "new",
      initialTemplateId,
    });
  };
  const beginReply = () => {
    if (!replyTarget) return;
    setComposerLaunch({
      key: crypto.randomUUID(),
      kind: "reply",
      to: replyTarget.to,
      creatorId: replyTarget.creatorId,
      contactRouteId: replyTarget.contactRouteId,
      message: replyTarget.message,
    });
  };

  if (!runtimeAvailable) return <RuntimeUnavailable className="h-full border-0" />;
  if (mailboxes.isLoading) {
    return <div className="grid h-full place-items-center"><Loader2 className="animate-spin text-muted-foreground" /></div>;
  }

  const folderTotal = mailboxId ? emailFolderCount(folders.data, folder) : 0;
  const listError = folders.error ?? threads.error ?? drafts.error;

  const navColumn = (collapsed: boolean) => (
    <>
      <div className={cn(
        "flex shrink-0 gap-2 border-b px-2",
        collapsed ? "flex-col items-center py-2" : "h-14 items-center",
      )}>
        <Tooltip>
          <TooltipTrigger asChild>
            <div
              className={cn("rounded-md focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring", !collapsed && "min-w-0 flex-1")}
              tabIndex={!selectedMailbox ? 0 : undefined}
              aria-label={!selectedMailbox ? t("mailbox.noneAvailable") : undefined}
            >
              <MailboxCombobox
                mailboxes={mailboxOptions}
                value={mailboxId}
                displayValue="label"
                contentAlign="start"
                collapsed={collapsed}
                className={cn(!collapsed && "min-w-0 max-w-none w-full")}
                onValueChange={changeMailbox}
              />
            </div>
          </TooltipTrigger>
          {collapsed && (
            <TooltipContent side="right">
              {selectedMailbox
                ? selectedMailbox.label && selectedMailbox.label !== selectedMailbox.email_address
                  ? `${selectedMailbox.label} · ${selectedMailbox.email_address}`
                  : selectedMailbox.email_address
                : t("mailbox.noneAvailable")}
            </TooltipContent>
          )}
        </Tooltip>
        <Button variant="outline" size="icon" disabled={!selectedMailbox || replySync.isPending}
          aria-label={t("page.sync.action")} onClick={() => mailboxId && syncReplies(mailboxId)}>
          <RefreshCw className={replySync.isPending ? "animate-spin" : undefined} />
        </Button>
      </div>
      <ScrollArea className="min-h-0 flex-1">
        <MailFolderNav
          sentRecordsOnly
          collapsed={collapsed}
          folder={folder}
          folders={folders.data}
          onFolderChange={changeFolder}
        />
      </ScrollArea>
      <div className={cn(
        "mt-auto space-y-2 p-2",
        collapsed && "flex flex-col items-center gap-2 space-y-0",
      )}>
        {!collapsed && <p className="p-3 text-xs leading-5 text-muted-foreground">{t("page.sentRecordsOnly")}</p>}
        {!mailboxCanReceive(selectedMailbox) && (collapsed ? (
          <Tooltip>
            <TooltipTrigger asChild>
              <Button variant="ghost" size="icon" className="size-9 text-muted-foreground" aria-label={receivingUnavailableDescription}>
                <Info className="size-4" />
              </Button>
            </TooltipTrigger>
            <TooltipContent side="right" className="max-w-72">{receivingUnavailableDescription}</TooltipContent>
          </Tooltip>
        ) : (
          <p className="rounded-lg bg-muted/50 p-3 text-xs leading-5 text-muted-foreground">
            {receivingUnavailableDescription}
          </p>
        ))}
        <MailTemplateLibrary
          collapsed={collapsed}
          canCompose={mailboxCanSend(selectedMailbox)}
          onUseTemplate={(templateId) => beginNewEmail(templateId)}
        />
        <ComposeButton collapsed={collapsed} disabled={!mailboxCanSend(selectedMailbox)} onClick={() => beginNewEmail()} />
      </div>
    </>
  );
  const listPane = (
    <div className="flex h-full min-h-0 flex-col">
      <div className="flex h-14 shrink-0 items-center gap-2 border-b px-4">
        {singlePane && (
          <SheetTrigger asChild>
            <Button variant="ghost" size="icon" className="-ml-2 size-8 shrink-0" aria-label={t("folders.navigationLabel")}>
              <Folders />
            </Button>
          </SheetTrigger>
        )}
        <div className="min-w-0 flex-1">
          <div className="truncate font-semibold">{emailFolderLabel(folder, t)}</div>
          <div className="truncate text-xs text-muted-foreground">
            {folderTotal === null ? t("page.folderCountLoading") : t("page.folderCount", { count: folderTotal })}
          </div>
        </div>
        {folder !== "drafts" && (
          <Select
            value={threadSort}
            onValueChange={(value) => setThreadSort(value as EmailThreadSort)}
          >
            <SelectTrigger size="sm" className="w-[132px] shrink-0" aria-label={t("page.sortLabel")}>
              <SelectValue />
            </SelectTrigger>
            <SelectContent align="end" viewportClassName="h-auto">
              <SelectItem value="newest">{t("page.sortNewest")}</SelectItem>
              <SelectItem value="oldest">{t("page.sortOldest")}</SelectItem>
            </SelectContent>
          </Select>
        )}
      </div>
      <div className="shrink-0 p-4 pb-3">
        <div className="relative">
          <Search className="pointer-events-none absolute left-3 top-1/2 size-4 -translate-y-1/2 text-muted-foreground" />
          <Input
            value={search}
            onChange={(event) => setSearch(event.target.value)}
            placeholder={t("page.searchPlaceholder")}
            className="pl-9"
          />
        </div>
      </div>
      {listError ? (
        <div className="p-4 text-sm text-destructive">{t("page.errors.folder", { error: String(listError) })}</div>
      ) : threads.isLoading || drafts.isLoading ? (
        <div className="grid min-h-0 flex-1 place-items-center"><Loader2 className="animate-spin text-muted-foreground" /></div>
      ) : (
        <div className="min-h-0 flex-1">
          <MailItemList
            threads={folder === "drafts" ? [] : threadItems}
            drafts={draftState ? draftItems : []}
            selectedThreadId={selectedThreadId}
            query={search}
            emptyDescription={folder === "drafts" ? t("page.emptyDraftsDescription") : t("page.emptyFolderDescription")}
            onSelectThread={openThread}
            onSelectDraft={(draftId) => setComposerLaunch({ key: crypto.randomUUID(), kind: "draft", draftId })}
          />
          {(folder !== "drafts" && threads.hasNextPage) || (draftState && drafts.hasNextPage) ? (
            <div className="border-t p-2">
              <Button
                variant="ghost"
                size="sm"
                className="w-full"
                disabled={threads.isFetchingNextPage || drafts.isFetchingNextPage}
                onClick={() => {
                  if (folder !== "drafts" && threads.hasNextPage) void threads.fetchNextPage();
                  if (draftState && drafts.hasNextPage) void drafts.fetchNextPage();
                }}
              >
                {threads.isFetchingNextPage || drafts.isFetchingNextPage ? t("common.loading") : t("common.loadMore")}
              </Button>
            </div>
          ) : null}
        </div>
      )}
    </div>
  );
  // One pane has no list beside the reader, so every reader state keeps a way back.
  const readerBackRow = singlePane ? (
    <div className="flex h-14 shrink-0 items-center border-b px-2">
      <Button ref={attachReaderBack} variant="ghost" size="icon" onClick={closeReader} aria-label={t("message.backToList")}>
        <ArrowLeft />
      </Button>
    </div>
  ) : null;
  const readerPane = (
    <div className="flex h-full min-h-0 flex-col">
      {selectedThread.isLoading ? (
        <>
          {readerBackRow}
          <div className="grid min-h-0 flex-1 place-items-center"><Loader2 className="animate-spin text-muted-foreground" /></div>
        </>
      ) : selectedThread.error ? (
        <>
          {readerBackRow}
          <div className="p-4 text-sm text-destructive">{t("page.errors.thread", { error: String(selectedThread.error) })}</div>
        </>
      ) : lastMessage ? (
        <EmailMessageDisplay
          messages={selectedThread.data?.messages ?? []}
          threadFolder={selectedThread.data?.thread.folder}
          viewFolder={folder}
          dispositionPending={updateDisposition.isPending}
          onDispositionAction={selectedThreadId
            ? (action) => updateDisposition.mutate({ threadId: selectedThreadId, action })
            : undefined}
          onReply={replyTarget ? beginReply : undefined}
          onBack={singlePane ? closeReader : undefined}
          backButtonRef={attachReaderBack}
        />
      ) : (
        <Empty className="h-full">
          <EmptyHeader>
            <EmptyMedia variant="icon"><MailOpen /></EmptyMedia>
            <EmptyTitle>{t("page.readerEmptyTitle")}</EmptyTitle>
            <EmptyDescription>{t("page.readerEmptyDescription")}</EmptyDescription>
          </EmptyHeader>
          {mailboxCanSend(selectedMailbox) && <Button size="sm" onClick={() => beginNewEmail()}>{t("common.compose")}</Button>}
        </Empty>
      )}
    </div>
  );

  return (
    <TooltipProvider delayDuration={0}>
      <div ref={setMailRoot} className="flex h-full min-h-0 flex-col overflow-hidden border-t-0 bg-card">
        <GmailConnection />
        <GmailBulk mailboxes={mailboxes.data ?? []} mailboxError={mailboxes.error} mailboxesLoading={mailboxes.isFetching} refreshMailboxes={() => mailboxes.refetch({ throwOnError: true })} />
        <MailboxUnavailable mailbox={selectedMailbox} />
        {mailboxes.error && (
          <div className="border-b border-destructive/30 bg-destructive/5 px-4 py-3 text-xs text-destructive">
            {t("page.errors.mailboxes", { error: String(mailboxes.error) })}
          </div>
        )}
        {!selectedMailbox && onManageMailboxes && (
          <div className="border-b bg-muted/20 px-4 py-3">
            <Button variant="outline" size="sm" onClick={onManageMailboxes}>{t("page.connectMailbox")}</Button>
          </div>
        )}

        {/* Like the shadcn mail example, a narrow Mail is one pane (list, then
            reader) and the folder column opens as a Sheet from its trigger.
            Both panes stay mounted; the hidden one uses visibility, not
            display, so the list keeps its layout and scroll position. */}
        {singlePane ? (
          <Sheet open={foldersOpen} onOpenChange={setFoldersOpen}>
            <div className="relative min-h-0 flex-1">
              <div ref={listRef} className={cn("absolute inset-0", selectedThreadId && "invisible")}>{listPane}</div>
              <div className={cn("absolute inset-0", !selectedThreadId && "invisible")}>{readerPane}</div>
            </div>
            <SheetContent side="left" container={canvas} showCloseButton={false} className="w-72 gap-0 p-0">
              <SheetHeader className="sr-only">
                <SheetTitle>{t("folders.navigationLabel")}</SheetTitle>
                <SheetDescription>{t("folders.sheetDescription")}</SheetDescription>
              </SheetHeader>
              {navColumn(false)}
            </SheetContent>
          </Sheet>
        ) : (
          <ResizablePanelGroup orientation="horizontal" className="min-h-0 flex-1">
            <ResizablePanel
              defaultSize="220px"
              collapsedSize="56px"
              collapsible
              minSize="180px"
              maxSize="320px"
              groupResizeBehavior="preserve-pixel-size"
              panelRef={navPanelRef}
              onResize={() => {
                // The measured width is not the layout size, so ask the panel.
                if (navPanelRef.current) setIsCollapsed(navPanelRef.current.isCollapsed());
              }}
              className="flex min-w-0 flex-col"
            >
              {navColumn(isCollapsed)}
            </ResizablePanel>
            <ResizableHandle withHandle />

            <ResizablePanel defaultSize="32%" minSize="30%" maxSize="45%" className="min-w-0">
              {listPane}
            </ResizablePanel>
            <ResizableHandle withHandle />

            <ResizablePanel minSize="30%" className="min-w-0">
              {readerPane}
            </ResizablePanel>
          </ResizablePanelGroup>
        )}

        <MailComposerSheet
          ref={composerRef}
          launch={composerLaunch}
          mailbox={selectedMailbox}
          container={canvas}
          onClose={() => setComposerLaunch(null)}
          onSent={(result) => {
            setFolder("sent");
            setSelectedThreadId(result.thread.id);
          }}
        />
      </div>
    </TooltipProvider>
  );
});
