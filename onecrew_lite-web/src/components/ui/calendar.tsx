import * as React from "react";
import { format } from "date-fns";
import { ChevronDownIcon, ChevronLeftIcon, ChevronRightIcon } from "lucide-react";
import { DayPicker, getDefaultClassNames, type DayButton } from "react-day-picker";
import { enUS as enUSDayPicker } from "react-day-picker/locale/en-US";
import { zhCN as zhCNDayPicker } from "react-day-picker/locale/zh-CN";
import { useTranslation } from "react-i18next";

import { Button, buttonVariants } from "@/components/ui/button";
import { normalizeLocale } from "@/i18n/locale";
import {
  DEFAULT_LOCALE,
  SUPPORTED_LOCALES,
  type SupportedLocale,
} from "@/i18n/resources";
import { cn } from "@/lib/utils";

const DAY_PICKER_LOCALES = {
  [DEFAULT_LOCALE]: zhCNDayPicker,
  [SUPPORTED_LOCALES[1]]: enUSDayPicker,
} satisfies Record<SupportedLocale, typeof zhCNDayPicker>;

function Calendar({
  className,
  classNames,
  showOutsideDays = true,
  captionLayout = "label",
  buttonVariant = "ghost",
  locale: localeOverride,
  formatters,
  components,
  ...props
}: React.ComponentProps<typeof DayPicker> & {
  buttonVariant?: React.ComponentProps<typeof Button>["variant"];
}) {
  const { i18n } = useTranslation();
  const defaults = getDefaultClassNames();
  const appLocale = normalizeLocale(i18n.resolvedLanguage ?? i18n.language)
    ?? DEFAULT_LOCALE;
  return (
    <DayPicker
      showOutsideDays={showOutsideDays}
      className={cn("group/calendar bg-background p-3 [--cell-size:--spacing(8)]", className)}
      captionLayout={captionLayout}
      locale={localeOverride ?? DAY_PICKER_LOCALES[appLocale]}
      formatters={formatters}
      classNames={{
        root: cn("w-fit", defaults.root),
        months: cn("relative flex flex-col gap-4 md:flex-row", defaults.months),
        month: cn("flex w-full flex-col gap-4", defaults.month),
        nav: cn("absolute inset-x-0 top-0 flex w-full items-center justify-between gap-1", defaults.nav),
        button_previous: cn(buttonVariants({ variant: buttonVariant }), "size-(--cell-size) p-0", defaults.button_previous),
        button_next: cn(buttonVariants({ variant: buttonVariant }), "size-(--cell-size) p-0", defaults.button_next),
        month_caption: cn("flex h-(--cell-size) w-full items-center justify-center px-(--cell-size)", defaults.month_caption),
        dropdowns: cn("flex h-(--cell-size) w-full items-center justify-center gap-1.5 text-sm font-medium", defaults.dropdowns),
        dropdown_root: cn("relative rounded-md border border-input shadow-xs", defaults.dropdown_root),
        dropdown: cn("absolute inset-0 bg-popover opacity-0", defaults.dropdown),
        caption_label: cn("font-medium", captionLayout === "label" ? "text-sm" : "flex h-8 items-center gap-1 rounded-md px-2 text-sm", defaults.caption_label),
        month_grid: cn("w-full border-collapse", defaults.month_grid),
        weekdays: cn("flex", defaults.weekdays),
        weekday: cn("flex-1 rounded-md text-[0.8rem] font-normal text-muted-foreground", defaults.weekday),
        week: cn("mt-2 flex w-full", defaults.week),
        day: cn("group/day relative aspect-square h-full w-full p-0 text-center", defaults.day),
        range_start: cn("rounded-l-md bg-accent", defaults.range_start),
        range_middle: cn("rounded-none", defaults.range_middle),
        range_end: cn("rounded-r-md bg-accent", defaults.range_end),
        today: cn("rounded-md bg-accent text-accent-foreground", defaults.today),
        outside: cn("text-muted-foreground", defaults.outside),
        disabled: cn("text-muted-foreground opacity-50", defaults.disabled),
        hidden: cn("invisible", defaults.hidden),
        ...classNames,
      }}
      components={{
        Chevron: ({ className: chevronClassName, orientation, ...chevronProps }) => {
          if (orientation === "left") return <ChevronLeftIcon className={cn("size-4", chevronClassName)} {...chevronProps} />;
          if (orientation === "right") return <ChevronRightIcon className={cn("size-4", chevronClassName)} {...chevronProps} />;
          return <ChevronDownIcon className={cn("size-4", chevronClassName)} {...chevronProps} />;
        },
        DayButton: CalendarDayButton,
        ...components,
      }}
      {...props}
    />
  );
}

function CalendarDayButton({
  className,
  day,
  modifiers,
  ...props
}: React.ComponentProps<typeof DayButton>) {
  const ref = React.useRef<HTMLButtonElement>(null);
  React.useEffect(() => {
    if (modifiers.focused) ref.current?.focus();
  }, [modifiers.focused]);
  return (
    <Button
      ref={ref}
      variant="ghost"
      size="icon"
      data-day={format(day.date, "yyyy-MM-dd")}
      data-selected-single={modifiers.selected}
      className={cn(
        "flex aspect-square size-auto w-full min-w-(--cell-size) flex-col gap-1 font-normal leading-none data-[selected-single=true]:bg-primary data-[selected-single=true]:text-primary-foreground",
        className,
      )}
      {...props}
    />
  );
}

export { Calendar, CalendarDayButton };
