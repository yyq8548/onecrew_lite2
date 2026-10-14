import type { WorkspaceActivity } from "@mcp-ui/lib/result-contract/workspace";
import { OneCrewHeader } from "@mcp-ui/components/ui/onecrew-brand";
import { Card, CardHeader, CardTitle, CardDescription } from "@mcp-ui/components/ui/card";

export function WorkspaceStatusView({ phase, activity, error, libraryError, locale, updatedAt, historical = false }: {
  phase: string; activity: WorkspaceActivity | null; error: string | null; libraryError: string | null; locale: "zh-CN" | "en-US"; updatedAt: number | null;
  historical?: boolean;
}) {
  const zh = locale === "zh-CN";
  const stages = zh ? { ready: "等待继续", searching: "正在搜索", verifying: "正在核实", pausing: "暂停中", paused: "已暂停", finished: "本轮搜索结束", budget_exhausted: "读取上限已用完", needs_evidence: "等待核实证据", error: "读取结果未确认" }
    : { ready: "Ready to continue", searching: "Searching", verifying: "Verifying", pausing: "Pausing", paused: "Paused", finished: "Search round ended", budget_exhausted: "Read limit reached", needs_evidence: "Evidence needed", error: "Read outcome unconfirmed" };
  const interrupted = phase === "disconnected" || !!error;
  const title = historical ? (zh ? "历史任务记录" : "Previous task record")
    : interrupted ? (zh ? "状态更新已中断" : "Status updates interrupted")
    : phase === "failed" ? (zh ? "onecrew_lite 启动异常" : "onecrew_lite startup failed")
    : phase !== "ready" ? (zh ? "onecrew_lite 连接中" : "Connecting onecrew_lite")
    : libraryError && activity?.library_sync_pending !== false ? (zh ? "动态表格未连接" : "Results table unavailable")
    : activity?.stale ? (zh ? "工作台状态待更新" : "Workbench status needs refresh")
    : activity?.stage === "error" && activity.failure_phase === "score" ? (zh ? "评分中断" : "Scoring interrupted")
    : activity ? stages[activity.stage] : (zh ? "onecrew_lite 已连接" : "onecrew_lite connected");
  const number = (value: number) => new Intl.NumberFormat(locale).format(value);
  const reasons = zh ? {
    user_paused: "你已暂停搜索", search_page_left: "离开搜索页后暂停", page_closed: "搜索页关闭或刷新后暂停",
    request_failed: "执行请求结果未确认，后续读取已停止", revision_changed: "计划版本已变化", progress_unconfirmed: "未确认新的读取进度",
    busy: "已有读取正在执行", needs_evidence: "需要核实已有证据", search_exhausted: "本计划可执行的搜索已完成",
    target_reached: "已达到计划目标", budget_exhausted: "读取上限已用完", execution_interrupted: "本地服务或账号连接中断，请在工作台核对进度",
  } : {
    user_paused: "You paused the search", search_page_left: "Paused after leaving the search page", page_closed: "Paused after the search page closed or reloaded",
    request_failed: "Execution outcome unconfirmed; further reads stopped", revision_changed: "The plan revision changed", progress_unconfirmed: "New read progress unconfirmed",
    busy: "A read is already in progress", needs_evidence: "Saved evidence needs review", search_exhausted: "Executable searches in this plan are complete",
    target_reached: "Plan target reached", budget_exhausted: "Read limit reached", execution_interrupted: "Local service or account connection interrupted; check progress in the workbench",
  };
  return <Card role="status" aria-live="polite" className="result-shell gap-0 py-0 shadow-none">
    <OneCrewHeader />
    <CardHeader className="gap-3 px-4 py-4"><CardTitle className="text-lg leading-snug">{title}</CardTitle>
      {activity && <>
        <p className="line-clamp-2 break-words text-sm font-medium">{activity.brief.split(/\n(?:Default search entries when no platform is requested:|Creator location:|Content language:|Content category:|Preferred search keywords:|Search only these entries:|Search all supported entries;)/)[0].trim()}</p>
        {activity.stop && <CardDescription>{reasons[activity.stop.reason]}</CardDescription>}
        <dl className="grid grid-cols-2 gap-x-4 gap-y-2 text-sm tabular-nums">
          <div><dt className="text-muted-foreground">{zh ? "推荐候选人" : "Recommended candidates"}</dt><dd>{number(activity.qualified)}/{number(activity.target)}</dd></div>
          <div><dt className="text-muted-foreground">{zh ? "待核实" : "Pending review"}</dt><dd>{number(activity.pending)}</dd></div>
          <div><dt className="text-muted-foreground">{zh ? "已尝试来源" : "Sources attempted"}</dt><dd>{number(activity.attempted)}/{number(activity.total)}</dd></div>
          <div><dt className="text-muted-foreground">{zh ? "读取次数" : "Reads"}</dt><dd>{number(activity.used)}</dd></div>
        </dl>
        {(activity.login_required + activity.read_errors + activity.restricted > 0) && <p className="text-sm text-muted-foreground">{zh
          ? `需登录 ${activity.login_required} · 累计读取失败 ${activity.read_errors} · 平台限制 ${activity.restricted}`
          : `Sign-in needed ${activity.login_required} · Failed read attempts ${activity.read_errors} · Restricted ${activity.restricted}`}</p>}
        {!!activity.current_read_stops && <CardDescription>{zh ? `读取受阻：${activity.current_read_stops} 个来源` : `Read blocked: ${activity.current_read_stops} sources`}</CardDescription>}
        {activity.read_operation_stops?.map(stop => <CardDescription key={`${stop.platform === "twitter" ? "X" : stop.platform}:${stop.profile_ref}:${stop.operation}`}>
          {zh ? `${stop.platform === "twitter" ? "X" : stop.platform} 的${stop.operation === "profile" ? "主页" : "内容"}读取已停止。请修复连接或读取问题，再审核新计划后重试。` : `${stop.platform === "twitter" ? "X" : stop.platform} ${stop.operation} reads stopped. Resolve the connection or read issue, then approve a new plan before retrying.`}
        </CardDescription>)}
        {activity.stale && <CardDescription>{zh ? "工作台未更新；请回到工作台确认当前进度。" : "Workbench updates are stale. Return to the workbench to check progress."}</CardDescription>}
        {activity.library_sync_pending && <CardDescription>{zh ? "结果待同步；搜索进度已保存。" : "Results need syncing; search progress is saved."}</CardDescription>}
        {activity.library_sync_error && <CardDescription>{zh ? "表格同步诊断：" : "Table sync diagnostic: "}{activity.library_sync_error}</CardDescription>}
        {activity.library_synced_at && !activity.library_sync_pending && !activity.library_sync_error && <CardDescription>{zh ? "表格已同步" : "Table synced"}</CardDescription>}
        {!!activity.deferred_scores && <CardDescription>{zh ? `后台评分待续：${activity.deferred_scores}` : `Backend scores pending: ${activity.deferred_scores}`}</CardDescription>}
        {activity.failure_phase && <CardDescription>{zh ? "异常阶段：" : "Failure phase: "}{(zh ? { read: "平台读取", score: "评分", admission: "账号检查", readback: "进度回读", execution: "后台执行" } : { read: "Platform read", score: "Scoring", admission: "Account check", readback: "Progress readback", execution: "Background execution" })[activity.failure_phase]}</CardDescription>}
        {activity.failure_code && <CardDescription>{activity.failure_code}</CardDescription>}
      </>}
      {!activity && <CardDescription>{zh ? "请在工作台创建搜索计划。" : "Create a search plan in the workbench."}</CardDescription>}
      {(activity?.updated_at ?? updatedAt) && <CardDescription>{zh ? "上次更新：" : "Last updated: "}{new Intl.DateTimeFormat(locale, { hour: "2-digit", minute: "2-digit", second: "2-digit" }).format(activity?.updated_at ?? updatedAt!)}</CardDescription>}
      {interrupted && <CardDescription>{zh ? "界面同步中断，任务是否运行请在工作台确认。" : "Display sync interrupted. Check execution in the workbench."}</CardDescription>}
    </CardHeader>
  </Card>;
}
