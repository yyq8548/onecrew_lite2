import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { isRuntimeAvailable } from "@/lib/local-runtime";
import { useInfiniteQuery, useQuery, useQueryClient } from "@tanstack/react-query";
import { type ColumnDef, type FilterFn, type RowSelectionState, type SortingState } from "@tanstack/react-table";
import { Archive, Check, ExternalLink, Folder, Link2, List as ListIcon, Loader2, Mail, MoreHorizontal, Pin, PinOff, RefreshCw, Trash2 } from "lucide-react";
import { toast } from "sonner";
import { useTranslation } from "react-i18next";

import { DataTable, DataTableColumnHeader } from "@/components/data-table";
import { CreatorScoreBadge } from "@/components/creator-score-badge";
import { usePersistedColumnFilters } from "@/components/data-table/filter-state";
import { canFetchCreatorPage, creatorFacetValues, creatorTableCriteria } from "@/lib/creator-list-query";
import { SavedContactRoutes } from "@/components/contact-routes";
import { RuntimeUnavailable } from "@/components/runtime-unavailable";
import { CreatorIdentityAvatar, PlatformIcon } from "@/components/platform";
import { useCanvasPortal } from "@/lib/canvas-portal";
import { revealWhenClipped } from "@/lib/clipped-title";
import {
  hiddenChannels,
  hiddenFollowerCounts,
  hiddenPlatformLabels,
} from "./leads/creator-overflow";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { Input } from "@/components/ui/input";
import { Separator } from "@/components/ui/separator";
import {
  Sheet,
  SheetClose,
  SheetContent,
  SheetDescription,
  SheetFooter,
  SheetHeader,
  SheetTitle,
} from "@/components/ui/sheet";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { Tabs, TabsList, TabsTrigger } from "@/components/ui/tabs";
import {
  creatorAccountViews,
  archiveCreator,
  deleteCreator,
  CREATORS_QUERY_KEY,
  PLATFORM_LABEL,
  formatCount,
  formatDate,
  creatorRecordAvatarUrl,
  primaryCreatorAccount,
  updateCreatorStage,
  type CreatorRecord,
  type CreatorContactRouteRecord,
  type CreatorStage,
  type CreatorSummary,
  type CreatorPlatform,
  type UpdateCreatorStageInput,
  setCreatorPinned,
} from "@/lib/creator";
import {
  CREATOR_GROUPS_QUERY_KEY,
  creatorGroupsInfiniteQueryOptions,
  shouldAutoFetchCreatorGroupPage,
} from "@/lib/creator-groups";
import { creatorDetailQueryOptions, creatorsInfiniteQueryOptions } from "@/lib/runtime-query";
import { openExternalUrl } from "@/lib/open-external";
import { timestampMatchesDateRange } from "@/lib/date-range-filter";
import { cn } from "@/lib/utils";
import { ManualContactRouteDialog } from "@/pages/leads/manual-contact-route-dialog";
import { ManualCreatorDialog } from "@/pages/leads/manual-creator-dialog";
import { CreatorTransferMenu } from "@/pages/leads/creator-transfer-menu";
import { CreatorBulkMoveBar } from "@/pages/leads/creator-bulk-move-bar";
import { CreateCreatorGroupDialog } from "@/pages/leads/creator-group-dialogs";
import { CreatorGroupsView } from "@/pages/leads/creator-groups-view";
import { creatorDetailActionsEnabled } from "@/pages/leads/creator-detail-policy";
import { DeleteContactRouteDialog, EditContactRouteDialog } from "@/pages/leads/contact-route-mutation-dialogs";
import { creatorSourcePresentation } from "@/pages/leads/creator-source";
import { primaryFollowerEntry } from "@/pages/leads/creator-followers";
import { CreatorTagsBadges } from "@/pages/leads/creator-tags-input";
import { CreatorTagsEditor } from "@/pages/leads/creator-tags-editor";
import {
  contactChannelLabel,
  creatorStageLabel,
  type LeadsT,
} from "@/pages/leads/i18n-labels";

const STAGE_TONE: Record<CreatorStage, string> = {
  prospect: "bg-muted text-muted-foreground",
  engaged: "bg-[var(--tone-info-bg)] text-[var(--tone-info-fg)]",
  collaborating: "bg-[var(--tone-success-bg)] text-[var(--tone-success-fg)]",
  archived: "bg-muted text-muted-foreground",
};

export const CREATOR_DETAIL_SHEET_CLASS =
  "flex w-[min(560px,calc(100%-1rem))] max-w-[calc(100%-1rem)] flex-col gap-0 overflow-hidden p-0 sm:max-w-[560px]";

function isArchivedCreator(record: CreatorRecord) {
  return record.creator.archived_at != null || record.relationship.stage === "archived";
}

function isArchivedSummary(summary: CreatorSummary) {
  return summary.archived_at != null || summary.stage === "archived";
}

const inArray: FilterFn<CreatorSummary> = (row, id, value) =>
  (value as string[]).includes(row.getValue(id) as string);

const inDateRange: FilterFn<CreatorSummary> = (row, id, value) =>
  timestampMatchesDateRange(row.getValue(id), value);

function platformFilterIcon(platform: CreatorPlatform) {
  return function PlatformFilterIcon({ className }: { className?: string }) {
    return <PlatformIcon platform={platform} className={className} />;
  };
}

/** The follower count shown in the list: the first-captured platform's count,
 * falling back to any platform that has one. */
/** 计数徽章:内容不是被压窄,是根本没渲染 —— 所以除了 hover 的 tooltip
 *  还得留一份 sr-only,否则读屏那边直接就是「+2」两个字。
 *  TooltipTrigger 一律 asChild 包一个非可聚焦元素:裸用会渲染成真的
 *  <button>,每行凭空多出几个 tab 停靠点,而且 focus 会绕过延迟直接弹开。*/
