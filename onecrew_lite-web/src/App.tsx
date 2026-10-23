import { useCallback, useEffect, useRef, useState, useSyncExternalStore, type CSSProperties, type ReactNode } from "react";
import { AlertTriangle, Loader2 } from "lucide-react";
import { useTranslation } from "react-i18next";

import { SidebarInset, SidebarProvider, SidebarTrigger } from "@/components/ui/sidebar";
import { AppSidebar, type View } from "@/components/AppSidebar";
import { Leads } from "@/pages/Leads";
import { Accounts } from "@/pages/Accounts";
import { Connect } from "@/pages/Connect";
import { Mail, type MailNavigationGuardHandle } from "@/pages/Mail";
import { Toaster } from "@/components/ui/sonner";
import { requestGuardedMailNavigation } from "@/components/mail/navigation-guard";
import { sidebarLayout } from "@/lib/sidebar-layout";
import { Button } from "@/components/ui/button";
import { Alert, AlertDescription } from "@/components/ui/alert";
import { Card, CardContent, CardFooter, CardHeader } from "@/components/ui/card";
import { PairingGateController } from "@/components/pairing-gate-controller";
import { inspectLocalData, failureMessage, type DataStartupStatus } from "@/lib/local-data";
import { connectLocalRuntime, isRuntimeAvailable, runtimeConnectionState, subscribeRuntimeAvailability } from "@/lib/local-runtime";
import { CanvasPortalProvider } from "@/lib/canvas-portal";
import { RuntimeChangeBridge } from "@/lib/runtime-change-bridge";

type AppMetaTranslationKey =
  | `views.${View}.title`
  | `views.${View}.subtitle`;

const META: Record<View, { titleKey: AppMetaTranslationKey; subtitleKey: AppMetaTranslationKey }> = {
  free: { titleKey: "views.free.title", subtitleKey: "views.free.subtitle" },
  "free-search": { titleKey: "views.free-search.title", subtitleKey: "views.free-search.subtitle" },
  leads: { titleKey: "views.leads.title", subtitleKey: "views.leads.subtitle" },
  mail: { titleKey: "views.mail.title", subtitleKey: "views.mail.subtitle" },
  accounts: { titleKey: "views.accounts.title", subtitleKey: "views.accounts.subtitle" },
  connect: { titleKey: "views.connect.title", subtitleKey: "views.connect.subtitle" },
};

export default function App() {
  const ready = useSyncExternalStore(subscribeRuntimeAvailability, isRuntimeAvailable);
  useEffect(() => {
    const refresh = () => { void connectLocalRuntime(true).then(() => {}, () => {}); };
    const visible = () => { if (document.visibilityState !== "hidden") refresh(); };
    const timer = setInterval(() => {
      const state = runtimeConnectionState();
      if (state !== "session_required" && state !== "account_changed") visible();
    }, 30_000);
    window.addEventListener("focus", visible);
    window.addEventListener("hashchange", refresh);
    document.addEventListener("visibilitychange", visible);
    return () => {
      clearInterval(timer);
      window.removeEventListener("focus", visible);
      window.removeEventListener("hashchange", refresh);
      document.removeEventListener("visibilitychange", visible);
    };
  }, []);
  if (!ready) return <DisconnectedCard />;
  return (
    <LocalDataGate>
      <PairingGateController>
        {(ownerKey) => <ActivatedApp key={ownerKey} />}
      </PairingGateController>
    </LocalDataGate>
  );
}

