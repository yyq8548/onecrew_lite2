import { useState } from "react";
import { invoke } from "@/lib/local-runtime";
import { type ColumnDef } from "@tanstack/react-table";
import type { TFunction } from "i18next";
import {
  BadgeCheck,
  Check,
  ExternalLink,
  Fingerprint,
  KeyRound,
  MoreHorizontal,
  ShieldCheck,
  Trash2,
} from "lucide-react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";

import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogClose,
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
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { DataTableColumnHeader } from "@/components/data-table";
import { cn } from "@/lib/utils";
import { tone } from "@/lib/ui";
import { type Account, type Platform } from "@/types";
import type { OperatorBrowserStatus } from "@/lib/operator-account";
import { browserConnectionReady } from "./browser-environment";

import { PlatformTile } from "./shared";
import { ProxyDialog } from "./proxy";
import { ReloginDialog } from "./relogin-dialog";
import { XChatPasscodeDialog } from "./x-chat-passcode-dialog";
import { xChatPasscodeStatusView } from "./x-chat-passcode-state";

const sourceLabel = (p: Platform, t: TFunction<"accounts">) =>
  p === "telegram" ? t("accountTable.source.telegram") : t("accountTable.source.browser");

/** Format the backend `updated_at` (epoch seconds) as local `YYYY-MM-DD HH:mm:ss`. */
function fmtTime(value: string): string {
  const d = new Date(Number(value) * 1000);
  const p = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())} ${p(d.getHours())}:${p(d.getMinutes())}:${p(d.getSeconds())}`;
}

/** Row actions: whoami re-validate + re-login + proxy config + disconnect. */
function RowActions({ account, onChanged, browserStatus }: { account: Account; onChanged: () => void; browserStatus: OperatorBrowserStatus | null }) {
  const { t } = useTranslation("accounts");
  const [proxyOpen, setProxyOpen] = useState(false);
  const [reloginOpen, setReloginOpen] = useState(false);
  const [xChatOpen, setXChatOpen] = useState(false);
  const [disconnectOpen, setDisconnectOpen] = useState(false);
  const isBrowser = account.platform === "x" || account.platform === "instagram";
  const browserReady = browserConnectionReady(browserStatus);

  async function refresh() {
    try {
      await invoke("refresh_profile", { profileId: account.id });
      toast.success(t("accountTable.toasts.verified", { username: account.username }), {
        description: t("accountTable.toasts.verifiedDescription"),
      });
      onChanged();
    } catch (error) {
      toast.error(t("accountTable.toasts.verifyFailed"), { description: String(error) });
    }
  }

  async function disconnect() {
    try {
      await invoke("onecrew_operator_account_disconnect", {
        input: {
          operator_account_id: account.id,
          confirmed: true,
          idempotency_key: `accounts-disconnect:${account.id}`,
        },
      });
      toast(t("accountTable.toasts.disconnected", { username: account.username }));
      onChanged();
    } catch (error) {
      toast.error(t("accountTable.toasts.disconnectFailed"), { description: String(error) });
    }
  }

  return (
    <div className="flex justify-end">
      <DropdownMenu>
        <DropdownMenuTrigger asChild>
          <Button
            variant="ghost"
            size="icon"
            className="size-8"
            aria-label={t("accountTable.actions.menuAria", { username: account.username })}
          >
            <MoreHorizontal className="size-4" />
          </Button>
        </DropdownMenuTrigger>
        <DropdownMenuContent align="end" className="w-52">
          <DropdownMenuItem onClick={refresh}>
            <BadgeCheck className="size-4" /> {t("accountTable.actions.verify")}
          </DropdownMenuItem>
          {/* Re-login + re-extract are ONE guided flow (登录→导入), not two actions. */}
          {isBrowser && (
            <DropdownMenuItem disabled={!browserReady} onClick={() => setReloginOpen(true)}>
              <ExternalLink className="size-4" /> {t("accountTable.actions.relogin")}
            </DropdownMenuItem>
          )}
          {account.platform === "x" && (
            <DropdownMenuItem onClick={() => setXChatOpen(true)}>
              <KeyRound className="size-4" /> {t("accountTable.actions.xChat")}
            </DropdownMenuItem>
          )}
          <DropdownMenuItem onClick={() => setProxyOpen(true)}>
            <ShieldCheck className="size-4" /> {t("accountTable.actions.proxy")}
          </DropdownMenuItem>
          <DropdownMenuSeparator />
          <DropdownMenuItem className="text-destructive focus:text-destructive" onSelect={() => setDisconnectOpen(true)}>
            <Trash2 className="size-4" /> {t("accountTable.actions.disconnect")}
          </DropdownMenuItem>
        </DropdownMenuContent>
      </DropdownMenu>
      <ProxyDialog account={account} open={proxyOpen} onOpenChange={setProxyOpen} onChanged={onChanged} />
      {isBrowser && (
        <ReloginDialog
          key={reloginOpen ? "open" : "closed"}
          account={account}
          open={reloginOpen}
          onOpenChange={setReloginOpen}
          onDone={onChanged}
          browserStatus={browserStatus}
        />
      )}
      {account.platform === "x" && (
        <XChatPasscodeDialog
          key={xChatOpen ? `open-${account.x_chat_passcode_status}` : "closed"}
          account={account}
          open={xChatOpen}
          onOpenChange={setXChatOpen}
          onChanged={onChanged}
        />
      )}
      <Dialog open={disconnectOpen} onOpenChange={setDisconnectOpen}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>{t("accountTable.disconnect.title", { username: account.username })}</DialogTitle>
            <DialogDescription>
              {t("accountTable.disconnect.description")}
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <DialogClose asChild><Button variant="outline">{t("common.actions.cancel")}</Button></DialogClose>
            <Button variant="destructive" onClick={() => void disconnect()}>
              {t("accountTable.disconnect.confirm")}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  );
}

export function accountColumns(
  platform: Platform,
  onChanged: () => void,
  browserStatus: OperatorBrowserStatus | null,
  t: TFunction<"accounts">,
): ColumnDef<Account>[] {
  const isBrowser = platform === "x" || platform === "instagram";
  const secondColumn: ColumnDef<Account> = isBrowser
    ? {
        id: "fingerprint",
        header: t("accountTable.columns.fingerprint"),
        enableSorting: false,
        cell: ({ row }) =>
          row.original.fingerprint_seed ? (
            <span className="inline-flex items-center gap-1.5 text-xs text-muted-foreground">
              <Fingerprint className="size-3.5 shrink-0" /> {row.original.fingerprint_seed}
            </span>
          ) : (
            <span className="text-xs text-muted-foreground">—</span>
          ),
      }
    : {
        id: "source",
        header: t("accountTable.columns.source"),
        enableSorting: false,
        cell: () => <span className="text-muted-foreground">{sourceLabel(platform, t)}</span>,
      };
  return [
    {
      accessorKey: "username",
      header: ({ column }) => <DataTableColumnHeader column={column} title={t("accountTable.columns.account")} />,
      enableHiding: false,
      cell: ({ row }) => (
        <div className="flex items-center gap-2.5">
          <PlatformTile platform={platform} className="size-8" />
          <div className="min-w-0 leading-tight">
            <div className="font-medium">@{row.original.username}</div>
            {row.original.profile_name && (
              <div className="text-xs text-muted-foreground">
                {t("accountTable.profile", { name: row.original.profile_name })}
              </div>
            )}
          </div>
        </div>
      ),
    },
    secondColumn,
    {
      id: "status",
      header: t("accountTable.columns.status"),
      enableSorting: false,
      cell: ({ row }) => {
        const xChat = xChatPasscodeStatusView(row.original.x_chat_passcode_status, t);
        const xChatTone = xChat.tone === "success"
          ? tone.success
          : xChat.tone === "warning"
            ? tone.warning
            : xChat.tone === "destructive"
              ? "bg-destructive text-white"
              : tone.muted;
        return (
          <div className="flex flex-wrap gap-1.5">
            <Badge className={cn("border-transparent", tone.success)}>
              <Check className="mr-1 size-3" /> {t("accountTable.ready")}
            </Badge>
            {platform === "x" && (
              <Badge variant="secondary" className={cn("border-transparent", xChatTone)}>
                {xChat.label}
              </Badge>
            )}
          </div>
        );
      },
    },
    {
      id: "proxy",
      header: t("accountTable.columns.proxy"),
      enableSorting: false,
      cell: ({ row }) => {
        const { proxy, proxy_kind, proxy_egress } = row.original;
        if (!proxy && !proxy_egress) {
          return <span className="text-xs text-muted-foreground">{t("accountTable.direct")}</span>;
        }
        const kind = proxy_kind ? proxy_kind.toUpperCase() : null;
        return (
          <div className="leading-tight">
            <span className="inline-flex items-center gap-1 text-xs font-medium text-[var(--tone-success-fg)]">
              <ShieldCheck className="size-3.5" /> {t("accountTable.residential")}
              {kind ? ` · ${kind}` : ""} · {proxy_egress ?? proxy}
            </span>
            {/* Real egress is the exit; show the gateway you typed as secondary. */}
            {proxy_egress && proxy && (
              <div className="text-[11px] text-muted-foreground">
                {t("accountTable.via", { proxy })}
              </div>
            )}
          </div>
        );
      },
    },
    {
      accessorKey: "updated_at",
      meta: { label: t("accountTable.columns.connectedAt") },
      header: ({ column }) => <DataTableColumnHeader column={column} title={t("accountTable.columns.connectedAt")} />,
      cell: ({ row }) => (
        <span className="whitespace-nowrap text-muted-foreground tabular-nums">
          {fmtTime(row.original.updated_at)}
        </span>
      ),
    },
    {
      id: "actions",
      enableHiding: false,
        cell: ({ row }) => <RowActions account={row.original} onChanged={onChanged} browserStatus={browserStatus} />,
    },
  ];
}
