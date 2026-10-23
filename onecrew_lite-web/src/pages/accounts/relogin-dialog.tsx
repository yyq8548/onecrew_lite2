import { useEffect, useRef, useState } from "react";
import { BadgeCheck, DownloadCloud, ExternalLink, Loader2 } from "lucide-react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";

import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { type Account } from "@/types";
import {
  beginOperatorConnection,
  cleanupPendingOperatorConnection,
  finalizeOperatorConnection,
  prepareOperatorConnection,
  type OperatorBrowserStatus,
} from "@/lib/operator-account";
import { browserConnectionReady, browserOperationErrorMessage } from "./browser-environment";

import { Stepper } from "./shared";
import { BrowserCookieImportAlert } from "./browser-cookie-import-alert";

/** Re-login into an existing profile as ONE guided card flow (登录 → 导入).
 * The runtime opens an isolated copy and replaces the persistent profile only
 * after the extracted identity matches the selected account. */
export function ReloginDialog({
  account,
  open,
  onOpenChange,
  onDone,
  browserStatus,
}: {
  account: Account;
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onDone: () => void;
  browserStatus: OperatorBrowserStatus | null;
}) {
  const { t } = useTranslation("accounts");
  const reloginSteps = [t("relogin.steps.login"), t("relogin.steps.import")] as const;
  const [step, setStep] = useState(0);
  const [flowId, setFlowId] = useState<string | null>(null);
  const [opening, setOpening] = useState(false);
  const [extracting, setExtracting] = useState(false);
  const [done, setDone] = useState(false);
  const [completionNotice, setCompletionNotice] = useState<string | null>(null);
  const flowRef = useRef<string | null>(null);
  const completedRef = useRef(false);
  const browserReady = browserConnectionReady(browserStatus);

  useEffect(() => {
    flowRef.current = flowId;
  }, [flowId]);

  useEffect(
    () => () => {
      void cleanupPendingOperatorConnection(flowRef.current, () => completedRef.current);
    },
    [],
  );

  async function openChrome() {
    if (!browserReady) {
      toast.error(t("relogin.toasts.unsupported"));
      return;
    }
    setOpening(true);
    try {
      const prepared = await prepareOperatorConnection({
        platform: account.platform,
        intent: "reconnect",
        method: "browser",
        operator_account_id: account.id,
      });
      flowRef.current = prepared.flow_id;
      setFlowId(prepared.flow_id);
      const status = await beginOperatorConnection(prepared.flow_id);
      if (status.phase !== "awaiting_user_in_app") {
        throw new Error(status.error_code ?? "browser_connection_failed");
      }
      setCompletionNotice(status.browser_capability?.completion_notice?.message ?? null);
      toast(t("relogin.toasts.browserOpened"), {
        description: t("relogin.toasts.browserOpenedDescription", { username: account.username }),
      });
    } catch (error) {
      toast.error(t("relogin.toasts.openFailed"), {
        description: browserOperationErrorMessage(
          error,
          t,
          t("relogin.toasts.openFailedDescription"),
        ),
      });
    } finally {
      setOpening(false);
    }
  }

  async function extract() {
    if (!flowId) return;
    setExtracting(true);
    try {
      const status = await finalizeOperatorConnection(flowId);
      if (status.phase !== "completed") {
        throw new Error(status.error_code ?? "browser_connection_failed");
      }
      completedRef.current = true;
      setDone(true);
      toast.success(t("relogin.toasts.updated", { username: account.username }), {
        description: t("relogin.toasts.updatedDescription"),
      });
    } catch (error) {
      toast.error(t("relogin.toasts.extractFailed"), {
        description: browserOperationErrorMessage(
          error,
          t,
          t("relogin.toasts.extractFailedDescription"),
        ),
      });
    } finally {
      setExtracting(false);
    }
  }

  const canNext = [flowId !== null, done][step];
  const nextLabel = [t("relogin.actions.loginCompleted"), t("relogin.actions.finish")][step];
  function next() {
    if (step < reloginSteps.length - 1) setStep((s) => s + 1);
    else {
      onOpenChange(false);
      onDone();
    }
  }

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-lg">
        <DialogHeader>
          <DialogTitle>{t("relogin.title")}</DialogTitle>
          <DialogDescription>
            {t("relogin.description")}
          </DialogDescription>
        </DialogHeader>
        <div className="space-y-5">
          <Stepper step={step} steps={reloginSteps} />
          <div className="min-h-[132px] space-y-4">
            {step === 0 && (
              <div className="space-y-3">
                <p className="text-sm text-muted-foreground">
                  {t("relogin.loginDescription")}
                </p>
                <Button type="button" variant={flowId ? "outline" : "default"} onClick={openChrome} disabled={!browserReady || opening || Boolean(flowId)}>
                  {opening ? <Loader2 className="size-4 animate-spin" /> : <ExternalLink className="size-4" />}
                  {flowId ? t("relogin.actions.chromeOpened") : t("relogin.actions.openChrome")}
                </Button>
                {flowId && (
                  <div className="rounded-lg border bg-muted/40 p-3 text-xs text-muted-foreground">
                    {completionNotice ?? t("relogin.browserOpenedFallback")}
                  </div>
                )}
              </div>
            )}
            {step === 1 && (
              <div className="space-y-3">
                <p className="text-sm text-muted-foreground">
                  {t("relogin.importDescription")}
                </p>
                <BrowserCookieImportAlert status={browserStatus} />
                <Button type="button" variant="outline" disabled={extracting || done} onClick={extract}>
                  {extracting ? (
                    <Loader2 className="size-4 animate-spin" />
                  ) : (
                    <DownloadCloud className="size-4" />
                  )}
                  {done ? t("relogin.actions.extracted") : t("relogin.actions.extractCookie")}
                </Button>
                {done && (
                  <div className="rounded-lg border p-3">
                    <div className="flex items-center gap-1.5 text-sm text-[var(--tone-success-fg)]">
                      <BadgeCheck className="size-4" /> {t("relogin.updated", { username: account.username })}
                    </div>
                  </div>
                )}
              </div>
            )}
          </div>
          <DialogFooter className="sm:justify-between">
            {step > 0 ? (
              <Button type="button" variant="ghost" onClick={() => setStep((s) => s - 1)}>
                {t("common.actions.previous")}
              </Button>
            ) : (
              <span />
            )}
            <Button type="button" disabled={!canNext} onClick={next}>
              {nextLabel}
            </Button>
          </DialogFooter>
        </div>
      </DialogContent>
    </Dialog>
  );
}
