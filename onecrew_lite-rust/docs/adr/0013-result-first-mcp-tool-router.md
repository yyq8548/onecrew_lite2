# ADR 0013: Result-first MCP tool router

## Status

Accepted on 2026-08-30. Supersedes ADR 0012's public tool names, catalog-first
presentation branches, public Plan, single-call executor, and v6 Envoy alias.
The desktop Envoy host and alias consumer described below are historical and
not supported after frontend Phase D; external MCP Apps and generic result
owners remain supported.
ADR 0012's typed registry and owner-safety decisions remain accepted.

## Context

The stateless registry solved `tools/list` growth, but `onecrew_tool_get` became
four protocols at once: operation detail, playbook/guide reader, card/form
builder, and Plan/Choice factory. It minted presentation authority before any
execution result existed. Agents repeated get/execute per operation, while a
public Creator Plan duplicated both registry workflow guidance and the Agent's
own execution sequence.

The application already has typed owners and a closed registry. The correction
belongs in the MCP projection and shared process-local result lifecycle, not in
a hosted session, generic provider proxy, persisted workflow engine, or second
business model.

## Decision

Ordinary surfaces publish exactly five model-visible tools:

- `onecrew_connection_list` reads bounded secret-free local connection references;
- `onecrew_tool_search` returns ranked operation summaries, complete compact guides
  for represented/inferred toolkits, and bounded workflow hints;
- `onecrew_tool_get` returns ordered exact execution contracts for one to eight
  unique operation IDs;
- `onecrew_tool_execute` prevalidates one to eight typed calls before the first
  owner invocation, executes them in order, and returns ordered outcomes plus
  an optional execute-minted `result_ref`; and
- `onecrew_result_present` presents only that result reference and is the sole
  App-bearing model-visible tool.

The Skills surface is Search/Get-only. One hidden App-only
`onecrew_result_state` capability reads and transitions the exact result binding.
There are no V1 aliases, dual public handlers, Plan/get-card/form branches, or
generic dataflow/output-reference language.

Search is guidance-complete without returning full schemas. Each represented
toolkit carries its current surface-visible ID/title/summary directory, and a
toolkit-filtered lexical miss still returns that guide. Registry playbooks are
internal workflow hints in search, not public entries. A genuine reusable
workflow must be one typed high-level runtime operation.

Get is operation-only and all-or-nothing. Execute preserves the registered
Rust decode and typed owner binding; arbitrary JSON never crosses into a
runtime/provider. Every batch is fully validated before an owner call.
Effectful calls remain ordered; input-required, failed, or unknown outcomes
stop later calls and make them explicit `skipped`. The batch is not claimed
atomic across external systems, and data-dependent arguments require a later
execute round.

Presentation is downstream of execution. Normal reads return directly.
Requested terminal results may mint bounded read-only cards; attended
operations mint an Available card and stop before the owner. The App submits
only an exact binding and a closed action. The server consumes the stored
owner payload, revalidates the pending calls, closes the Submitted barrier,
invokes the typed owner, and atomically replaces the card batch/projection with
Resolved, Failed, or Unknown state. Secret-bearing or sensitive-path
operations cannot mint the generic App projection.

`task.choice.request` is a registered no-effect operation producing a bounded
multi-question Choice result. Selection validates exactly one answer per
server question and continues with authoritative labels/IDs. Campaign and Task
create/update operations use the same attended result path; a Choice or edit
request never authorizes a later write.

`platform-runtime` owns the bounded in-memory result state: strict `rr_` and
view identities, origin/fingerprint/revision binding, one-shot actions,
terminal batch/projection updates, TTL/capacity, restart absence, and an
independent continuation CAS/lease. Continuation requests are server-built,
secret-free, absent from snapshots, and delivered only after a successful
claim. Lease expiry becomes DeliveryUnknown and never auto-resends.

External Apps use the hidden MCP route. The historical Envoy host used explicit
typed native peers over the same owner and never called loopback MCP. Its append-only transcript
writer uses `local.envoy_result_alias` v1 with `result_ref`; the old v6
presentation alias is not reinterpreted and carries no V2 authority.