function OverflowChip({ label, items }: { label: string; items: string[] }) {
  const { t } = useTranslation("leads");
  if (items.length === 0) return <Badge variant="secondary" className="border-transparent">{label}</Badge>;
  const text = items.join(t("common.separators.list"));
  return (
    <Tooltip delayDuration={300}>
      <TooltipTrigger asChild>
        <Badge variant="secondary" className="border-transparent">
          {label}
          <span className="sr-only">{t("common.overflowItems", { items: text })}</span>
        </Badge>
      </TooltipTrigger>
      <TooltipContent side="top">{text}</TooltipContent>
    </Tooltip>
  );
}

function channelIcon(channel: CreatorSummary["channels"][number]) {
  if (channel === "email") return <Mail className="size-3.5" />;
  if (channel === "website" || channel === "link_hub" || channel === "wechat") return <Link2 className="size-3.5" />;
  return <PlatformIcon platform={channel} className="size-3.5" />;
}

function ContactChannelBadge({ channel }: { channel: CreatorSummary["channels"][number] }) {
  const { t } = useTranslation("leads");
  const label = contactChannelLabel(t, channel);
  return (
    <Tooltip delayDuration={300}>
      <TooltipTrigger asChild>
        <Badge
          variant="secondary"
          role="img"
          aria-label={label}
          className="size-6 shrink-0 justify-center border-transparent p-0"
        >
          <span aria-hidden="true">{channelIcon(channel)}</span>
        </Badge>
      </TooltipTrigger>
      <TooltipContent side="top">{label}</TooltipContent>
    </Tooltip>
  );
}

// List rows carry the summary identity facts (first-captured platform, latest
// handle and avatar, distinct platform/channel sets); full records stay in the
// detail sheet.
/** flexRender 把 `cell` 当组件类型渲染,所以列定义每换一次身份,所有单元格
 *  子树都会卸载重挂(头像重新拉取,就是肉眼看到的闪动)。这个数组必须被
 *  memo 住,也因此不能依赖行数据 —— 置顶状态从 row.original.pinned_at 读。 */