export function DisconnectedCard() {
  const { t } = useTranslation("shell");
  const { t: tCommon } = useTranslation("common");
  const [copied, setCopied] = useState<"idle" | "done" | "unavailable">("idle");
  const state = useSyncExternalStore(subscribeRuntimeAvailability, runtimeConnectionState, runtimeConnectionState);
  const [retrying, setRetrying] = useState(false);
  const openRequest = t(state === "unreachable" ? "repair.offlineRequest" : "service.openRequest");
  const copy = useCallback(() => {
    const clipboard = navigator.clipboard;
    if (!clipboard) {
      setCopied("unavailable");
      return;
    }
    void clipboard
      .writeText(openRequest)
      .then(() => setCopied("done"))
      .catch(() => setCopied("unavailable"));
  }, [openRequest]);
  return (
    <main className="grid min-h-svh place-items-center bg-background p-6">
      <Card className="w-full max-w-md">
        <CardHeader>{t("service.disconnectedTitle")}</CardHeader>
        <CardContent className="space-y-3">
          <p className="text-sm text-muted-foreground">{t(state === "session_required" || state === "account_changed" ? "service.sessionRequiredDescription" : "service.disconnectedDescription")}</p>
          <p className="text-sm text-muted-foreground">{t("service.openWorkspaceHelp")}</p>
          <p className="block rounded bg-muted p-3 text-sm">{openRequest}</p>
          {copied !== "idle" && (
            <p className="text-sm text-muted-foreground" role="status">
              {copied === "done" ? t("service.copied") : tCommon("errors.clipboardUnavailable")}
            </p>
          )}
        </CardContent>
        <CardFooter className="gap-2">
          <Button onClick={copy}>{t(state === "unreachable" ? "repair.copy" : "service.copyCommand")}</Button>
          <Button variant="outline" disabled={retrying} onClick={() => {
            setRetrying(true);
            void connectLocalRuntime(true).then(() => setRetrying(false), () => setRetrying(false));
          }}>
            {t("service.retry")}
          </Button>
        </CardFooter>
      </Card>
    </main>
  );
}

function ActivatedApp() {
  return (
    <>
      <RuntimeChangeBridge />
      <WorkspaceApp />
    </>
  );
}

function LocalDataGate({ children }: { children: ReactNode }) {
  const { t } = useTranslation("shell");
  const [status, setStatus] = useState<DataStartupStatus | null>(null);
  const [requestFailed, setRequestFailed] = useState(false);
  const inspect = useCallback(async () => {
    try {
      const next = await inspectLocalData();
      setStatus(next);
      setRequestFailed(false);
      return next;
    } catch {
      setRequestFailed(true);
      return null;
    }
  }, []);
  useEffect(() => {
    let active = true;
    let timer: ReturnType<typeof setTimeout> | undefined;
    const observe = async () => {
      const next = await inspect();
      if (active && next && next.phase !== "ready" && next.phase !== "failed") {
        timer = setTimeout(() => { void observe(); }, 500);
      }
    };
    void observe();
    return () => { active = false; clearTimeout(timer); };
  }, [inspect]);
  if (status?.phase === "ready") return <>{children}</>;
  if (!requestFailed && status?.phase !== "failed") return (
    <main className="grid min-h-svh place-items-center bg-background p-6">
      <div className="flex max-w-md flex-col items-center gap-4 text-center" role="status" aria-live="polite">
        <Loader2 className="size-6 animate-spin motion-reduce:animate-none" />
        <p>{t("service.preparing")}</p>
        <p className="text-sm text-muted-foreground">{t("service.preparingDescription")}</p>
      </div>
    </main>
  );
  const failure = status?.failure;
  const preflight = status?.postflight ?? status?.preflight;
  const kind = failure?.kind ?? preflight?.failure_kind;
  const message = kind ? failureMessage(kind, failure?.storage_upgraded === true) : t("service.startupFailedDescription");
  const supportCode = status?.diagnostic_code ?? failure?.support_code ?? preflight?.support_code;
  const recoveryPoint = [...(failure?.results ?? []), ...(status?.upgrade?.results ?? [])].some((result) => result.recovery_point_created);
  return (
    <main className="grid min-h-svh place-items-center bg-background p-6">
      <Card className="w-full max-w-lg">
        <CardHeader className="flex-row items-center gap-2"><AlertTriangle className="size-5" />{t("service.startupFailed")}</CardHeader>
        <CardContent className="space-y-4">
          <p className="text-sm">{message}</p>
          {supportCode && <code className="block break-all rounded bg-muted p-3 text-xs">{supportCode}</code>}
          {recoveryPoint && <Alert><AlertDescription>{t("service.backupPreserved")}</AlertDescription></Alert>}
          <p className="text-sm text-muted-foreground">{t("service.businessBlocked")}</p>
        </CardContent>
        <CardFooter><Button variant="outline" onClick={() => { void inspect(); }}>{t("service.inspectStatus")}</Button></CardFooter>
      </Card>
    </main>
  );
}

