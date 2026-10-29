import { discoveryProductBrief } from "@/lib/discovery-draft-preferences";
import { formatEpochMilliseconds } from "@/lib/creator";
import { discoveryExecutionFailureKey } from "@/lib/discovery-execution-error";
import { ExternalLink, Loader2, MoreHorizontal, Pause, Play, AlertCircle } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { DropdownMenu, DropdownMenuContent, DropdownMenuItem, DropdownMenuTrigger } from "@/components/ui/dropdown-menu";
import type { DiscoveryCampaign, DiscoveryCandidateCounts, syncDiscoveryLibrary } from "@/lib/opencli-discovery";
import type { WorkspaceStage } from "@/lib/discovery-workspace";
import type { discoveryLibraryFailureKey } from "@/lib/discovery-library-error";
import type { ExecutionStatus } from "@onecrew/runtime-contracts/discovery";

const scoreReasons = ["initial_match_citation_not_saved", "initial_match_rationale_too_large", "initial_match_level_invalid", "initial_match_criterion_invalid", "initial_match_outcome_invalid", "initial_match_rationale_empty", "initial_match_auxiliary_field_invalid", "initial_match_citation_count_invalid"] as const;
type ExclusionReason = "unchecked" | "signedOut" | "permission" | "failed" | "unverified" | "expired";