function creatorColumns(
  t: LeadsT,
  locale: string,
  onStage: (summary: CreatorSummary, stage: CreatorStage) => void,
  onArchive: (summary: CreatorSummary) => void,
  onDelete: (summary: CreatorSummary) => void,
  onTogglePin: (summary: CreatorSummary) => void,
): ColumnDef<CreatorSummary>[] {
  const stageFilterOptions = (["prospect", "engaged", "collaborating", "archived"] as CreatorStage[])
    .map((value) => ({ value, label: creatorStageLabel(t, value) }));
  const platformFilterOptions = [
    ...(Object.entries(PLATFORM_LABEL) as [CreatorPlatform, string][]).map(([value, label]) => ({
      value: value as string,
      label,
      icon: platformFilterIcon(value),
    })),
    { value: "", label: t("table.manualEntry") },
  ];
  return [
    {
      id: "select",
      size: 44,
      minSize: 44,
      enableSorting: false,
      enableHiding: false,
      header: ({ table }) => {
        const visibleRows = table.getRowModel().rows.filter((row) => row.getCanSelect());
        const selectedCount = visibleRows.filter((row) => row.getIsSelected()).length;
        return (
          <Checkbox
            aria-label={t("table.selection.allVisible")}
            disabled={visibleRows.length === 0}
            checked={
              selectedCount === visibleRows.length && visibleRows.length > 0
                ? true
                : selectedCount > 0
                  ? "indeterminate"
                  : false
            }
            onCheckedChange={(checked) => {
              for (const row of visibleRows) row.toggleSelected(checked === true);
            }}
          />
        );
      },
      cell: ({ row }) => (
        <div
          className="flex items-center"
          onClick={(event) => event.stopPropagation()}
          onPointerDown={(event) => event.stopPropagation()}
          onKeyDown={(event) => event.stopPropagation()}
        >
          <Checkbox
            aria-label={t("table.selection.row", { name: row.original.display_name })}
            checked={row.getIsSelected()}
            onCheckedChange={(checked) => row.toggleSelected(checked === true)}
          />
        </div>
      ),
    },
    {
      id: "creator",
      // 栅格布局下列宽不再自动:creator 吃掉剩余宽度,其余在 size 与
      // minSize 之间收缩,压到 minSize 还不够就横向滚动。
      size: 240,
      minSize: 180,
      meta: { grow: true },
      accessorFn: (summary) => summary.display_name,
      header: ({ column }) => <DataTableColumnHeader column={column} title={t("table.columns.creator")} />,
      enableHiding: false,
      cell: ({ row }) => (
        <div className="flex min-w-0 items-center gap-2.5">
          <CreatorIdentityAvatar
            platform={row.original.primary_platform}
            name={row.original.display_name}
            src={row.original.avatar_url}
          />
          <div className="min-w-0">
            <div className="flex items-center gap-1.5">
              {row.original.pinned_at != null && (
                <Pin className="size-3 shrink-0 fill-current text-muted-foreground" />
              )}
              <span className="truncate font-medium" onPointerEnter={revealWhenClipped}>
                {row.original.display_name}
              </span>
            </div>
            <div
              className="truncate text-xs text-muted-foreground"
              onPointerEnter={revealWhenClipped}
            >
              {row.original.primary_handle
                ? `@${row.original.primary_handle}`
                : row.original.primary_platform
                  ? PLATFORM_LABEL[row.original.primary_platform]
                  : t("table.noPlatformAccount")}
            </div>
          </div>
        </div>
      ),
    },
    {
      id: "platform",
      size: 170,
      minSize: 112,
      accessorFn: (summary) => summary.primary_platform ?? "",
      // 表头写「初次收录」,显示列菜单也必须是同一个名字 ——「平台」在表上
      // 根本不存在。改表头才是大动作:accessorFn/filterFn 都得跟着换语义。
      meta: { label: t("table.columns.firstCaptured") },
      header: ({ column }) => (
        <DataTableColumnHeader column={column} title={t("table.columns.firstCaptured")} filterOptions={platformFilterOptions} />
      ),
      filterFn: inArray,
      enableSorting: false,
      cell: ({ row }) => {
        const summary = row.original;
        if (!summary.primary_platform) {
          return summary.has_manual_origin
            ? <Badge variant="secondary" className="border-transparent">{t("table.manualEntry")}</Badge>
            : <span className="text-muted-foreground">—</span>;
        }
        return (
          <div className="flex min-w-0 items-center gap-1 overflow-hidden">
            {/* Badge 基元是 inline-flex + whitespace-nowrap + shrink-0,truncate
                挂在它自己身上不生效;要放开收缩并把文字装进块级容器。 */}
            <Badge variant="outline" className="min-w-0 shrink gap-1">
              <PlatformIcon platform={summary.primary_platform} className="size-3" />
              <span className="truncate" onPointerEnter={revealWhenClipped}>
                {summary.primary_handle
                  ? `@${summary.primary_handle}`
                  : PLATFORM_LABEL[summary.primary_platform]}
              </span>
            </Badge>
            {summary.platforms.length > 1 && (
              <OverflowChip label={`+${summary.platforms.length - 1}`} items={hiddenPlatformLabels(summary)} />
            )}
          </div>
        );
      },
    },
    {
      id: "contacts",
      size: 184,
      minSize: 116,
      accessorFn: (summary) => summary.contact_route_count,
      header: t("table.columns.contacts"),
      enableSorting: false,
      cell: ({ row }) => row.original.channels.length > 0 ? (
        <div className="flex min-w-0 items-center gap-1 overflow-hidden">
          {row.original.channels.slice(0, 3).map((channel) => (
            <ContactChannelBadge key={channel} channel={channel} />
          ))}
          {row.original.channels.length > 3 && (
            <OverflowChip
              label={`+${row.original.channels.length - 3}`}
              items={hiddenChannels(row.original.channels, 3).map((channel) => contactChannelLabel(t, channel))}
            />
          )}
        </div>
      ) : <span className="text-muted-foreground">—</span>,
    },
    {
      id: "followers",
      size: 118,
      minSize: 96,
      accessorFn: (summary) => primaryFollowerEntry(summary)?.[1] ?? -1,
      meta: { label: t("table.columns.followers") },
      header: ({ column }) => <DataTableColumnHeader column={column} title={t("table.columns.followers")} />,
      cell: ({ row }) => {
        const entry = primaryFollowerEntry(row.original);
        if (!entry) return <span className="text-muted-foreground">—</span>;
        const [platform, count] = entry;
        const extra = Object.keys(row.original.follower_counts ?? {}).length - 1;
        return (
          <div className="flex items-center gap-1.5 whitespace-nowrap tabular-nums">
            <PlatformIcon platform={platform} className="size-3.5 text-muted-foreground" />
            <span className="font-medium">{formatCount(count, locale)}</span>
            {extra > 0 && (
              <OverflowChip
                label={`+${extra}`}
                items={hiddenFollowerCounts(row.original, platform, locale)}
              />
            )}
          </div>
        );
      },
    },
    {
      id: "tags",
      size: 156,
      minSize: 112,
      accessorFn: (summary) => summary.tags,
      meta: { label: t("table.columns.tags") },
      header: t("table.columns.tags"),
      enableSorting: false,
      enableColumnFilter: false,
      cell: ({ row }) => <CreatorTagsBadges tags={row.original.tags} />,
    },
    {
      id: "score",
      size: 96,
      minSize: 88,
      accessorFn: (summary) => summary.score ?? -1,
      meta: { label: t("table.columns.score") },
      header: ({ column }) => <DataTableColumnHeader column={column} title={t("table.columns.score")} />,
      cell: ({ row }) => (
        <CreatorScoreBadge score={row.original.score} assessment={row.original.assessment} />
      ),
    },
    {
      id: "stage",
      size: 108,
      minSize: 88,
      accessorFn: (summary) => summary.stage,
      // 阶段是筛选维度,不是排序维度:枚举顺序排序没有业务含义。
      enableSorting: false,
      meta: { label: t("table.columns.stage") },
      header: ({ column }) => (
        <DataTableColumnHeader column={column} title={t("table.columns.stage")} filterOptions={stageFilterOptions} />
      ),
      filterFn: inArray,
      cell: ({ row }) => (
        <Badge variant="secondary" className={cn("border-transparent", STAGE_TONE[row.original.stage])}>
          {creatorStageLabel(t, row.original.stage)}
        </Badge>
      ),
    },
    {
      id: "lastTouch",
      // formatDate 恒定 16 字符「2026-08-19 14:32」,而非 grow 列的
      // flex-grow 是 0,再宽的窗口也长不过 size —— 这是列宽,不是 hover。
      size: 160,
      minSize: 152,
      accessorFn: (summary) => summary.last_touch_at ?? 0,
      meta: { label: t("table.columns.lastTouch") },
      header: ({ column }) => (
        <DataTableColumnHeader column={column} title={t("table.columns.lastTouch")} dateFilter />
      ),
      filterFn: inDateRange,
      cell: ({ row }) => (
        <span
          className="truncate tabular-nums text-muted-foreground"
          onPointerEnter={revealWhenClipped}
        >
          {formatDate(row.original.last_touch_at)}
        </span>
      ),
    },
    {
      id: "updatedAt",
      // formatDate 恒定 16 字符「2026-08-19 14:32」,而非 grow 列的
      // flex-grow 是 0,再宽的窗口也长不过 size —— 这是列宽,不是 hover。
      size: 160,
      minSize: 152,
      accessorFn: (summary) => summary.updated_at,
      meta: { label: t("table.columns.updatedAt") },
      header: ({ column }) => (
        <DataTableColumnHeader column={column} title={t("table.columns.updatedAt")} dateFilter />
      ),
      filterFn: inDateRange,
      cell: ({ row }) => (
        <span
          className="truncate tabular-nums text-muted-foreground"
          onPointerEnter={revealWhenClipped}
        >
          {formatDate(row.original.updated_at)}
        </span>
      ),
    },
    {
      id: "createdAt",
      // formatDate 恒定 16 字符「2026-08-19 14:32」,而非 grow 列的
      // flex-grow 是 0,再宽的窗口也长不过 size —— 这是列宽,不是 hover。
      size: 136,
      minSize: 128,
      accessorFn: (summary) => summary.created_at,
      meta: { label: t("table.columns.createdAt") },
      header: ({ column }) => <DataTableColumnHeader column={column} title={t("table.columns.createdAt")} />,
      cell: ({ row }) => (
        <span
          className="truncate tabular-nums text-muted-foreground"
          onPointerEnter={revealWhenClipped}
        >
          {formatDate(row.original.created_at)}
        </span>
      ),
    },
    {
      id: "actions",
      size: 48,
      minSize: 48,
      maxSize: 48,
      enableHiding: false,
      cell: ({ row }) => isArchivedSummary(row.original) ? (
        <div className="flex w-full items-center justify-center" onClick={(event) => event.stopPropagation()}>
          <DropdownMenu>
            <DropdownMenuTrigger asChild>
              <Button variant="ghost" size="icon" className="size-8">
                <MoreHorizontal className="size-4" /><span className="sr-only">{t("table.creatorActions")}</span>
              </Button>
            </DropdownMenuTrigger>
            <DropdownMenuContent align="end" className="w-44">
              <DropdownMenuLabel>{t("table.archived")}</DropdownMenuLabel>
              <DropdownMenuItem className="text-destructive focus:text-destructive" onClick={() => onDelete(row.original)}>
                <Trash2 className="size-4" />{t("table.delete")}
              </DropdownMenuItem>
            </DropdownMenuContent>
          </DropdownMenu>
        </div>
      ) : (
        <div className="flex w-full justify-center" onClick={(event) => event.stopPropagation()}>
          <DropdownMenu>
            <DropdownMenuTrigger asChild>
              <Button variant="ghost" size="icon" className="size-8">
                <MoreHorizontal className="size-4" /><span className="sr-only">{t("table.creatorActions")}</span>
              </Button>
            </DropdownMenuTrigger>
            <DropdownMenuContent align="end" className="w-44">
              <DropdownMenuItem onClick={() => onTogglePin(row.original)}>
                {row.original.pinned_at != null
                  ? <><PinOff className="size-4" />{t("table.unpin")}</>
                  : <><Pin className="size-4" />{t("table.pin")}</>}
              </DropdownMenuItem>
              <DropdownMenuSeparator />
              <DropdownMenuLabel>{t("table.globalStage")}</DropdownMenuLabel>
              {(["prospect", "engaged", "collaborating"] as CreatorStage[])
                .map((stage) => (
                  <DropdownMenuItem key={stage} onClick={() => onStage(row.original, stage)}>
                    <Check className={cn("size-4", row.original.stage !== stage && "invisible")} />{creatorStageLabel(t, stage)}
                  </DropdownMenuItem>
                ))}
              <DropdownMenuSeparator />
              <DropdownMenuItem className="text-destructive focus:text-destructive" onClick={() => onArchive(row.original)}>
                <Archive className="size-4" />{t("table.archive")}
              </DropdownMenuItem>
              <DropdownMenuItem className="text-destructive focus:text-destructive" onClick={() => onDelete(row.original)}>
                <Trash2 className="size-4" />{t("table.delete")}
              </DropdownMenuItem>
            </DropdownMenuContent>
          </DropdownMenu>
        </div>
      ),
    },
  ];
}

