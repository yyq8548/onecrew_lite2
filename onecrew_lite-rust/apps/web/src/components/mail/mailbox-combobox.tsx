import { useState } from "react";
import { Check, ChevronsUpDown, Mail } from "lucide-react";
import { useTranslation } from "react-i18next";

import { Button } from "@/components/ui/button";
import {
  Command,
  CommandEmpty,
  CommandGroup,
  CommandInput,
  CommandItem,
  CommandList,
} from "@/components/ui/command";
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover";
import type { MailboxDetail } from "@/lib/creator";
import { cn } from "@/lib/utils";

export function MailboxCombobox({
  mailboxes,
  value,
  onValueChange,
  className,
  displayValue = "email",
  contentAlign = "end",
  collapsed = false,
}: {
  mailboxes: MailboxDetail[];
  value: string | null;
  onValueChange: (value: string) => void;
  className?: string;
  displayValue?: "email" | "label";
  contentAlign?: "start" | "center" | "end";
  collapsed?: boolean;
}) {
  const { t } = useTranslation("mail");
  const [open, setOpen] = useState(false);
  const selected = mailboxes.find((mailbox) => mailbox.mailbox_ref_id === value) ?? mailboxes[0] ?? null;

  return (
    <Popover open={open} onOpenChange={setOpen}>
      <PopoverTrigger asChild>
        <Button
          variant="outline"
          role="combobox"
          aria-expanded={open}
          aria-label={selected
            ? t("mailbox.selectCurrent", { mailbox: selected.label || selected.email_address })
            : t("common.selectMailbox")}
          disabled={!selected}
          size={collapsed ? "icon" : "default"}
          className={cn(
            "h-9 justify-between bg-background font-normal",
            collapsed ? "size-9 min-w-0 justify-center px-0" : "min-w-[180px] max-w-[280px] px-2.5",
            className,
          )}
        >
          {collapsed ? <Mail className="size-4 shrink-0" /> : selected ? (
            <span className="flex min-w-0 items-center gap-2">
              <Mail className="size-4 shrink-0" />
              <span className="truncate">
                {displayValue === "label"
                  ? selected.label || selected.email_address
                  : selected.email_address}
              </span>
            </span>
          ) : (
            <span className="truncate text-muted-foreground">{t("mailbox.noneAvailable")}</span>
          )}
          {!collapsed && <ChevronsUpDown className="ml-2 size-3.5 shrink-0 opacity-50" />}
        </Button>
      </PopoverTrigger>
      <PopoverContent align={contentAlign} className="w-[330px] p-0">
        <Command>
          <CommandInput placeholder={t("mailbox.searchPlaceholder")} />
          <CommandList>
            <CommandEmpty>{t("mailbox.noMatch")}</CommandEmpty>
            <CommandGroup heading={t("mailbox.chooseSender")}>
              {mailboxes.map((mailbox) => (
                <CommandItem
                  key={mailbox.mailbox_ref_id}
                  value={`${mailbox.email_address} ${mailbox.label} ${mailbox.display_name ?? ""} ${mailbox.provider}`}
                  onSelect={() => {
                    onValueChange(mailbox.mailbox_ref_id);
                    setOpen(false);
                  }}
                >
                  <Mail className="size-4" />
                  <span className="min-w-0 flex-1">
                    <span className="block truncate">{mailbox.label || mailbox.email_address}</span>
                    <span className="block truncate text-[11px] text-muted-foreground">
                      {mailbox.email_address} · Resend
                    </span>
                  </span>
                  {!mailbox.available && <span className="text-[10px] text-destructive">{t("mailbox.unavailable")}</span>}
                  <Check className={cn("size-4", selected?.mailbox_ref_id === mailbox.mailbox_ref_id ? "opacity-100" : "opacity-0")} />
                </CommandItem>
              ))}
            </CommandGroup>
          </CommandList>
        </Command>
      </PopoverContent>
    </Popover>
  );
}