## Consequences

- Unknown capability discovery is one Search; exact operations are one batch
  Get; independent work is one batch Execute.
- Complete toolkit guidance no longer requires a second guide get.
- Public Plan and get-created card authority are removed.
- MCP App and Envoy share one process-local owner while keeping distinct peer
  adapter transports.
- Restart, expiry, invalid binding, replay, and unknown delivery fail closed;
  browser storage and transcript prose cannot reconstruct authority.
- Provider/CRM confirmation, idempotency, audit, readback, provenance, and
  unknown-effect policy remain with their typed owners.
- The new transcript alias identity required a historical native/plugin base
  release;
  historical files are not rewritten.

## Amendment (2026-09-10)

`task.choice.request` was renamed to `onecrew.ask_user_question` on 2026-09-09;
its no-effect multi-question Choice semantics are unchanged. The Task runtime
and its create/update operations were retired with Creator schema v34 (release
0.5.22), and the Campaign create/update operations with Creator schema v36
(release 0.6.0, 2026-09-10); the attended result path they shared now serves
`onecrew.search.plan.prepare`, whose confirmed plan binds the exact search
operation contracts it authorizes. The decision body above is a dated record
and is not rewritten.

## Amendment (2026-09-17): 独立只读批次的有界并行

用户确认以通用 MCP 批次能力解决独立读取串行瓶颈，并要求相应架构限制一并处理。本修订是已接受的目标决策，实施与验收由 [Creator 保存与读取吞吐规格](../../.scratch/creator-save-and-read-throughput/spec.md) 跟踪；不宣称当前执行器已改为并行。

本修订仅替代原文对独立只读批次的“executes them in order”与停止后所有“later calls”均 skipped 的承诺，其他批次及所有权规则保持不变：

- 所有调用仍在任何 owner 开始前完成输入、schema、surface 和授权预校验；排队不延长授权，实际执行仍受 typed owner 的即时准入约束。
- 仅整批都是彼此独立的外部只读、普通 payload、无 attended prompt／owner review／写入／先后依赖时才允许有界并行。混合批次整体顺序执行，数据依赖仍需要下一轮 execute；不新增通用 dataflow 语言。
- 并发资格属于 MCP 注册期封闭的 typed 执行声明，并与 effect、approval、output class 等契约一致。不能按名称、平台名单或单一 read 标签推断；缺失必需声明或矛盾声明使注册失败。中立 capability catalog 不持有执行规则或状态。
- envelope 调度上限与 provider／credential 请求上限独立。共享 session／credential owner 管理跨 envelope、单项读取和保存准备的限制，不以每次新建客户端绕开。平台读取的本地健康提交及实际效果不能被“只读”标签抹去。
- scheduler 首次观察到停止结果后停止启动排队项。只有未启动项为 skipped，已经启动的调用全部由生命周期 owner 管理直至结束，并报告真实成功、失败、已应用本地效果或未知效果；不承诺失败输入位置之后的项全部未执行。
- 最终 outcomes 保持输入序和 call ID 关联，不按完成序排列；所有已启动调用结算后才封存最终批次和展示结果。批次不具备跨系统原子性，不重读整批或回滚已完成读取来模拟原子性。
- 取消、超时、断连和 service drain 不得造成 detached task、提前释放 guard／permit 或伪造无效果。凭据轮换、失效及并发健康提交仍由原 owner 版本与准入政策治理。
- 写入、用户确认、owner review、结果绑定、一次性消费、幂等、审计、continuation 与未知效果保护不因本修订放宽。搜索计划继续遵守 ADR 0015 的精确范围、有效期及 criteria-only 模型。

验证必须覆盖预校验失败零调用、并发重叠与双层上限、混合批顺序、失败后不再调度、已启动项真实结算、乱序完成但输入序输出，以及取消／轮换／shutdown 生命周期。新能力由既有 execute 入口提供，不为实现并发新增 YouTube 专属批量 operation。