type CreatorView = "list" | "groups";

function CreatorViewToggle({
  value,
  onChange,
}: {
  value: CreatorView;
  onChange: (value: CreatorView) => void;
}) {
  const { t } = useTranslation("leads");
  return (
    <Tabs
      value={value}
      onValueChange={(next) => onChange(next as CreatorView)}
      className="block"
    >
      <TabsList className="h-8">
        <TabsTrigger value="list" className="h-7 px-2.5">
          <ListIcon className="size-3.5" />{t("view.list")}
        </TabsTrigger>
        <TabsTrigger value="groups" className="h-7 px-2.5">
          <Folder className="size-3.5" />{t("view.groups")}
        </TabsTrigger>
      </TabsList>
    </Tabs>
  );
}

export function Leads() {
  const { i18n, t } = useTranslation("leads");
  const locale = i18n.resolvedLanguage ?? i18n.language;
  const runtimeAvailable = isRuntimeAvailable();
  const queryClient = useQueryClient();
  // Backend search covers identity and saved contact routes, so agents and
  // humans find creators beyond the loaded pages; debounced to avoid a query
  // per keystroke.
  const [view, setView] = useState<CreatorView>("list");
  const [rowSelection, setRowSelection] = useState<RowSelectionState>({});
  const [search, setSearch] = useState("");
  const [debouncedSearch, setDebouncedSearch] = useState("");
  const [columnFilters, setColumnFilters] = usePersistedColumnFilters("leads-creators-unfiltered");
  const [sorting, setSorting] = useState<SortingState>([]);
  useEffect(() => {
    const handle = setTimeout(() => setDebouncedSearch(search), 300);
    return () => clearTimeout(handle);
  }, [search]);
  // The runtime applies every table criterion before limiting a page. Restored
  // filters participate in the initial query, including the archived stage.
  const listOptions = creatorsInfiniteQueryOptions(
    runtimeAvailable && view === "list",
    {
      include_archived: true,
      query: debouncedSearch,
      ...creatorTableCriteria(columnFilters, sorting),
    },
  );
  const query = useInfiniteQuery(listOptions);
  const listResetKey = JSON.stringify(listOptions.queryKey);
  const canLoadMore = canFetchCreatorPage({
    hasNextPage: Boolean(query.hasNextPage),
    isFetching: query.isFetching,
    isPlaceholderData: query.isPlaceholderData,
    isPending: query.isPending,
    searchPending: search !== debouncedSearch,
  });
  const facets = useMemo(
    () => creatorFacetValues(query.isPlaceholderData ? null : query.data?.pages[0]?.facets),
    [query.data, query.isPlaceholderData],
  );
  const selectedCreatorIds = useMemo(
    () => Object.entries(rowSelection)
      .filter(([, selected]) => selected)
      .map(([creatorId]) => creatorId),
    [rowSelection],
  );
  const groupQuery = useInfiniteQuery(creatorGroupsInfiniteQueryOptions(
    runtimeAvailable && (view === "groups" || selectedCreatorIds.length > 0),
    { include_archived: true },
  ));
  useEffect(() => {
    if (!shouldAutoFetchCreatorGroupPage({
      selectedCount: selectedCreatorIds.length,
      hasNextPage: Boolean(groupQuery.hasNextPage),
      isFetchingNextPage: groupQuery.isFetchingNextPage,
      hasError: Boolean(groupQuery.error) || groupQuery.isFetchNextPageError,
    })) return;
    void groupQuery.fetchNextPage();
  }, [
    groupQuery.error,
    groupQuery.fetchNextPage,
    groupQuery.hasNextPage,
    groupQuery.isFetchingNextPage,
    groupQuery.isFetchNextPageError,
    selectedCreatorIds.length,
  ]);
  const groups = useMemo(
    () => groupQuery.data?.pages.flatMap((page) => page.items) ?? [],
    [groupQuery.data],
  );
  const uncategorizedCount = groupQuery.data?.pages[0]?.uncategorized_count ?? 0;
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const creatorDetailReturnFocus = useRef<HTMLElement | null>(null);
  const [pendingArchive, setPendingArchive] = useState<{ id: string; displayName: string } | null>(null);
  const [pendingDelete, setPendingDelete] = useState<{ id: string; displayName: string } | null>(null);
  const [deleting, setDeleting] = useState(false);
  const [archiving, setArchiving] = useState(false);
  const rows = useMemo(
    () => query.data?.pages.flatMap((page) => page.items) ?? [],
    [query.data],
  );
  // 置顶是持久化的 Creator 工作台字段;tanstack row pinning 把置顶行钉在
  // 表头下(凌驾列排序,服从筛选),置顶时间新的在前。
  const pinnedRowIds = useMemo(
    () =>
      rows
        .filter((row) => row.pinned_at != null)
        .sort((a, b) => (b.pinned_at ?? 0) - (a.pinned_at ?? 0))
        .map((row) => row.id),
    [rows],
  );
  const togglePin = useCallback(async (summary: CreatorSummary) => {
    const pinned = summary.pinned_at == null;
    try {
      await setCreatorPinned(summary.id, pinned);
      await queryClient.invalidateQueries({ queryKey: CREATORS_QUERY_KEY });
    } catch (error) {
      toast.error(pinned ? t("toasts.pinFailed") : t("toasts.unpinFailed"), {
        description: String(error),
      });
    }
  }, [queryClient, t]);
  const detailQuery = useQuery(creatorDetailQueryOptions(selectedId, runtimeAvailable));
  const selected = selectedId ? detailQuery.data ?? null : null;

  const updateStage = useCallback(
    async (creatorId: string, currentStage: CreatorStage, stage: CreatorStage) => {
      if (currentStage === stage) return;
      try {
        const input: UpdateCreatorStageInput = { creator_id: creatorId, stage };
        await updateCreatorStage(input);
        await queryClient.invalidateQueries({ queryKey: CREATORS_QUERY_KEY });
        toast.success(t("toasts.stageUpdated", { stage: creatorStageLabel(t, stage) }));
      } catch (error) {
        toast.error(t("toasts.stageUpdateFailed"), { description: String(error) });
      }
    },
    [queryClient, t],
  );

  const removeCreator = async () => {
    if (!pendingDelete) return;
    setDeleting(true);
    try {
      const result = await deleteCreator(pendingDelete.id);
      await queryClient.invalidateQueries({ queryKey: CREATORS_QUERY_KEY });
      if (selectedId === pendingDelete.id) setSelectedId(null);
      setRowSelection((current) => {
        const next = { ...current };
        delete next[pendingDelete.id];
        return next;
      });
      setPendingDelete(null);
      toast.success(t("toasts.creatorDeleted"), {
        description: t("toasts.creatorDeleteSummary", {
          routes: result.removed_contact_routes,
          touchpoints: result.removed_touchpoints,
        }),
      });
    } catch (error) {
      toast.error(t("toasts.creatorDeleteFailed"), { description: String(error) });
    } finally {
      setDeleting(false);
    }
  };

  const archive = async () => {
    if (!pendingArchive) return;
    setArchiving(true);
    try {
      await archiveCreator(pendingArchive.id);
      await queryClient.invalidateQueries({ queryKey: CREATORS_QUERY_KEY });
      if (selectedId === pendingArchive.id) setSelectedId(null);
      setPendingArchive(null);
      toast.success(t("toasts.creatorArchived"));
    } catch (error) {
      toast.error(t("toasts.creatorArchiveFailed"), { description: String(error) });
    } finally {
      setArchiving(false);
    }
  };

  const columns = useMemo(
    () =>
      creatorColumns(
        t,
        locale,
        (summary, stage) => void updateStage(summary.id, summary.stage, stage),
        (summary) => setPendingArchive({ id: summary.id, displayName: summary.display_name }),
        (summary) => setPendingDelete({ id: summary.id, displayName: summary.display_name }),
        (summary) => void togglePin(summary),
      ),
    [locale, t, updateStage, togglePin],
  );

  // 模态的 Portal 目标,见下面 SheetContent/DialogContent 的 container。
  const canvas = useCanvasPortal();
  const openCreator = useCallback((summary: CreatorSummary) => {
    creatorDetailReturnFocus.current =
      document.activeElement instanceof HTMLElement ? document.activeElement : null;
    setSelectedId(summary.id);
  }, []);
  const groupChoicesError = groupQuery.error ? String(groupQuery.error) : null;
  const groupChoicesLoading = !groupChoicesError && (
    groupQuery.isLoading
    || groupQuery.isFetchingNextPage
    || Boolean(groupQuery.hasNextPage)
  );
  const retryGroupPages = () => {
    if (groupQuery.data?.pages.length && groupQuery.hasNextPage) {
      void groupQuery.fetchNextPage();
    } else {
      void groupQuery.refetch();
    }
  };

  // 虚拟表格自己是滚动容器,所以这一页不滚:壳给高度,表格吃满剩余空间。
  const shell = (content: React.ReactNode) => (
    <div className="flex h-full min-h-0 flex-col p-4 md:p-6">{content}</div>
  );
  if (!runtimeAvailable) return shell(<RuntimeUnavailable />);
  if (view === "list" && query.isLoading) {
    return shell(<div className="grid min-h-72 place-items-center" role="status"><Loader2 className="size-5 animate-spin text-muted-foreground" /><span className="sr-only">{t("common.actions.loading")}</span></div>);
  }
  if (view === "list" && query.error) {
    return shell(<div className="rounded-lg border border-destructive/30 bg-destructive/5 p-4 text-sm text-destructive">{t("table.readError", { error: String(query.error) })}</div>);
  }
  if (view === "groups" && groupQuery.isLoading) {
    return shell(<div className="grid min-h-72 place-items-center" role="status"><Loader2 className="size-5 animate-spin text-muted-foreground" /><span className="sr-only">{t("common.actions.loading")}</span></div>);
  }

  return shell(
    <>
      {view === "list" ? (
        <div className="flex min-h-0 flex-1 flex-col gap-3">
          <CreatorBulkMoveBar
            creatorIds={selectedCreatorIds}
            groups={groups}
            groupsLoading={groupChoicesLoading}
            groupsError={groupChoicesError}
            onRetryGroups={retryGroupPages}
            onClear={() => setRowSelection({})}
          />
          <DataTable
            columns={columns}
            data={rows}
            emptyText={
              <div className="flex flex-col items-center gap-3 py-6">
                <span>{t("table.empty")}</span>
              </div>
            }
            onRowClick={openCreator}
            pinnedRight={["actions"]}
            // 创建时间在列表里与更新时间重复,默认收起(视图 → 显示列可以拿回来)。
            initialColumnVisibility={{ createdAt: false }}
            columnFilters={columnFilters}
            onColumnFiltersChange={setColumnFilters}
            sorting={sorting}
            onSortingChange={setSorting}
            manualFiltering
            manualSorting
            facetedUniqueValues={facets}
            getRowId={(row) => row.id}
            pinnedRowIds={pinnedRowIds}
            rowSelection={rowSelection}
            onRowSelectionChange={setRowSelection}
            enableRowSelection
            virtual={{
              resetKey: listResetKey,
              onEndReached: () => {
                if (canLoadMore) void query.fetchNextPage();
              },
              footer: query.hasNextPage ? (
                <Button
                  variant="ghost"
                  size="sm"
                  className="w-full"
                  disabled={!canLoadMore}
                  onClick={() => { if (canLoadMore) void query.fetchNextPage(); }}
                >
                  {query.isFetchingNextPage ? t("table.loadingMore") : t("table.loadMore")}
                </Button>
              ) : null,
            }}
            toolbar={{
              search: { value: search, onChange: setSearch },
              searchPlaceholder: t("table.searchPlaceholder"),
              controls: <CreatorViewToggle value={view} onChange={setView} />,
              actions: (
                <div className="flex flex-wrap items-center gap-2">
                  <ManualCreatorDialog />
                  <CreatorTransferMenu disabled={!runtimeAvailable} container={canvas} />
                  <Button
                    variant="outline"
                    size="icon"
                    className="size-8"
                    disabled={query.isFetching}
                    onClick={() => void query.refetch()}
                    aria-label={t("table.refreshList")}
                    title={t("table.refreshList")}
                  >
                    <RefreshCw className={cn("size-4", query.isFetching && "animate-spin")} />
                  </Button>
                </div>
              ),
            }}
          />
        </div>
      ) : (
        <div className="flex min-h-0 flex-1 flex-col gap-3">
          <div className="flex flex-wrap items-center justify-between gap-2">
            <div className="flex min-w-0 flex-1 flex-wrap items-center gap-2">
              <Input
                value={search}
                onChange={(event) => setSearch(event.target.value)}
                placeholder={t("table.searchPlaceholder")}
                className="h-8 w-full sm:w-[180px] lg:w-[280px]"
              />
              <CreatorViewToggle value={view} onChange={setView} />
            </div>
            <div className="flex flex-wrap items-center gap-2">
              <CreateCreatorGroupDialog container={canvas} />
              <ManualCreatorDialog />
              <CreatorTransferMenu disabled={!runtimeAvailable} container={canvas} />
              <Button
                variant="outline"
                size="icon"
                className="size-8"
                disabled={groupQuery.isFetching}
                onClick={() => {
                  void groupQuery.refetch();
                  void queryClient.invalidateQueries({ queryKey: CREATORS_QUERY_KEY });
                  void queryClient.invalidateQueries({ queryKey: CREATOR_GROUPS_QUERY_KEY });
                }}
                aria-label={t("table.refreshGroups")}
                title={t("table.refreshGroups")}
              >
                <RefreshCw className={cn("size-4", groupQuery.isFetching && "animate-spin")} />
              </Button>
            </div>
          </div>
          <CreatorBulkMoveBar
            creatorIds={selectedCreatorIds}
            groups={groups}
            groupsLoading={groupChoicesLoading}
            groupsError={groupChoicesError}
            onRetryGroups={retryGroupPages}
            onClear={() => setRowSelection({})}
          />
          {groupQuery.error && !groupQuery.data?.pages.length ? (
            <div className="flex min-h-48 flex-1 flex-col items-center justify-center gap-3 rounded-md border border-destructive/30 bg-destructive/5 p-4 text-sm text-destructive" role="alert">
              <span>{t("groups.readError")}</span>
              <Button variant="outline" size="sm" onClick={retryGroupPages}>{t("common.actions.retry")}</Button>
            </div>
          ) : (
            <CreatorGroupsView
              groups={groups}
              uncategorizedCount={uncategorizedCount}
              search={debouncedSearch}
              columns={columns}
              rowSelection={rowSelection}
              onRowSelectionChange={setRowSelection}
              onRowClick={openCreator}
              hasNextGroupPage={Boolean(groupQuery.hasNextPage)}
              isFetchingNextGroupPage={groupQuery.isFetchingNextPage}
              groupPageError={groupQuery.error ? String(groupQuery.error) : null}
              onFetchNextGroupPage={() => void groupQuery.fetchNextPage()}
              onRetryGroupPage={retryGroupPages}
              container={canvas}
            />
          )}
        </div>
      )}

      <Sheet
        open={Boolean(selectedId)}
        onOpenChange={(open) => !open && setSelectedId(null)}
      >
        <SheetContent
          container={canvas}
          onCloseAutoFocus={(event) => {
            const target = creatorDetailReturnFocus.current;
            if (!target?.isConnected) return;
            event.preventDefault();
            target.focus();
          }}
          className={CREATOR_DETAIL_SHEET_CLASS}
        >
          <SheetHeader className="shrink-0 border-b px-5 py-4">
            <SheetTitle>{t("detail.title")}</SheetTitle>
            <SheetDescription>{t("detail.description")}</SheetDescription>
          </SheetHeader>
          <div className="min-h-0 flex-1 overflow-y-auto">
            {detailQuery.isLoading ? (
              <div className="grid min-h-48 place-items-center" role="status"><Loader2 className="size-5 animate-spin text-muted-foreground" /><span className="sr-only">{t("common.actions.loading")}</span></div>
            ) : detailQuery.error ? (
              <div className="p-5 text-sm text-destructive">{t("detail.readError", { error: String(detailQuery.error) })}</div>
            ) : selected && (
              <CreatorDetail
                record={selected}
                onStage={(stage) => void updateStage(selected.creator.id, selected.relationship.stage, stage)}
              />
            )}
          </div>
          <SheetFooter className="shrink-0 gap-2 border-t p-4 sm:flex-col">
            {selected && (isArchivedCreator(selected) ? (
              <div className="space-y-2">
                <div className="rounded-lg bg-muted px-3 py-2 text-center text-sm text-muted-foreground">
                  {t("detail.archivedNotice")}
                </div>
                <Button
                  variant="ghost"
                  size="sm"
                  className="w-full text-destructive hover:text-destructive"
                  onClick={() => setPendingDelete({ id: selected.creator.id, displayName: selected.creator.display_name })}
                >
                  <Trash2 className="size-4" />{t("detail.permanentDelete")}
                </Button>
              </div>
            ) : (
              <div className="space-y-2">
                <Button
                  variant="destructive"
                  className="w-full"
                  onClick={() => setPendingArchive({ id: selected.creator.id, displayName: selected.creator.display_name })}
                >
                  <Archive className="size-4" />{t("table.archive")}
                </Button>
                <Button
                  variant="ghost"
                  size="sm"
                  className="w-full text-destructive hover:text-destructive"
                  onClick={() => setPendingDelete({ id: selected.creator.id, displayName: selected.creator.display_name })}
                >
                  <Trash2 className="size-4" />{t("detail.permanentDelete")}
                </Button>
              </div>
            ))}
            <SheetClose asChild>
              <Button type="button" variant="outline" className="w-full">{t("detail.close")}</Button>
            </SheetClose>
          </SheetFooter>
        </SheetContent>
      </Sheet>

      <Dialog open={Boolean(pendingArchive)} onOpenChange={(open) => !open && setPendingArchive(null)}>
        <DialogContent container={canvas}>
          <DialogHeader>
            <DialogTitle>{t("archiveDialog.title")}</DialogTitle>
            <DialogDescription>
              {t("archiveDialog.description", { name: pendingArchive?.displayName ?? "" })}
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button variant="outline" disabled={archiving} onClick={() => setPendingArchive(null)}>{t("common.actions.cancel")}</Button>
            <Button variant="destructive" disabled={archiving} onClick={() => void archive()}>
              {archiving ? t("archiveDialog.submitting") : t("archiveDialog.submit")}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      <Dialog open={Boolean(pendingDelete)} onOpenChange={(open) => !open && setPendingDelete(null)}>
        <DialogContent container={canvas}>
          <DialogHeader>
            <DialogTitle>{t("deleteCreatorDialog.title")}</DialogTitle>
            <DialogDescription>
              {t("deleteCreatorDialog.description", { name: pendingDelete?.displayName ?? "" })}
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button variant="outline" disabled={deleting} onClick={() => setPendingDelete(null)}>{t("common.actions.cancel")}</Button>
            <Button variant="destructive" disabled={deleting} onClick={() => void removeCreator()}>
              {deleting ? t("deleteCreatorDialog.submitting") : t("deleteCreatorDialog.submit")}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </>,
  );
}

