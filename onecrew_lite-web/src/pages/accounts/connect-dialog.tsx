import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { Loader2, Plus, RefreshCw } from "lucide-react";
import { useTranslation } from "react-i18next";

import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from "@/components/ui/dialog";
import { PLATFORM_LABEL } from "@/lib/creator";
import type { Platform } from "@/types";
import type { OperatorBrowserStatus } from "@/lib/operator-account";
import { browserConnectionGate, operatorBrowserStatusQueryOptions } from "./browser-environment";

import { CreateProfileWizard } from "./connect-wizard";
import { TgLoginForm } from "./tg-login";

const CONNECT_DESCRIPTION_KEYS = {
  x: "connectDialog.descriptions.x",
  instagram: "connectDialog.descriptions.instagram",
  telegram: "connectDialog.descriptions.telegram",
} as const satisfies Record<Platform, string>;

/** The "连接" entry point — routes to the right login flow per platform. */
export function ConnectDialog({
  platform,
  onDone,
  browserStatus,
  browserStatusBusy = false,
  browserStatusError = null,
  onBrowserStatusRefresh,
}: {
  platform: Platform;
  onDone: () => void;
  browserStatus?: OperatorBrowserStatus | null;
  browserStatusBusy?: boolean;
  browserStatusError?: unknown;
  onBrowserStatusRefresh?: () => void;
}) {
  const { t } = useTranslation("accounts");
  const [open, setOpen] = useState(false);
  const shouldDetectBrowser = platform !== "telegram" && browserStatus === undefined;
  const detectedStatusQuery = useQuery(operatorBrowserStatusQueryOptions(shouldDetectBrowser));
  const label = PLATFORM_LABEL[platform];
  const effectiveStatus = browserStatus === undefined ? (detectedStatusQuery.data ?? null) : browserStatus;
  const effectiveError = browserStatus === undefined ? detectedStatusQuery.error : browserStatusError;
  const effectiveBusy = browserStatus === undefined ? detectedStatusQuery.isPending : browserStatusBusy;
  const gate = browserConnectionGate(effectiveStatus, effectiveBusy, t, effectiveError);
  const browserBlocked = platform !== "telegram" && gate.state !== "ready";

  const refreshBrowser = () => {
    if (browserStatus === undefined) void detectedStatusQuery.refetch();
    else onBrowserStatusRefresh?.();
  };
  // All flows are now stepped (X/IG browser wizard, TG 家宽→登录) → wider dialog.
  const finish = () => {
    setOpen(false);
    onDone();
  };
  return (
    <div className="flex flex-col items-end gap-1">
      <Dialog open={open} onOpenChange={setOpen}>
        <DialogTrigger asChild>
          <Button size="sm" disabled={browserBlocked}>
            {gate.state === "loading" && platform !== "telegram" ? <Loader2 className="size-4 animate-spin" /> : <Plus className="size-4" />}
            {gate.state === "loading" && platform !== "telegram"
              ? t("connectDialog.detecting")
              : t("connectDialog.connect")}
          </Button>
        </DialogTrigger>
        <DialogContent className="sm:max-w-lg">
          <DialogHeader>
            <DialogTitle>{t("connectDialog.title", { platform: label })}</DialogTitle>
            <DialogDescription>{t(CONNECT_DESCRIPTION_KEYS[platform])}</DialogDescription>
          </DialogHeader>
          {platform === "telegram" ? (
            <TgLoginForm key={open ? "open" : "closed"} onDone={finish} />
          ) : (
            <CreateProfileWizard key={open ? "open" : "closed"} platform={platform} browserStatus={effectiveStatus} onDone={finish} />
          )}
        </DialogContent>
      </Dialog>
      {platform !== "telegram" && (gate.state === "error" || gate.state === "blocked") && (
        <div className="flex max-w-72 items-center justify-end gap-1 text-right text-xs text-muted-foreground" role="status">
          <span>{gate.description}</span>
          <Button
            type="button"
            variant="ghost"
            size="sm"
            className="h-7 shrink-0 px-2"
            onClick={refreshBrowser}
            disabled={effectiveBusy || (browserStatus !== undefined && !onBrowserStatusRefresh)}
          >
            <RefreshCw className="size-3" />{t("browser.redetect")}
          </Button>
        </div>
      )}
    </div>
  );
}
