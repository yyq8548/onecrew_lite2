import { useState } from "react";
import { CalendarClock } from "lucide-react";
import { type Column } from "@tanstack/react-table";
import { type DateRange } from "react-day-picker";
import { useTranslation } from "react-i18next";

import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Calendar } from "@/components/ui/calendar";
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover";
import {
  localDatesToTimestampRange,
  readTimestampDateRange,
} from "@/lib/date-range-filter";
import { cn } from "@/lib/utils";

function selectedDates(value: unknown): DateRange | undefined {
  const range = readTimestampDateRange(value);
  if (!range) return undefined;
  return {
    from: new Date(range.from * 1000),
    to: new Date((range.toExclusive - 1) * 1000),
  };
}

function formatDay(date: Date, locale: string) {
  return new Intl.DateTimeFormat(locale, {
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
  }).format(date);
}

export function DataTableDateFilter<TData, TValue>({
  column,
  title,
}: {
  column: Column<TData, TValue>;
  title: string;
}) {
  const { i18n, t } = useTranslation("common");
  const locale = i18n.resolvedLanguage ?? i18n.language;
  const selected = selectedDates(column.getFilterValue());
  const [open, setOpen] = useState(false);
  const [draft, setDraft] = useState<DateRange | undefined>(() =>
    selected);
  const active = selected?.from != null;
  const selectedLabel = selected?.from
    ? selected.to && selected.to.getTime() !== selected.from.getTime()
      ? `${formatDay(selected.from, locale)} – ${formatDay(selected.to, locale)}`
      : formatDay(selected.from, locale)
    : null;

  const handleOpenChange = (next: boolean) => {
    setOpen(next);
    if (next) setDraft(selectedDates(column.getFilterValue()));
  };

  const clear = () => {
    column.setFilterValue(undefined);
    setDraft(undefined);
    setOpen(false);
  };

  const apply = () => {
    const value = localDatesToTimestampRange(draft?.from, draft?.to);
    if (!value) return;
    column.setFilterValue(value);
    setOpen(false);
  };

  return (
    <Popover open={open} onOpenChange={handleOpenChange}>
      <PopoverTrigger asChild>
        <Button
          variant="ghost"
          size="sm"
          className={cn(
            "h-8 shrink-0 gap-1 px-1.5 data-[state=open]:bg-accent",
            active
              ? "bg-accent text-accent-foreground"
              : "text-muted-foreground/70",
          )}
          aria-label={selectedLabel
            ? t("dataTable.filter.ariaLabelWithSelection", { title, selection: selectedLabel })
            : t("dataTable.filter.ariaLabel", { title })}
          aria-pressed={active}
        >
          <CalendarClock className="size-4" />
          {active && (
            <Badge variant="secondary" className="rounded-sm px-1 font-normal">
              1
            </Badge>
          )}
        </Button>
      </PopoverTrigger>
      <PopoverContent className="w-auto p-0" align="end">
        <Calendar
          mode="range"
          selected={draft}
          onSelect={setDraft}
          defaultMonth={draft?.from}
        />
        <div className="border-t px-3 py-2 text-xs text-muted-foreground">
          {draft?.from
            ? draft.to && draft.to.getTime() !== draft.from.getTime()
              ? `${formatDay(draft.from, locale)} – ${formatDay(draft.to, locale)}`
              : formatDay(draft.from, locale)
            : t("dataTable.filter.datePrompt")}
        </div>
        <div className="flex items-center justify-between gap-2 border-t p-2">
          <Button variant="ghost" size="sm" onClick={clear} disabled={!active && !draft?.from}>
            {t("actions.clear")}
          </Button>
          <Button size="sm" onClick={apply} disabled={!draft?.from}>
            {t("actions.apply")}
          </Button>
        </div>
      </PopoverContent>
    </Popover>
  );
}
