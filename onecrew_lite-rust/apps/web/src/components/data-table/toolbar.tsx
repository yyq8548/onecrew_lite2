import { X } from "lucide-react";
import { type ColumnFiltersState, type Table } from "@tanstack/react-table";
import { useTranslation } from "react-i18next";

import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { DataTableFacetedFilter } from "./faceted-filter";
import { DataTableViewOptions } from "./view-options";

type DataTableToolbarProps<TData> = {
  table: Table<TData>;
  searchPlaceholder?: string;
  searchKey?: string;
  /** Controlled search bridged to an external (e.g. backend) query; takes
   * precedence over searchKey/globalFilter so only one search box renders. */
  search?: { value: string; onChange: (value: string) => void };
  controls?: React.ReactNode;
  actions?: React.ReactNode;
  initialColumnFilters?: ColumnFiltersState;
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

export function DataTableToolbar<TData>({
  table,
  searchPlaceholder,
  searchKey,
  search,
  controls,
  filters = [],
  actions,
  initialColumnFilters = [],
}: DataTableToolbarProps<TData>) {
  const { t } = useTranslation("common");
  const resolvedSearchPlaceholder = searchPlaceholder ?? t("dataTable.toolbar.searchPlaceholder");
  const isFiltered =
    JSON.stringify(table.getState().columnFilters) !== JSON.stringify(initialColumnFilters)
    || Boolean(table.getState().globalFilter);

  return (
    <div className="flex flex-wrap items-center justify-between gap-2">
      <div className="flex min-w-0 flex-1 flex-col-reverse items-start gap-y-2 sm:flex-row sm:items-center sm:gap-x-2">
        {search ? (
          <Input
            placeholder={resolvedSearchPlaceholder}
            value={search.value}
            onChange={(event) => search.onChange(event.target.value)}
            className="h-8 w-full sm:w-[180px] lg:w-[280px]"
          />
        ) : searchKey ? (
          <Input
            placeholder={resolvedSearchPlaceholder}
            value={(table.getColumn(searchKey)?.getFilterValue() as string) ?? ""}
            onChange={(event) =>
              table.getColumn(searchKey)?.setFilterValue(event.target.value)
            }
            className="h-8 w-full sm:w-[150px] lg:w-[250px]"
          />
        ) : (
          <Input
            placeholder={resolvedSearchPlaceholder}
            value={table.getState().globalFilter ?? ""}
            onChange={(event) => table.setGlobalFilter(event.target.value)}
            className="h-8 w-full sm:w-[150px] lg:w-[250px]"
          />
        )}
        {controls}
        <div className="flex gap-x-2">
          {filters.map((filter) => {
            const column = table.getColumn(filter.columnId);
            if (!column) return null;
            return (
              <DataTableFacetedFilter
                key={filter.columnId}
                column={column}
                title={filter.title}
                options={filter.options}
              />
            );
          })}
        </div>
        {isFiltered && (
          <Button
            variant="ghost"
            onClick={() => {
              table.resetColumnFilters();
              table.setGlobalFilter("");
            }}
            className="h-8 px-2 lg:px-3"
          >
            {t("actions.reset")}
            <X className="ml-2 size-4" />
          </Button>
        )}
      </div>
      <div className="flex flex-wrap items-center gap-2">
        {actions}
        <DataTableViewOptions table={table} />
      </div>
    </div>
  );
}