function CreatorDetail({ record, onStage }: { record: CreatorRecord; onStage: (stage: CreatorStage) => void }) {
  const { i18n, t } = useTranslation("leads");
  const locale = i18n.resolvedLanguage ?? i18n.language;
  const [editingRoute, setEditingRoute] = useState<CreatorContactRouteRecord | null>(null);
  const [deletingRoute, setDeletingRoute] = useState<CreatorContactRouteRecord | null>(null);
  const primary = primaryCreatorAccount(record);
  const accounts = creatorAccountViews(record);
  const source = creatorSourcePresentation(record);
  const sourceLabel = source.kind === "platform"
    ? PLATFORM_LABEL[source.accounts[0].platform]
    : source.kind === "manual_entry"
      ? t("detail.sourceManualEntry")
      : source.kind === "creator_import"
        ? t("detail.sourceCreatorImport")
        : t("common.unavailable");
  const archived = isArchivedCreator(record);
  const actionsEnabled = creatorDetailActionsEnabled(archived);
  const followerEntries = Object.entries(record.creator.follower_counts ?? {}) as [CreatorPlatform, number | bigint][];

  useEffect(() => {
    if (editingRoute && (!actionsEnabled || !record.contact_routes.some((route) => (
      route.route.id === editingRoute.route.id && route.allowed_actions.includes("update")
    )))) setEditingRoute(null);
    if (deletingRoute && (!actionsEnabled || !record.contact_routes.some((route) => (
      route.route.id === deletingRoute.route.id && route.allowed_actions.includes("delete")
    )))) setDeletingRoute(null);
  }, [actionsEnabled, deletingRoute, editingRoute, record.contact_routes]);

  return (
    <div>
      <div className="space-y-4 p-5 pb-4">
        <div className="flex items-center gap-3">
          <CreatorIdentityAvatar platform={primary?.platform} name={record.creator.display_name} src={creatorRecordAvatarUrl(record)} className="size-12" />
          <div className="min-w-0 flex-1">
            {/* 详情面板不再截断:标题一行放不下就换行,别把出口也堵上。 */}
            <div className="wrap-anywhere text-lg font-semibold">{record.creator.display_name}</div>
            <div className="wrap-anywhere text-sm text-muted-foreground">
              {primary?.handle
                ? `@${primary.handle}`
                : primary?.external_id ?? (source.kind === "platform" ? t("table.noPlatformAccount") : sourceLabel)}
            </div>
          </div>
          {actionsEnabled && primary?.profile_url && (
            <Button variant="ghost" size="icon" onClick={() => void openExternalUrl(primary.profile_url!)}>
              <ExternalLink className="size-4" /><span className="sr-only">{t("detail.viewProfile")}</span>
            </Button>
          )}
        </div>
        <div className="flex flex-wrap items-start gap-4">
          <div className="w-44 shrink-0 space-y-1.5">
            <div className="text-xs font-medium text-muted-foreground">{t("detail.globalStage")}</div>
            {archived ? (
              <Badge variant="secondary" className={cn("border-transparent", STAGE_TONE.archived)}>{creatorStageLabel(t, "archived")}</Badge>
            ) : (
              <Select value={record.relationship.stage} onValueChange={(value) => onStage(value as CreatorStage)}>
                <SelectTrigger className="w-44"><SelectValue /></SelectTrigger>
                <SelectContent>
                  {(["prospect", "engaged", "collaborating"] as CreatorStage[])
                    .map((stage) => <SelectItem key={stage} value={stage}>{creatorStageLabel(t, stage)}</SelectItem>)}
                </SelectContent>
              </Select>
            )}
          </div>
          <CreatorTagsEditor key={record.creator.id} creatorId={record.creator.id}
            tags={record.creator.tags} editable={actionsEnabled} />
        </div>
        <div className="space-y-1.5">
          <div className="text-xs font-medium text-muted-foreground">{t("detail.group")}</div>
          <Badge variant="outline">{record.group?.name ?? t("detail.uncategorized")}</Badge>
        </div>
        <div className="grid grid-cols-2 gap-3 text-sm">
          <div className="rounded-lg border p-3">
            <div className="text-xs text-muted-foreground">{t("detail.firstCapturedSource")}</div>
            <div className="mt-1 font-medium">
              {sourceLabel}
            </div>
          </div>
          <div className="rounded-lg border p-3"><div className="text-xs text-muted-foreground">{t("detail.lastTouch")}</div><div className="mt-1 font-medium">{formatDate(record.relationship.last_touch_at, locale)}</div></div>
          <div className="rounded-lg border p-3">
            <div className="text-xs text-muted-foreground">{t("detail.agentScore")}</div>
            <div className="mt-1"><CreatorScoreBadge score={record.creator.score} assessment={record.creator.assessment} /></div>
          </div>
          <div className="rounded-lg border p-3">
            <div className="text-xs text-muted-foreground">{t("detail.followers")}</div>
            {followerEntries.length > 0 ? (
              <div className="mt-1.5 flex flex-wrap gap-1.5">
                {followerEntries.map(([platform, count]) => (
                  <Badge key={platform} variant="secondary" className="gap-1 border-transparent tabular-nums">
                    <PlatformIcon platform={platform} className="size-3" />
                    {formatCount(count, locale)}
                  </Badge>
                ))}
              </div>
            ) : <span className="mt-1 text-sm text-muted-foreground">{t("detail.noData")}</span>}
          </div>
        </div>
        {record.creator.assessment && (
          <div className="rounded-lg border p-3 text-sm">
            <div className="text-xs text-muted-foreground">{t("detail.agentAssessment")}</div>
            <div className="mt-1 whitespace-pre-wrap">{record.creator.assessment}</div>
          </div>
        )}
      </div>
      <Separator />
      <div className="p-5 pt-4">
        <div className="mb-2 flex items-center justify-between">
          <div className="text-sm font-medium">{t("detail.contactRoutes")}</div>
          <div className="flex items-center gap-2">
            <span className="text-xs text-muted-foreground">{t("detail.contactRouteCount", { count: accounts.length + record.contact_routes.length })}</span>
            <ManualContactRouteDialog creatorId={record.creator.id} disabled={archived} />
          </div>
        </div>
        <SavedContactRoutes
          routes={record.contact_routes}
          accounts={accounts}
          scrollable={false}
          readOnly={!actionsEnabled}
          onEdit={actionsEnabled ? setEditingRoute : undefined}
          onDelete={actionsEnabled ? setDeletingRoute : undefined}
        />
        <EditContactRouteDialog route={editingRoute} onClose={() => setEditingRoute(null)} />
        <DeleteContactRouteDialog route={deletingRoute} onClose={() => setDeletingRoute(null)} />
      </div>
    </div>
  );
}
