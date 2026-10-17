import { useEffect, useRef, useState } from "react";
import { useMutation, useQuery } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { managedBrowserBinding, managedBrowserConnection, type ManagedBrowserConnection } from "@onecrew/runtime-contracts/browser-connection";
import { Alert, AlertDescription } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import { invoke, unifiedAccountSessionKey } from "@/lib/local-runtime";

async function command(action: "prepare" | "begin" | "finalize" | "cancel" | "reopen", input: Record<string, unknown>) {
  return managedBrowserConnection.parse(await invoke<unknown>(`onecrew_managed_browser_connection_${action}`, input));
}

export function ManagedBrowserSetup({ contextId, onConnected }: { contextId: string; onConnected: (contextId: string) => void }) {
  const { t } = useTranslation("shell");
  const [flow, setFlow] = useState<ManagedBrowserConnection>();
  const current = useRef<ManagedBrowserConnection | undefined>(undefined);
  const mounted = useRef(true);
  const [cleanupError, setCleanupError] = useState<unknown>();
  const [reopenContext, setReopenContext] = useState<string>();
  const bindings = useQuery({ queryKey: ["managed-browser-bindings", unifiedAccountSessionKey()], retry: false,
    queryFn: async () => managedBrowserBinding.array().parse(await invoke<unknown>("onecrew_managed_browser_bindings", {})) });
  const update = (value: ManagedBrowserConnection) => { current.current = value; if (mounted.current) setFlow(value); };
  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
      const pending = current.current;
      if (pending && pending.phase !== "completed" && pending.phase !== "canceled") {
        void command("cancel", { flowId: pending.flow_id, confirmed: true }).then(update, error => {
          console.error("Managed browser cleanup failed", error);
        });
      }
    };
  }, []);
  const action = useMutation({
    retry: false,
    mutationFn: async (kind: "begin" | "finalize" | "cancel" | { bindingId: string; contextId: string }) => {
      setCleanupError(undefined);
      if (typeof kind === "object") {
        setReopenContext(kind.contextId);
        const reopened = await command("reopen", { bindingId: kind.bindingId, confirmed: true });
        if (!mounted.current) return command("cancel", { flowId: reopened.flow_id, confirmed: true });
        return reopened;
      }
      if (kind === "begin") {
        setReopenContext(undefined);
        const prepared = await command("prepare", {});
        update(prepared);
        if (!mounted.current) return command("cancel", { flowId: prepared.flow_id, confirmed: true });
        return command("begin", { flowId: prepared.flow_id, confirmed: true });
      }
      const selected = current.current;
      if (!selected) throw new Error("browser.connection_missing");
      return command(kind, { flowId: selected.flow_id, confirmed: true, ...(kind === "finalize" ? { contextId: reopenContext ?? contextId } : {}) });
    },
    onSuccess: value => {
      update(value);
      if (value.phase === "completed") void bindings.refetch();
      if (mounted.current && value.phase === "completed" && value.context_id) onConnected(value.context_id);
    },
    onError: error => setCleanupError(error),
  });
  const active = flow && !["completed", "canceled", "failed"].includes(flow.phase);
  const errorCode = cleanupError instanceof Error ? cleanupError.message : cleanupError;
  const errorMessage = errorCode === "browser.managed_challenge_tab_required" ? t("opencli.managedSelectConnectionTab")
    : errorCode === "browser.managed_challenge_mismatch" ? t("opencli.managedChallengeMismatch")
    : errorCode === "browser.owned_process_not_running" || errorCode === "browser.connection_expired" ? t("opencli.managedReconnectRequired")
    : `${t("opencli.managedFailed")} ${String(errorCode)}`;
  return <section className="space-y-3 rounded-md border p-3" aria-label={t("opencli.managedTitle")}>
    <p className="text-sm font-medium">{t("opencli.managedTitle")}</p>
    <p className="text-sm text-muted-foreground">{t("opencli.managedDescription")}</p>
    <p className="text-sm text-muted-foreground">{t("opencli.managedRestartHelp")}</p>
    {!active && <Button size="sm" variant="outline" disabled={action.isPending} onClick={() => action.mutate("begin")}>{t("opencli.managedBegin")}</Button>}
    {!active && bindings.data && bindings.data.length > 0 && <div className="flex flex-wrap gap-2">{bindings.data.map(binding => <Button key={binding.binding_id} size="sm" variant="outline" disabled={action.isPending} onClick={() => action.mutate({ bindingId: binding.binding_id, contextId: binding.context_id })}>{t("opencli.managedReopen", { profile: binding.context_id })}</Button>)}</div>}
    {bindings.isError && <p className="text-sm" role="alert">{t("opencli.managedHistoryFailed")}</p>}
    {active && <>
      <p className="text-sm" role="status">{t("opencli.managedInstructions")}</p>
      <div className="flex flex-wrap gap-2">
        <Button size="sm" disabled={!(reopenContext ?? contextId) || action.isPending || flow.phase !== "awaiting_user_in_app"} onClick={() => action.mutate("finalize")}>{t("opencli.managedFinalize")}</Button>
        <Button size="sm" variant="outline" disabled={action.isPending} onClick={() => action.mutate("cancel")}>{t("opencli.managedCancel")}</Button>
      </div>
    </>}
    {flow?.phase === "completed" && <p className="text-sm" role="status">{t("opencli.managedConnected")}</p>}
    {cleanupError !== undefined && <Alert variant="destructive"><AlertDescription>{errorMessage}</AlertDescription></Alert>}
  </section>;
}
