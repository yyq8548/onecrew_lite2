# ADR 0010: Task scheduler retirement

## Status

Superseded. The task ledger and local task scheduler were retired by the
schema 33 to 34 upgrade. This record remains only to explain the historical
design and must not be used as a current OneCrew entry point.

## Historical context

The former product modeled recurring work as CRM tasks with a claim/report
ledger. A native desktop host periodically looked for due work, while the
Codex plugin and other MCP clients could claim work opportunistically. That
design used a short claim lease and a uniqueness constraint to make concurrent
claims safe. The desktop alarm and the `crm_task_*` operations belonged to that
retired task runtime.

## Current decision

OneCrew has no task scheduler, task ledger, or `crm_task_*` MCP surface. Fresh
and upgraded stores remove the task tables and task-only idempotency records;
the current task-named toolkit is limited to user-choice interactions such as
`task.choice.request`. The OneCrew CLI, shared Web, and MCP expose current
business operations through `local-service` and the fixed `onecrew_*` tools.

No client should reintroduce a timer, task claim loop, or compatibility alias
for the retired scheduler. A future scheduled-work product requires a new
owner, persisted schema, and ADR with explicit CLI/Web/MCP contracts.

## Consequences

- Historical task plans and desktop/Envoy dispatch notes are archival evidence,
  not runbook instructions.
- Local-data upgrades preserve non-task CRM records and fail closed when the
  retired task shape or another unsupported format is encountered.
- User-choice results remain process-local interactions and do not imply a
  work queue or scheduled execution.

## Amendment (2026-09-10)

No task-named toolkit remains. The user-choice operation `task.choice.request`
was renamed to `onecrew.ask_user_question` on 2026-09-09 and belongs to the
`onecrew` toolkit; it is still a no-effect, process-local interaction with no
alias. The current-decision text above is a dated record and is not rewritten.
