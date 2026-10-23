import { AppWindow, Loader2, RefreshCw } from "lucide-react";
import { useTranslation } from "react-i18next";

import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import type { OperatorBrowserStatus } from "@/lib/operator-account";

import { browserConnectionGate } from "./browser-environment";

export function BrowserEnvironmentStatus({
  status,
  busy,
  error,
  onRefresh,
}: {
  status: OperatorBrowserStatus | null;
  busy: boolean;
  error: unknown;
  onRefresh: () => void;
}) {
  const { t } = useTranslation("accounts");
  const gate = browserConnectionGate(status, busy, t, error);
  if (gate.state === "ready") return null;

  return (
    <Alert variant="warning">
      <AppWindow />
      <AlertTitle>{gate.title}</AlertTitle>
      <AlertDescription className="flex w-full items-center justify-between gap-3">
        <div>
          <p>{gate.description}</p>
          {gate.diagnostic && (
            <p className="mt-1 font-mono text-xs">
              {t("browser.diagnosticCode", { code: gate.diagnostic })}
            </p>
          )}
        </div>
        <Button className="shrink-0" variant="outline" size="sm" onClick={onRefresh} disabled={busy}>
          {busy ? <Loader2 className="size-4 animate-spin" /> : <RefreshCw className="size-4" />}
          {t("browser.redetect")}
        </Button>
      </AlertDescription>
    </Alert>
  );
}
