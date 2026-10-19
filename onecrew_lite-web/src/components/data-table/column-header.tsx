import { ArrowDown, ArrowUp, ChevronsUpDown, EyeOff, ListFilter } from "lucide-react";
import { type Column } from "@tanstack/react-table";
import { useTranslation } from "react-i18next";

import { cn } from "@/lib/utils";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover";
import {
  DataTableFacetedFilterContent,
  type DataTableFacetedFilterOption,
} from "./faceted-filter";
import { DataTableDateFilter } from "./date-filter";

type DataTableColumnHeaderProps<TData, TValue> =
  React.HTMLAttributes<HTMLDivElement> & {
    column: Column<TData, TValue>;
    title: string;
    /** Faceted filter options surfaced directly on the header: the upstream
     * shadcn faceted-filter panel opens from a filter icon next to the sort
     * menu instead of a toolbar button. */
    filterOptions?: DataTableFacetedFilterOption[];
    /** Calendar range filter surfaced beside, not inside, the sort menu. */
    dateFilter?: boolean;
  };

function HeaderFilter<TData, TValue>({
  column,
  title,
  options,
}: {
  column: Column<TData, TValue>;
  title: string;
  options: DataTableFacetedFilterOption[];
}) {
  const { t } = useTranslation("common");
  const selectedCount = ((column.getFilterValue() as string[]) ?? []).length;
  return (
    <Popover>
      <PopoverTrigger asChild>
        <Button
          variant="ghost"
          size="sm"
          className={cn(
            "h-8 px-1.5 data-[state=open]:bg-accent",
            selectedCount === 0 && "text-muted-foreground/70",
          )}
        >
          <ListFilter className="size-4" />
          {selectedCount > 0 && (
            <Badge variant="secondary" className="rounded-sm px-1 font-normal">
              {selectedCount}
            </Badge>
          )}
          <span className="sr-only">{t("dataTable.filter.ariaLabel", { title })}</span>
        </Button>
      </PopoverTrigger>
      <PopoverContent className="w-50 p-0" align="start">
        <DataTableFacetedFilterContent column={column} title={title} options={options} />
      </PopoverContent>
    </Popover>
  );
}

export function DataTableColumnHeader<TData, TValue>({
  column,
  title,
  filterOptions,
  dateFilter,
  className,
}: DataTableColumnHeaderProps<TData, TValue>) {
  const { t } = useTranslation("common");
  const filter = column.getCanFilter()
    ? dateFilter
      ? <DataTableDateFilter column={column} title={title} />
      : filterOptions
        ? <HeaderFilter column={column} title={title} options={filterOptions} />
        : null
    : null;

  if (!column.getCanSort()) {
    return (
      <div className={cn("flex items-center gap-1", className)}>
        <span>{title}</span>
        {filter}
      </div>
    );
  }

  return (
    <div className={cn("flex items-center gap-1", className)}>
      <DropdownMenu>
        <DropdownMenuTrigger asChild>
          <Button variant="ghost" size="sm" className="-ml-3 h-8 data-[state=open]:bg-accent">
            <span>{title}</span>
            {column.getIsSorted() === "desc" ? (
              <ArrowDown className="ml-2 size-4" />
            ) : column.getIsSorted() === "asc" ? (
              <ArrowUp className="ml-2 size-4" />
            ) : (
              <ChevronsUpDown className="ml-2 size-4" />
            )}
          </Button>
        </DropdownMenuTrigger>
        <DropdownMenuContent align="start">
          <DropdownMenuItem onClick={() => column.toggleSorting(false)}>
            <ArrowUp className="size-3.5 text-muted-foreground/70" />
            {t("dataTable.sort.ascending")}
          </DropdownMenuItem>
          <DropdownMenuItem onClick={() => column.toggleSorting(true)}>
            <ArrowDown className="size-3.5 text-muted-foreground/70" />
            {t("dataTable.sort.descending")}
          </DropdownMenuItem>
          {column.getCanHide() && (
            <>
              <DropdownMenuSeparator />
              <DropdownMenuItem onClick={() => column.toggleVisibility(false)}>
                <EyeOff className="size-3.5 text-muted-foreground/70" />
                {t("dataTable.sort.hide")}
              </DropdownMenuItem>
            </>
          )}
        </DropdownMenuContent>
      </DropdownMenu>
      {filter}
    </div>
  );
}
