import { useMemo, useState } from "react";
import { useInfiniteQuery } from "@tanstack/react-query";
import {
  type ColumnDef,
  type OnChangeFn,
  type RowSelectionState,
} from "@tanstack/react-table";
import { ChevronDown, ChevronRight, Folder, FolderOpen, Loader2 } from "lucide-react";
import { useTranslation } from "react-i18next";

import { DataTable } from "@/components/data-table";
import { Button } from "@/components/ui/button";
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from "@/components/ui/collapsible";
import {
  type CreatorGroupFilter,
  type CreatorGroupSummary,
  type CreatorSummary,
} from "@/lib/creator";
import { creatorsInfiniteQueryOptions } from "@/lib/runtime-query";
import { CreatorGroupActions } from "@/pages/leads/creator-group-dialogs";

type CreatorGroupSectionSpec = {
  key: string;
  name: string;
  count: number;
  filter: CreatorGroupFilter;
  summary?: CreatorGroupSummary;
};

export function CreatorGroupsView({
  groups,
  uncategorizedCount,
  search,
  columns,
  rowSelection,
  onRowSelectionChange,
  onRowClick,
  hasNextGroupPage,
  isFetchingNextGroupPage,
  groupPageError,
  onFetchNextGroupPage,
  onRetryGroupPage,
  container,
}: {
  groups: CreatorGroupSummary[];
  uncategorizedCount: number;
  search: string;
  columns: ColumnDef<CreatorSummary>[];
  rowSelection: RowSelectionState;
  onRowSelectionChange: OnChangeFn<RowSelectionState>;
  onRowClick: (creator: CreatorSummary) => void;
  hasNextGroupPage: boolean;
  isFetchingNextGroupPage: boolean;
  groupPageError: string | null;
  onFetchNextGroupPage: () => void;
  onRetryGroupPage: () => void;
  container: HTMLElement | null;
}) {
  const { t } = useTranslation("leads");
  const sections = useMemo<CreatorGroupSectionSpec[]>(() => [
    ...groups.map((summary) => ({
      key: summary.group.id,
      name: summary.group.name,
      count: summary.creator_count,
      filter: { kind: "group" as const, group_id: summary.group.id },
      summary,
    })),
    {
      key: "uncategorized",
      name: t("groups.uncategorized"),
      count: uncategorizedCount,
      filter: { kind: "uncategorized" as const },
    },
  ], [groups, t, uncategorizedCount]);

  return (
    <div className="min-h-0 flex-1 overflow-y-auto rounded-md border bg-background">
      <div className="divide-y">
        {sections.map((section) => (
          <CreatorGroupSection
            key={section.key}
            section={section}
            search={search}
            columns={columns}
            rowSelection={rowSelection}
            onRowSelectionChange={onRowSelectionChange}
            onRowClick={onRowClick}
            container={container}
          />
        ))}
      </div>
      {groupPageError ? (
        <div className="flex items-center justify-between gap-3 border-t bg-destructive/5 p-3" role="alert">
          <span className="text-sm text-destructive">{t("groups.laterPagesIncomplete")}</span>
          <Button variant="outline" size="sm" onClick={onRetryGroupPage}>{t("common.actions.retry")}</Button>
        </div>
      ) : hasNextGroupPage && (
        <div className="border-t p-2">
          <Button
            variant="ghost"
            size="sm"
            className="w-full"
            disabled={isFetchingNextGroupPage}
            onClick={onFetchNextGroupPage}
          >
            {isFetchingNextGroupPage && <Loader2 className="size-4 animate-spin" />}
            {isFetchingNextGroupPage ? t("groups.loadingMore") : t("groups.loadMoreGroups")}
          </Button>
        </div>
      )}
    </div>
  );
}

function CreatorGroupSection({
  section,
  search,
  columns,
  rowSelection,
  onRowSelectionChange,
  onRowClick,
  container,
}: {
  section: CreatorGroupSectionSpec;
  search: string;
  columns: ColumnDef<CreatorSummary>[];
  rowSelection: RowSelectionState;
  onRowSelectionChange: OnChangeFn<RowSelectionState>;
  onRowClick: (creator: CreatorSummary) => void;
  container: HTMLElement | null;
}) {
  const { t } = useTranslation("leads");
  const [open, setOpen] = useState(section.filter.kind === "uncategorized");
  const query = useInfiniteQuery(creatorsInfiniteQueryOptions(open, {
    include_archived: true,
    query: search,
    group: section.filter,
  }));
  const rows = useMemo(
    () => query.data?.pages.flatMap((page) => page.items) ?? [],
    [query.data],
  );
  const pinnedRowIds = useMemo(
    () => rows
      .filter((row) => row.pinned_at != null)
      .sort((a, b) => (b.pinned_at ?? 0) - (a.pinned_at ?? 0))
      .map((row) => row.id),
    [rows],
  );

  return (
    <Collapsible open={open} onOpenChange={setOpen}>
      <div className="flex min-h-14 items-center bg-muted/25">
        <CollapsibleTrigger asChild>
          <button
            type="button"
            className="flex min-w-0 flex-1 items-center gap-2 px-4 py-3 text-left outline-none hover:bg-muted/50 focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-inset"
          >
            {open ? <ChevronDown className="size-4 shrink-0" /> : <ChevronRight className="size-4 shrink-0" />}
            {open ? <FolderOpen className="size-4 shrink-0 text-muted-foreground" /> : <Folder className="size-4 shrink-0 text-muted-foreground" />}
            <span className="truncate font-medium">{section.name}</span>
            <span className="text-xs tabular-nums text-muted-foreground">{section.count}</span>
          </button>
        </CollapsibleTrigger>
        {section.summary && (
          <CreatorGroupActions group={section.summary.group} container={container} />
        )}
      </div>
      <CollapsibleContent className="border-t">
        {query.isLoading ? (
          <div className="grid min-h-28 place-items-center" role="status">
            <Loader2 className="size-5 animate-spin text-muted-foreground" />
            <span className="sr-only">{t("common.actions.loading")}</span>
          </div>
        ) : query.error ? (
          <div className="m-3 rounded-md border border-destructive/30 bg-destructive/5 p-3 text-sm text-destructive">
            {t("groups.readContentError", { error: String(query.error) })}
          </div>
        ) : (
          <>
            <DataTable
              columns={columns}
              data={rows}
              emptyText={search.trim() ? t("groups.noSearchMatches") : t("groups.empty")}
              onRowClick={onRowClick}
              pinnedRight={["actions"]}
              initialColumnVisibility={{ createdAt: false }}
              getRowId={(row) => row.id}
              pinnedRowIds={pinnedRowIds}
              rowSelection={rowSelection}
              onRowSelectionChange={onRowSelectionChange}
              enableRowSelection
              pagination={false}
              layout="grid"
              surface="embedded"
            />
            {query.hasNextPage && (
              <div className="border-t p-2">
                <Button
                  variant="ghost"
                  size="sm"
                  className="w-full"
                  disabled={query.isFetchingNextPage}
                  onClick={() => void query.fetchNextPage()}
                >
                  {query.isFetchingNextPage && <Loader2 className="size-4 animate-spin" />}
                  {query.isFetchingNextPage ? t("groups.loadingMore") : t("groups.loadMoreCreators")}
                </Button>
              </div>
            )}
          </>
        )}
      </CollapsibleContent>
    </Collapsible>
  );
}