export function DiscoveryOverview({ campaign, counts, stage, running, busy, profileChanged, syncing, library, libraryLoading, libraryReadFailed, syncFailed, syncPending = false, syncErrorKey, syncedAt, failurePhase, failureCode, pipeline, deferredHits = [], excludedSources = [], onSync, onStart, onPause, onAnother, onExpanded, canExpand, onIssues, onFailure }: {
  campaign: DiscoveryCampaign; counts: DiscoveryCandidateCounts | undefined; stage: WorkspaceStage;
  running: boolean; busy: boolean; profileChanged: boolean; syncing: boolean;
  library: Awaited<ReturnType<typeof syncDiscoveryLibrary>> | null; syncFailed: boolean;
  libraryLoading?: boolean; libraryReadFailed?: boolean;
  syncPending?: boolean; syncErrorKey?: ReturnType<typeof discoveryLibraryFailureKey>; syncedAt?: number | bigint | null; failurePhase?: string; failureCode?: string;
  pipeline?: ExecutionStatus["pipeline"];
  deferredHits?: readonly { reason: string }[]; excludedSources?: readonly { name: string; reason: ExclusionReason }[];
  onSync: () => void; onStart: () => void; onPause: () => void; onAnother: () => void; onExpanded: () => void; canExpand: boolean; onIssues: () => void; onFailure?: () => void;
}) {
  const { t, i18n } = useTranslation("shell");
  const zh = i18n.language.startsWith("zh");
  const n = (value: number | bigint | undefined) => value === undefined ? "—" : new Intl.NumberFormat(i18n.resolvedLanguage).format(value);
  const runs = campaign.platform_runs;
  const executionErrorKey = discoveryExecutionFailureKey(failureCode);
  const attempted = runs.filter(run => run.status !== "unsearched").length;
  const supplementing = running && pipeline?.recommendations_complete === true && pipeline.contact_pending && stage !== "pausing";
  const phase = ["read", "score", "admission", "readback", "execution"].includes(failurePhase ?? "") ? failurePhase as "read" | "score" | "admission" | "readback" | "execution" : "execution";
  return <section className="space-y-6" aria-label={t("opencliPilot.overview")}>
    <div className="flex flex-wrap items-start justify-between gap-3">
      <div className="min-w-0 space-y-2">
        <h2 className="break-words text-2xl font-semibold leading-snug">{t("opencliPilot.title")}</h2>
        <p className="max-w-prose break-words text-base leading-relaxed">{discoveryProductBrief(campaign.brief)}</p>
      </div>
      <div className="flex shrink-0 items-center gap-2">
      <Button variant="outline" disabled={running || busy} onClick={onAnother}>{t("opencliPilot.another")}</Button>
      {canExpand && <DropdownMenu><DropdownMenuTrigger asChild><Button variant="ghost" size="icon" className="size-11" aria-label={t("opencliPilot.moreActions")}><MoreHorizontal className="size-5" /></Button></DropdownMenuTrigger>
        <DropdownMenuContent align="end">
          <DropdownMenuItem disabled={running || busy} onSelect={onExpanded}>{t("opencliPilot.expandAllSources")}</DropdownMenuItem>
        </DropdownMenuContent>
      </DropdownMenu>}
      </div>
    </div>
    <div className="space-y-3">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <p role="status" className="flex items-center gap-2 text-sm font-medium">{running && <Loader2 className="size-4 animate-spin" aria-hidden />}{supplementing ? (zh ? "推荐已完成，正在补充公开联系方式" : "Recommendations complete; collecting public contact details") : t(stage === "error" && failurePhase ? `opencliPilot.failure.${phase}` : `opencliPilot.stages.${stage}`)}</p>
        {running && <Button className="h-11" disabled={stage === "pausing"} onClick={onPause}><Pause className="size-4" aria-hidden />{supplementing ? (zh ? "暂停补充" : "Pause contact collection") : t(stage === "pausing" ? "opencliPilot.stages.pausing" : "opencliPilot.automaticPause")}</Button>}
      </div>
      <dl className="grid grid-cols-2 gap-x-6 gap-y-4 border-y py-4 lg:grid-cols-4">
        {[ [t("opencliPilot.qualifiedAccounts"), `${n(counts?.qualified)}/${n(campaign.target_count)}`],
          [pipeline ? (zh ? "待核实记录（含已结束）" : "Review records (including ended)") : t("opencliPilot.pendingAccounts"), n(counts?.pending)], [t("opencliPilot.attemptedSources"), `${n(attempted)}/${n(campaign.scopes.length)}`],
          [t("opencliPilot.readCount"), n(campaign.used_calls)] ].map(([label, value]) =>
          <div key={label} className="min-w-0 space-y-1"><dt className="text-sm text-muted-foreground">{label}</dt><dd className="text-xl font-semibold tabular-nums">{value}</dd></div>)}
      </dl>
      {pipeline && <div className="space-y-2 text-sm text-muted-foreground">
        <p>{zh
          ? `活跃待核实 ${n(pipeline.active_pending)} · 评分排队 ${n(pipeline.queued_scores)} · 等待当前对话评估／提交 ${n(pipeline.running_scores)}`
          : `Active reviews ${n(pipeline.active_pending)} · Scores queued ${n(pipeline.queued_scores)} · Awaiting current conversation evaluation/submission ${n(pipeline.running_scores)}`}</p>
        {pipeline.running_scores > 0 && <p>{zh ? "这些材料尚未完成模型评估和提交。后台可并发读取和同步；对话中断或额度用完时保留断点。" : "These materials still await completed model evaluation and submission. Background reads and sync can run concurrently; progress is saved if the conversation stops or reaches its usage limit."}</p>}
        {pipeline.contact_pending && <p>{pipeline.recommendations_complete ? (zh ? "推荐人数已达标；仍有公开联系方式待补充，已有结果可使用。" : "Recommendation targets are met. Saved results are available while public contact details remain pending.") : (zh ? "仍有公开联系方式待补充" : "Public contact details still need supplementing")}</p>}
        {pipeline.discovery_held && <p role="status">{zh
          ? "待核实累计达到 10 人，已暂停发现新账号；优先完成现有核实，降至 5 人后继续搜索。"
          : "Reviews reached 10. New account discovery is on hold while existing reviews finish; search resumes at 5."}</p>}
      </div>}
      <p className="text-sm leading-relaxed text-muted-foreground">{t("opencliPilot.dedupNote")}</p>
      {deferredHits.length > 0 && <details className="text-sm text-warning"><summary>{t("opencliPilot.deferredScores", { count: deferredHits.length })}</summary>
        <ul>{[...new Set(deferredHits.map(hit => hit.reason))].map(reason => {
          const known = scoreReasons.find(code => code === reason);
          return <li key={reason}>{known ? t(`opencliPilot.scoreReasons.${known}`) : t("opencliPilot.scoreValidationFailed")} · {deferredHits.filter(hit => hit.reason === reason).length}<span className="ml-2 font-mono text-xs">{reason}</span></li>;
        })}</ul>
      </details>}
      {excludedSources.length > 0 && <div className="text-sm text-muted-foreground"><p>{t("opencliPilot.excludedSources")}</p><ul>{excludedSources.map(source => <li key={source.name}>{source.name}：{t(`opencliPilot.exclusionReasons.${source.reason}`)}</li>)}</ul></div>}
      {stage === "needs_evidence" && <p role="status" className="text-sm leading-relaxed">{t(campaign.platform_runs.every(run => run.status === "completed") ? "opencliPilot.evidenceStopHelp" : "opencliPilot.evidenceBlockedHelp")}</p>}
    </div>
    {stage === "error" && !profileChanged && <div role="alert" className="space-y-2 rounded-md bg-warning/10 px-4 py-3 text-sm">
      <p>{t(executionErrorKey ? `opencliPilot.${executionErrorKey}` : `opencliPilot.failure.${phase}`)}</p>
      {failureCode && failureCode !== "query_expansion_failed" && <p className="break-words font-mono text-xs">{failureCode}</p>}
      <Button variant="outline" onClick={() => (onFailure ?? onIssues)()}>{t("opencliPilot.inspectFailure")}</Button>
    </div>}
    {profileChanged && <div role="status" className="flex flex-wrap items-center justify-between gap-3 rounded-md bg-warning/10 px-4 py-3 text-sm">
      <p className="flex min-w-0 items-start gap-2"><AlertCircle className="mt-0.5 size-4 shrink-0" aria-hidden /><span>{t("opencliPilot.profileChanged")}</span></p>
      <Button variant="outline" onClick={onIssues}>{t("opencliPilot.handleIssues")}</Button>
    </div>}
    <div className="space-y-2">
      <p className="text-xs text-muted-foreground">{t("opencliPilot.syncPolicy")}</p>
      <div className="flex flex-col gap-3 sm:flex-row sm:flex-wrap">
        {library ? <Button asChild className="h-11 text-base"><a href={library.web_url} target="_blank" rel="noreferrer">{t("opencliPilot.openLibraryTable")}<ExternalLink className="size-4" aria-hidden /></a></Button>
          : libraryLoading ? <p role="status">{i18n.language.startsWith("zh") ? "正在读取已保存的表格…" : "Reading the saved table…"}</p>
          : libraryReadFailed ? <p role="alert">{i18n.language.startsWith("zh") ? "无法读取已保存的表格，请检查连接后重试。" : "The saved table could not be read. Check the connection and retry."}</p>
          : <p className="self-center text-sm text-muted-foreground">{t("opencliPilot.tableNotCreated")}</p>}
        <Button variant="outline" disabled={syncing} onClick={onSync}>{syncing && <Loader2 className="size-4 animate-spin" aria-hidden />}{t("opencliPilot.resyncLibrary")}</Button>
      </div>
      {library && <p className="text-sm tabular-nums text-muted-foreground">{t("opencliPilot.librarySynced", { total: n(library.total_hits), added: n(library.inserted_rows) })}</p>}
      {syncedAt && <p className="text-xs text-muted-foreground">{t("opencliPilot.lastSync")} {formatEpochMilliseconds(syncedAt, i18n.resolvedLanguage, { hour: "2-digit", minute: "2-digit", second: "2-digit" })}</p>}
      {syncPending && !syncFailed && <p role="status" className="text-sm text-muted-foreground">{t("opencliPilot.syncPending")}</p>}
      {syncFailed && <p role="alert" className="text-sm text-destructive">{t(`opencliPilot.${syncErrorKey ?? "newResultsSyncFailed"}`)}</p>}
    </div>
    {campaign.phase === "active" && <div className="space-y-2">
      {!running && stage !== "needs_evidence" && <Button variant="outline" disabled={profileChanged || busy} onClick={onStart}><Play className="size-4" aria-hidden />{t("opencliPilot.automaticStart")}</Button>}
      {running && <p className="max-w-prose text-sm leading-relaxed text-muted-foreground">{t("opencliPilot.pauseEffect")}</p>}
    </div>}
  </section>;
}
