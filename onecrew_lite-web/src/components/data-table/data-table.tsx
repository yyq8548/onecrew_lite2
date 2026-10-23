import { type CSSProperties, useEffect, useRef, useState } from "react";
import {
  type Cell,
  type Column,
  type ColumnDef,
  type ColumnFiltersState,
  type ColumnPinningState,
  type Header,
  type Row,
  type RowData,
  type OnChangeFn,
  type RowSelectionState,
  type SortingState,
  type VisibilityState,
  flexRender,
  getCoreRowModel,
  getFacetedRowModel,
  getFacetedUniqueValues,
  getFilteredRowModel,
  getPaginationRowModel,
  getSortedRowModel,
  useReactTable,
} from "@tanstack/react-table";
import { useVirtualizer } from "@tanstack/react-virtual";
import { useTranslation } from "react-i18next";

import {
  Table,
  TableBody,
  TableCell,
  TableFooter,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import { cn } from "@/lib/utils";
import { DataTablePagination } from "./pagination";
import { DataTableToolbar } from "./toolbar";
import { usePersistedColumnFilters } from "./filter-state";

declare module "@tanstack/react-table" {
  interface ColumnMeta<TData extends RowData, TValue> {
    /** Display name for the "view options" (show/hide columns) menu, so it can
     *  render a localized label instead of the raw column id. */
    label?: string;
    /** Grid layout only: this column absorbs the leftover width instead of
     *  holding its declared `size`. */
    grow?: boolean;
  }
}

/** Sticky offset styles for a pinned column — TanStack's official pattern
 *  (see docs/framework/react/examples/column-pinning-sticky). Returns `{}` for
 *  unpinned columns, so unpinned tables render exactly as before. */
function getPinnedStyles<TData>(column: Column<TData, unknown>): CSSProperties {
  const isPinned = column.getIsPinned();
  if (!isPinned) return {};
  return {
    position: "sticky",
    left: isPinned === "left" ? column.getStart("left") : undefined,
    right: isPinned === "right" ? column.getAfter("right") : undefined,
    zIndex: 1,
  };
}

/** Grid layout means the browser no longer sizes columns from their content:
 *  every cell is a flex item whose basis is the column's declared `size` and
 *  whose floor is its declared `minSize`.
 *  Between the two the column compacts and its content ellipsizes; below the
 *  floor the table scrolls sideways instead. TanStack's default `minSize` is
 *  20, far too low to be a floor, so a column that declares none simply does
 *  not shrink. A pinned column never shrinks either — it holds the row's last
 *  reachable control. */
function getGridCellStyles<TData>(column: Column<TData, unknown>): CSSProperties {
  const size = column.getSize();
  const floor = column.columnDef.minSize ?? size;
  return {
    display: "flex",
    alignItems: "center",
    flex: column.columnDef.meta?.grow
      ? `1 1 ${size}px`
      : column.getIsPinned()
        ? `0 0 ${size}px`
        : `0 1 ${size}px`,
    minWidth: floor,
    // 头尾都要:TableHead/TableCell 自带 whitespace-nowrap 但没有 overflow,
    // 被压窄的表头会溢出到邻列上。
    overflow: "hidden",
  };
}

// Normal-flow table sections contribute intrinsic width while virtual rows do
// not. One definite section/row width keeps both renderers on the same column
// flex contract.
const GRID_SECTION_STYLES: CSSProperties = { display: "grid", width: "100%", minWidth: 0 };
const GRID_ROW_STYLES: CSSProperties = { display: "flex", width: "100%", minWidth: 0 };

type ToolbarConfig = {
  searchKey?: string;
  searchPlaceholder?: string;
  /** Controlled search bridged to an external (e.g. backend) query. */
  search?: { value: string; onChange: (value: string) => void };
  /** Controls placed beside search, before the right-aligned actions. */
  controls?: React.ReactNode;
  actions?: React.ReactNode;
  filters?: {
    columnId: string;
    title: string;
    options: {
      label: string;
      value: string;
      icon?: React.ComponentType<{ className?: string }>;
    }[];
  }[];
};

/** Windowed rendering for a long, backend-paged list. Opt-in per call site:
 *  a table left without this prop renders and paginates exactly as before.
 *  Requires every column to declare `size` (see `getGridCellStyles`). */
type VirtualConfig = {
  /** Changing query criteria resets the viewport without remounting controls. */
  resetKey?: string;
  /** Row height used before a row has been measured. */
  estimatedRowHeight?: number;
  /** Fired while the viewport is within `endReachedThreshold` of the bottom.
   *  Callers are expected to ignore it while a fetch is already in flight. */
  onEndReached?: () => void;
  endReachedThreshold?: number;
  /** Rendered under the last row, inside the scroll container. */
  footer?: React.ReactNode;
};

type DataTableProps<TData, TValue> = {
  columns: ColumnDef<TData, TValue>[];
  data: TData[];
  /** Omit to hide the toolbar entirely. */
  toolbar?: ToolbarConfig;
  pageSize?: number;
  emptyText?: React.ReactNode;
  /** Column ids to pin sticky to the right (e.g. an actions column) so they stay
   *  visible when the table is wider than its container. */
  pinnedRight?: string[];
  initialColumnFilters?: ColumnFiltersState;
  columnFilters?: ColumnFiltersState;
  onColumnFiltersChange?: OnChangeFn<ColumnFiltersState>;
  sorting?: SortingState;
  onSortingChange?: OnChangeFn<SortingState>;
  /** Opt in only when the runtime has already filtered/sorted the full list. */
  manualFiltering?: boolean;
  manualSorting?: boolean;
  facetedUniqueValues?: Record<string, Map<string, number>>;
  /** 初始隐藏的列。columnVisibility 是受控状态,塞进 initialState 会被忽略。 */
  initialColumnVisibility?: VisibilityState;
  /** 提供后,列筛选状态跨页面/会话持久(localStorage);恢复值优先于
   *  initialColumnFilters,损坏的存储回落默认。 */
  filtersStorageKey?: string;
  /** tanstack row pinning(官方特性):这些行 id 钉在表头下(凌驾列排序、
   *  服从筛选);需要同时提供 getRowId。虚拟模式下它们渲染在窗口之外,
   *  滚出视口也不会被卸载。 */
  pinnedRowIds?: string[];
  getRowId?: (row: TData) => string;
  onRowClick?: (row: TData) => void;
  rowSelection?: RowSelectionState;
  onRowSelectionChange?: OnChangeFn<RowSelectionState>;
  enableRowSelection?: boolean | ((row: Row<TData>) => boolean);
  /** Disable client-side pagination for bounded section tables. */
  pagination?: boolean;
  /** Reuse declared column widths without enabling an inner virtual scroller. */
  layout?: "auto" | "grid";
  /** Let a parent container provide the border and corner treatment. */
  surface?: "standalone" | "embedded";
  virtual?: VirtualConfig;
};

export function DataTable<TData, TValue>({
  columns,
  data,
  toolbar,
  pageSize = 10,
  emptyText,
  pinnedRight,
  initialColumnFilters = [],
  columnFilters: controlledColumnFilters,
  onColumnFiltersChange,
  sorting: controlledSorting,
  onSortingChange,
  manualFiltering = false,
  manualSorting = false,
  facetedUniqueValues,
  initialColumnVisibility = {},
  filtersStorageKey,
  pinnedRowIds,
  getRowId,
  onRowClick,
  rowSelection,
  onRowSelectionChange,
  enableRowSelection,
  pagination = true,
  layout = "auto",
  surface = "standalone",
  virtual,
}: DataTableProps<TData, TValue>) {
  const { t } = useTranslation("common");
  const resolvedEmptyText = emptyText === undefined ? t("dataTable.empty") : emptyText;
  const [localSorting, setSorting] = useState<SortingState>([]);
  const [localColumnFilters, handleColumnFiltersChange] = usePersistedColumnFilters(
    controlledColumnFilters === undefined ? filtersStorageKey : undefined,
    initialColumnFilters,
  );
  const columnFilters = controlledColumnFilters ?? localColumnFilters;
  const sorting = controlledSorting ?? localSorting;
  const facetsRef = useRef(facetedUniqueValues);
  facetsRef.current = facetedUniqueValues;
  const [columnVisibility, setColumnVisibility] =
    useState<VisibilityState>(initialColumnVisibility);
  const columnPinning: ColumnPinningState = { left: [], right: pinnedRight ?? [] };

  const rowPinning = { top: pinnedRowIds ?? [], bottom: [] };
  const gridLayout = Boolean(virtual) || layout === "grid";
  const table = useReactTable({
    data,
    columns,
    getRowId,
    state: {
      sorting,
      columnFilters,
      columnVisibility,
      rowPinning,
      rowSelection: rowSelection ?? {},
    },
    // 置顶行从分页/排序流中提出;被筛选掉的置顶行不显示。
    keepPinnedRows: false,
    onSortingChange: onSortingChange ?? setSorting,
    onColumnFiltersChange: onColumnFiltersChange ?? handleColumnFiltersChange,
    manualFiltering,
    manualSorting,
    onColumnVisibilityChange: setColumnVisibility,
    onRowSelectionChange,
    enableRowSelection,
    getCoreRowModel: getCoreRowModel(),
    getSortedRowModel: getSortedRowModel(),
    getFilteredRowModel: getFilteredRowModel(),
    // 虚拟模式一次持有全部已加载行,分页行模型会把它们切掉。
    ...(virtual || !pagination ? {} : { getPaginationRowModel: getPaginationRowModel() }),
    getFacetedRowModel: getFacetedRowModel(),
    getFacetedUniqueValues: manualFiltering
      ? (_table, columnId) => () => facetsRef.current?.[columnId] ?? new Map<string, number>()
      : getFacetedUniqueValues(),
    initialState: { pagination: { pageSize }, columnPinning, columnFilters: initialColumnFilters },
  });

  const topRows = table.getTopRows();
  const centerRows = table.getCenterRows();
  const scrollRef = useRef<HTMLDivElement>(null);
  const rowVirtualizer = useVirtualizer({
    count: virtual ? centerRows.length : 0,
    getScrollElement: () => scrollRef.current,
    // 行高随通道徽章数量变化,所以估算只是首帧占位,真实高度由
    // measureElement(下面挂在每行 ref 上)回填,否则滚动条会说谎。
    estimateSize: () => virtual?.estimatedRowHeight ?? 52,
    overscan: 8,
  });
  useEffect(() => {
    if (virtual?.resetKey !== undefined && scrollRef.current) {
      rowVirtualizer.scrollToOffset(0);
    }
  }, [virtual?.resetKey, rowVirtualizer]);

  // 表格不能比所有可见列的下限之和更窄;再窄才横向滚动。
  const totalMinWidth = table
    .getVisibleLeafColumns()
    .reduce((sum, column) => sum + (column.columnDef.minSize ?? column.getSize()), 0);

  const headerCellClass = (pinned: false | "left" | "right") =>
    cn(pinned && "bg-background", pinned === "right" && "border-l");

  const renderHeaderCell = (header: Header<TData, unknown>) => {
    const pinned = header.column.getIsPinned();
    return (
      <TableHead
        key={header.id}
        style={{
          ...getPinnedStyles(header.column),
          ...(gridLayout ? getGridCellStyles(header.column) : {}),
        }}
        className={headerCellClass(pinned)}
      >
        {header.isPlaceholder
          ? null
          : flexRender(header.column.columnDef.header, header.getContext())}
      </TableHead>
    );
  };

  const renderBodyCell = (cell: Cell<TData, unknown>) => {
    const pinned = cell.column.getIsPinned();
    return (
      <TableCell
        key={cell.id}
        style={{
          ...getPinnedStyles(cell.column),
          ...(gridLayout ? getGridCellStyles(cell.column) : {}),
        }}
        className={headerCellClass(pinned)}
      >
        {flexRender(cell.column.columnDef.cell, cell.getContext())}
      </TableCell>
    );
  };

  const rowInteraction = (row: Row<TData>) => ({
    tabIndex: onRowClick ? 0 : undefined,
    onClick: () => onRowClick?.(row.original),
    onKeyDown: (event: React.KeyboardEvent) => {
      if (onRowClick && (event.key === "Enter" || event.key === " ")) {
        event.preventDefault();
        onRowClick(row.original);
      }
    },
    className: onRowClick
      ? "cursor-pointer focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-inset"
      : undefined,
  });

  const isEmpty = topRows.length + centerRows.length === 0;

  const plainBody = (
    <TableBody style={gridLayout ? GRID_SECTION_STYLES : undefined}>
      {isEmpty ? (
        <TableRow style={gridLayout ? GRID_ROW_STYLES : undefined}>
          <TableCell
            colSpan={columns.length}
            className={cn("h-24 text-center text-muted-foreground", gridLayout && "flex-1")}
          >
            {resolvedEmptyText}
          </TableCell>
        </TableRow>
      ) : (
        [...topRows, ...centerRows].map((row) => (
          <TableRow
            key={row.id}
            data-state={row.getIsSelected() ? "selected" : undefined}
            aria-selected={row.getIsSelected()}
            {...rowInteraction(row)}
            style={gridLayout ? GRID_ROW_STYLES : undefined}
          >
            {row.getVisibleCells().map(renderBodyCell)}
          </TableRow>
        ))
      )}
    </TableBody>
  );

  const virtualBody = (
    <TableBody
      style={{
        ...GRID_SECTION_STYLES,
        position: "relative",
        // 空态时总高是确定的 0,空态行会溢出到 tbody 外面(tfoot 会盖上去)。
        height: isEmpty ? undefined : rowVirtualizer.getTotalSize(),
      }}
    >
      {isEmpty ? (
        <TableRow style={GRID_ROW_STYLES}>
          <TableCell
            colSpan={columns.length}
            className="h-24 flex-1 text-center text-muted-foreground"
          >
            {resolvedEmptyText}
          </TableCell>
        </TableRow>
      ) : (
        rowVirtualizer.getVirtualItems().map((item) => {
          const row = centerRows[item.index];
          const interaction = rowInteraction(row);
          return (
            <TableRow
              key={row.id}
              data-index={item.index}
              ref={rowVirtualizer.measureElement}
              data-state={row.getIsSelected() ? "selected" : undefined}
              aria-selected={row.getIsSelected()}
              {...interaction}
              className={cn(interaction.className, "absolute w-full")}
              style={{ ...GRID_ROW_STYLES, transform: `translateY(${item.start}px)` }}
            >
              {row.getVisibleCells().map(renderBodyCell)}
            </TableRow>
          );
        })
      )}
    </TableBody>
  );

  return (
    <div className={cn("flex w-full flex-1 flex-col gap-4", virtual && "min-h-0")}>
      {toolbar && (
        <DataTableToolbar
          table={table}
          searchKey={toolbar.searchKey}
          searchPlaceholder={toolbar.searchPlaceholder}
          search={toolbar.search}
          controls={toolbar.controls}
          filters={toolbar.filters}
          actions={toolbar.actions}
          initialColumnFilters={initialColumnFilters}
        />
      )}
      <div
        className={cn(
          "overflow-hidden",
          surface === "standalone" && "rounded-md border",
          virtual && "flex min-h-0 flex-1 flex-col",
        )}
      >
        <Table
          style={gridLayout
            ? { display: "grid", width: "100%", minWidth: totalMinWidth }
            : undefined}
          containerProps={
            virtual
              ? {
                  ref: scrollRef,
                  className: "min-h-0 flex-1",
                  // overflow 走内联样式而不是 class:基元自带 overflow-x-auto,
                  // 纵向必须也能滚。overflow-anchor 与虚拟化互斥 —— 浏览器会
                  // 锚到一个即将被卸载的行上。
                  style: { overflow: "auto", overflowAnchor: "none" },
                  onScroll: (event) => {
                    const element = event.currentTarget;
                    const distance =
                      element.scrollHeight - element.scrollTop - element.clientHeight;
                    if (distance < (virtual.endReachedThreshold ?? 500)) {
                      virtual.onEndReached?.();
                    }
                  },
                }
              : undefined
          }
        >
          <TableHeader
            // 虚拟模式下表头与置顶行同处一个 sticky 块:置顶行不进虚拟窗口,
            // 也就不必测量表头高度去对齐第二层 sticky 偏移。
            style={gridLayout
              ? {
                  ...GRID_SECTION_STYLES,
                  ...(virtual ? { position: "sticky", top: 0, zIndex: 2 } : {}),
                }
              : undefined}
            className={cn(gridLayout && "bg-background")}
          >
            {table.getHeaderGroups().map((hg) => (
              <TableRow
                key={hg.id}
                style={gridLayout ? GRID_ROW_STYLES : undefined}
              >
                {hg.headers.map(renderHeaderCell)}
              </TableRow>
            ))}
            {virtual &&
              topRows.map((row) => (
                <TableRow
                  key={row.id}
                  data-state={row.getIsSelected() ? "selected" : undefined}
                  aria-selected={row.getIsSelected()}
                  {...rowInteraction(row)}
                  style={GRID_ROW_STYLES}
                >
                  {row.getVisibleCells().map(renderBodyCell)}
                </TableRow>
              ))}
          </TableHeader>
          {virtual ? virtualBody : plainBody}
          {virtual?.footer && (
            <TableFooter
              style={{ display: "grid" }}
              className="border-t-0 bg-transparent font-normal"
            >
              <TableRow style={GRID_ROW_STYLES}>
                <TableCell className="flex-1">{virtual.footer}</TableCell>
              </TableRow>
            </TableFooter>
          )}
        </Table>
      </div>
      {!virtual && pagination && <DataTablePagination table={table} />}
    </div>
  );
}
