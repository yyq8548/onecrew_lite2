import {
  Archive,
  FilePenLine,
  Inbox,
  MailOpen,
  Send,
  Trash2,
  type LucideIcon,
} from "lucide-react";
import type { TFunction } from "i18next";
import { useTranslation } from "react-i18next";

import { Button } from "@/components/ui/button";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import type { EmailFolder, EmailFolderSummary } from "@/lib/creator";
import { cn } from "@/lib/utils";

export const EMAIL_FOLDER_META: ReadonlyArray<{
  folder: EmailFolder;
  icon: LucideIcon;
}> = [
  { folder: "all", icon: MailOpen },
  { folder: "inbox", icon: Inbox },
  { folder: "sent", icon: Send },
  { folder: "drafts", icon: FilePenLine },
  { folder: "archive", icon: Archive },
  { folder: "deleted", icon: Trash2 },
] as const;

export function emailFolderLabel(folder: EmailFolder, t: TFunction<"mail">): string {
  switch (folder) {
    case "all": return t("folders.all");
    case "inbox": return t("folders.inbox");
    case "sent": return t("folders.sent");
    case "drafts": return t("folders.drafts");
    case "archive": return t("folders.archive");
    case "deleted": return t("folders.deleted");
  }
}

export function emailFolderCount(
  summaries: EmailFolderSummary[] | undefined,
  folder: EmailFolder,
): number | null {
  return summaries?.find((item) => item.folder === folder)?.count ?? null;
}

export function MailFolderNav({
  collapsed,
  folder,
  folders,
  onFolderChange,
  sentRecordsOnly = false,
}: {
  collapsed: boolean;
  folder: EmailFolder;
  folders: EmailFolderSummary[] | undefined;
  onFolderChange: (folder: EmailFolder) => void;
  sentRecordsOnly?: boolean;
}) {
  const { t } = useTranslation("mail");
  return (
    <nav className={cn("grid gap-1 p-2", collapsed && "justify-center")} aria-label={t("folders.navigationLabel")}>
      {EMAIL_FOLDER_META.filter((item) => !sentRecordsOnly || item.folder === "sent" || item.folder === "drafts").map((item) => {
        const count = emailFolderCount(folders, item.folder);
        const label = emailFolderLabel(item.folder, t);
        const button = (
          <Button
            key={item.folder}
            variant={folder === item.folder ? "default" : "ghost"}
            size={collapsed ? "icon" : "default"}
            className={cn("h-9", collapsed ? "size-9" : "w-full justify-start")}
            aria-label={count === null
              ? t("folders.itemAriaNoCount", { label })
              : t("folders.itemAria", { label, count })}
            onClick={() => onFolderChange(item.folder)}
          >
            <item.icon className="size-4 shrink-0" />
            {!collapsed && (
              <>
                <span>{label}</span>
                <span className="ml-auto text-xs opacity-70">{count ?? "—"}</span>
              </>
            )}
          </Button>
        );
        if (!collapsed) return button;
        return (
          <Tooltip key={item.folder}>
            <TooltipTrigger asChild>{button}</TooltipTrigger>
            <TooltipContent side="right" className="flex items-center gap-3">
              <span>{label}</span>
              {count !== null && <span className="text-muted-foreground">{count}</span>}
            </TooltipContent>
          </Tooltip>
        );
      })}
    </nav>
  );
}