function WorkspaceApp() {
  const { t } = useTranslation("shell");
  const connection = useSyncExternalStore(subscribeRuntimeAvailability, runtimeConnectionState);
  const [view, setView] = useState<View>("free");
  const [searchOpened, setSearchOpened] = useState(false);
  const [sidebarOpen, setSidebarOpen] = useState(true);
  const mailNavigationGuard = useRef<MailNavigationGuardHandle>(null);
  const requestNavigation = (
    next: View,
  ) => {
    const continueNavigation = () => {
      if (next === "free-search") setSearchOpened(true);
      setView(next);
    };
    if (view === "mail" && next !== "mail") {
      requestGuardedMailNavigation(mailNavigationGuard.current, continueNavigation);
      return;
    }
    continueNavigation();
  };
  const navigate = (next: View) => requestNavigation(next);

  const meta = META[view];
  // 模态的 Portal 目标。回调 ref 而不是 useRef:容器必须在任何 sheet 打开
  // 之前就已经存在,而 useRef 的赋值不会触发重渲染,消费方永远读到 null。
  const [canvas, setCanvas] = useState<HTMLDivElement | null>(null);

  return (
    <div
      className="flex h-svh w-full overflow-hidden bg-background"
    >
    <SidebarProvider
      ref={setCanvas}
      className="h-svh min-w-0 flex-1 overflow-hidden bg-background"
      open={sidebarOpen}
      onOpenChange={setSidebarOpen}
      style={
        {
          "--sidebar-width": sidebarLayout.expandedWidth,
          "--sidebar-width-icon": sidebarLayout.iconWidth,
        } as CSSProperties
      }
    >
      <CanvasPortalProvider value={canvas}>
      <AppSidebar view={view} onNavigate={navigate} />
      <SidebarInset
        className="flex min-w-0 flex-col overflow-hidden"
      >
        <header
          className="flex h-14 shrink-0 items-center justify-between gap-4 border-b bg-card px-5"
        >
          <div className="flex min-w-0 items-center gap-2">
            {/* Below md the sidebar is a closed Sheet whose own toggle lives
                inside it, so the header carries the only way to open it. */}
            <SidebarTrigger className="-ml-1 md:hidden" />
            <div className="min-w-0">
              <div className="text-sm font-semibold leading-tight">{t(meta.titleKey)}</div>
              <div className="truncate text-xs text-muted-foreground">{t(meta.subtitleKey)}</div>
            </div>
          </div>
          <div className="flex shrink-0 items-center gap-2 text-[11px] text-muted-foreground">
            <span className={`size-1.5 rounded-full ${connection === "connected" ? "bg-[var(--tone-success-solid)]" : "bg-amber-500"}`} />
            <span role="status">{t(connection === "connected" ? "service.connected" : "service.reconnecting")}</span>
          </div>
        </header>
        <div
          className={
            view === "mail" || view === "connect" || view === "leads"
              ? "min-h-0 flex-1 overflow-hidden"
              : "min-h-0 flex-1 overflow-y-auto p-4 md:p-6"
          }
        >
          {view === "leads" && <Leads />}
          {view === "mail" && (
            <Mail
              ref={mailNavigationGuard}
              onManageMailboxes={() => requestNavigation("accounts")}
            />
          )}
          {view === "accounts" && <Accounts onConnections={() => requestNavigation("free")} />}
          {view === "connect" && <Connect />}
          {view === "free" && <Connect freeEdition onFreeNavigate={() => requestNavigation("free-search")} />}
          {searchOpened && <div hidden={view !== "free-search"} className="h-full">
            <Connect freeEdition freePage="search" onFreeNavigate={() => requestNavigation("free")} />
          </div>}
        </div>
      </SidebarInset>
      <Toaster position="top-center" />
      </CanvasPortalProvider>
    </SidebarProvider>
    </div>
  );
}
