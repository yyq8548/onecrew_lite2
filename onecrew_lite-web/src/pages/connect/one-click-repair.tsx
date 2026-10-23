import { useEffect, useRef, useState, type ReactNode } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { CheckCircle2, ChevronDown, Loader2, Wrench, X } from "lucide-react";
import { repairStatus, type RepairStatus } from "@onecrew/runtime-contracts/repair";
import { stringifyApplicationJson } from "@onecrew/runtime-contracts/json";
import { Button } from "@/components/ui/button";
import { connectLocalRuntime, invoke } from "@/lib/local-runtime";
import { repairClientContext } from "@/lib/repair-context";
import { SERVICE_STATUS_QUERY_KEY } from "@/lib/connect";

const isActive = (value: RepairStatus | null | undefined) => value != null && ["checking", "awaiting_model", "repairing"].includes(value.state);
export function OneClickRepair({ page, credentialId, children }: { page: "connections" | "search"; credentialId: string | null; children: ReactNode }) {
  const { t } = useTranslation("shell"), queryClient = useQueryClient();
  const [open, setOpen] = useState(false), [copied, setCopied] = useState(false), [copyFailed, setCopyFailed] = useState(false);
  const [selectionFailed, setSelectionFailed] = useState(false);
  const [elapsed, setElapsed] = useState(0);
  const appliedDraft = useRef<string | null>(null);
  const key = ["onecrew-repair", credentialId];
  const status = useQuery({ queryKey: key, queryFn: async () => repairStatus.nullable().parse(await invoke("onecrew_repair_status")), enabled: open,
    retry: false, refetchInterval: query => open && !query.state.error && isActive(query.state.data) ? 1500 : false });
  const begin = useMutation({ retry: false, mutationFn: async () => {
    await connectLocalRuntime(true);
    return repairStatus.parse(await invoke("onecrew_repair_begin", { request_id: crypto.randomUUID(), page, ...await repairClientContext() }));
  }, onSuccess: result => { queryClient.setQueryData(key, result); void queryClient.invalidateQueries({ queryKey: key }); } });
  const job = status.data;
  useEffect(() => {
    if (!open || !job || !isActive(job)) return;
    setElapsed(Math.floor((Date.now() - job.created_at) / 1000));
    const timer = window.setInterval(() => setElapsed(Math.floor((Date.now() - job.created_at) / 1000)), 1000);
    return () => window.clearInterval(timer);
  }, [open, job?.id, job?.state]);
  useEffect(() => {
    if (!job?.draft_id || appliedDraft.current === job.draft_id) return;
    const id = job.draft_id;
    appliedDraft.current = id;
    window.dispatchEvent(new CustomEvent("onecrew:repair-plan-ready", { detail: id }));
    void Promise.resolve().then(() => window.localStorage.setItem("onecrew-opencli-pilot-campaign", id)).then(() => setSelectionFailed(false), () => setSelectionFailed(true));
    void queryClient.invalidateQueries({ queryKey: ["opencli-campaign", id] });
  }, [job?.draft_id, queryClient]);
  useEffect(() => {
    if (!job || isActive(job)) return;
    void queryClient.invalidateQueries({ queryKey: SERVICE_STATUS_QUERY_KEY });
    void queryClient.invalidateQueries({ queryKey: ["opencli-setup"] });
    void queryClient.invalidateQueries({ queryKey: ["discovery-library"] });
  }, [job?.id, job?.state, queryClient]);
  const waiting = !!job && job.received_at === null && isActive(job);
  const busy = begin.isPending || isActive(job);
  return <>
    <div className="flex flex-wrap items-center justify-end gap-2">
      {children}
      <Button size="sm" variant="outline" aria-expanded={open} aria-controls="onecrew-repair-panel" onClick={() => {
        setOpen(true); setCopied(false); setCopyFailed(false);
        if (!busy) begin.mutate();
      }}>{busy ? <Loader2 className="size-4 animate-spin" aria-hidden /> : <Wrench className="size-4" aria-hidden />}{t("repair.button")}</Button>
    </div>
    {open && <section id="onecrew-repair-panel" aria-label={t("repair.button")} className="space-y-4 rounded-lg border bg-background p-4 text-sm">
      <div className="flex items-start justify-between gap-3">
        <div role="status" aria-live="polite" className="min-w-0 space-y-1">
          <p className="font-medium">{begin.isPending ? t("repair.states.checking") : job ? t(`repair.states.${job.state}`) : t("repair.button")}</p>
          {waiting && <p className="text-muted-foreground">{t(job.notified_at ? "repair.notified" : "repair.waiting")}</p>}
          {job?.received_at && isActive(job) ? <p className="text-muted-foreground">{t("repair.received")}</p> : null}
        </div>
        <Button size="icon-sm" variant="ghost" aria-label={t("repair.close")} onClick={() => setOpen(false)}><X className="size-4" /></Button>
      </div>
      {(begin.isError || status.isError) && <p role="alert" className="text-destructive">{t("repair.connectionFailed")}</p>}
      {job && <>
        {job.checks.length > 0 && <ul className="grid gap-x-6 gap-y-2 sm:grid-cols-2">
          {job.checks.map(check => <li key={check.name} className="flex min-w-0 items-start gap-2">
            {check.state === "ready" ? <CheckCircle2 className="mt-0.5 size-4 shrink-0 text-emerald-700" aria-hidden /> : <Wrench className="mt-0.5 size-4 shrink-0" aria-hidden />}
            <span className="min-w-0 break-words">{t(`repair.checks.${check.name}`)} · {t(`repair.checkStates.${check.state}`)}{check.code && <span className="block text-xs text-muted-foreground">{check.code}</span>}</span>
          </li>)}
        </ul>}
        {job.steps.length > 0 && <ol className="space-y-2 border-t pt-3">{job.steps.map(step => <li key={step.action} className="break-words">
          {t(`repair.actions.${step.action}`)} · {t(`repair.stepStates.${step.state}`)}{step.code && <span className="block text-xs text-destructive">{step.code}</span>}
        </li>)}</ol>}
        {job.summary && <p className="whitespace-pre-wrap break-words" role="status">{job.summary}</p>}
        {job.failure_code && <p role="alert" className="break-words text-destructive">{job.failure_code}</p>}
        {selectionFailed && job.draft_id && <p role="alert" className="text-destructive">{t("repair.planSelectionFailed", { id: job.draft_id })}</p>}
        {waiting && elapsed >= 10 && <div className="space-y-2 border-t pt-3">
          <p>{t("repair.handoffHelp")}</p>
          <Button size="sm" variant="outline" onClick={() => {
            const message = t("repair.request", { id: job.id });
            void navigator.clipboard.writeText(message).then(() => { setCopied(true); setCopyFailed(false); }, () => setCopyFailed(true));
          }}>{t(copied ? "repair.copied" : "repair.copy")}</Button>
          {copyFailed && <p role="alert" className="text-destructive">{t("repair.copyFailed")}</p>}
        </div>}
        <details><summary className="cursor-pointer text-muted-foreground">{t("repair.details")}<ChevronDown className="ms-1 inline size-3" aria-hidden /></summary>
          <pre className="mt-2 max-h-48 overflow-auto whitespace-pre-wrap break-all text-xs">{stringifyApplicationJson({ repair_id: job.id, state: job.state, checks: job.checks, steps: job.steps })}</pre>
        </details>
      </>}
    </section>}
  </>;
}
