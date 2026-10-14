import { type Table } from "@tanstack/react-table";
import { ChevronLeft, ChevronRight, ChevronsLeft, ChevronsRight } from "lucide-react";
import { useTranslation } from "react-i18next";

import { cn } from "@/lib/utils";
import { Button } from "@/components/ui/button";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";

type DataTablePaginationProps<TData> = {
  table: Table<TData>;
  className?: string;
};

export function DataTablePagination<TData>({
  table,
  className,
}: DataTablePaginationProps<TData>) {
  const { t } = useTranslation("common");
  const totalPages = table.getPageCount();
  const hasPages = totalPages > 0;
  const currentPage = hasPages ? table.getState().pagination.pageIndex + 1 : 0;
  const canPreviousPage = hasPages && table.getCanPreviousPage();
  const canNextPage = hasPages && table.getCanNextPage();

  return (
    <div className={cn("flex items-center justify-between px-2", className)}>
      <div className="flex-1 text-sm text-muted-foreground">
        {t("dataTable.pagination.totalRows", {
          count: table.getFilteredRowModel().rows.length,
        })}
      </div>
      <div className="flex items-center gap-6 lg:gap-8">
        <div className="flex items-center gap-2">
          <p className="text-sm font-medium">{t("dataTable.pagination.rowsPerPage")}</p>
          <Select
            value={`${table.getState().pagination.pageSize}`}
            onValueChange={(value) => table.setPageSize(Number(value))}
          >
            <SelectTrigger className="h-8 w-[70px]">
              <SelectValue placeholder={table.getState().pagination.pageSize} />
            </SelectTrigger>
            <SelectContent side="top">
              {[10, 20, 30, 40, 50].map((pageSize) => (
                <SelectItem key={pageSize} value={`${pageSize}`}>
                  {pageSize}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        </div>
        <div className="flex w-[90px] items-center justify-center text-sm font-medium">
          {t("dataTable.pagination.pageStatus", { current: currentPage, total: totalPages })}
        </div>
        <div className="flex items-center gap-2">
          <Button
            variant="outline"
            className="hidden size-8 p-0 lg:flex"
            onClick={() => table.setPageIndex(0)}
            disabled={!canPreviousPage}
          >
            <span className="sr-only">{t("dataTable.pagination.firstPage")}</span>
            <ChevronsLeft className="size-4" />
          </Button>
          <Button
            variant="outline"
            className="size-8 p-0"
            onClick={() => table.previousPage()}
            disabled={!canPreviousPage}
          >
            <span className="sr-only">{t("dataTable.pagination.previousPage")}</span>
            <ChevronLeft className="size-4" />
          </Button>
          <Button
            variant="outline"
            className="size-8 p-0"
            onClick={() => table.nextPage()}
            disabled={!canNextPage}
          >
            <span className="sr-only">{t("dataTable.pagination.nextPage")}</span>
            <ChevronRight className="size-4" />
          </Button>
          <Button
            variant="outline"
            className="hidden size-8 p-0 lg:flex"
            onClick={() => table.setPageIndex(table.getPageCount() - 1)}
            disabled={!canNextPage}
          >
            <span className="sr-only">{t("dataTable.pagination.lastPage")}</span>
            <ChevronsRight className="size-4" />
          </Button>
        </div>
      </div>
    </div>
  );
}
