---
name: onecrew-local-service
description: Maintain Web command admission, blocking-work dispatch, lifecycle, and supervised background tasks. Excludes React layout and native distribution.
---

# OneCrew Local Service

## Goal

Keep the local-service host responsive: async command bodies avoid blocking work,
runtime owners offload blocking operations, lock holds stay bounded, and background
work runs under one supervisor that records its own outcome for the service log.

## Hard Constraints

- **HOST-01** Keep blocking repository/filesystem calls and expensive encoding off async workers; cheap argument mapping and in-memory status projections may run inline.
- **HOST-02** Keep HTTP handlers async; inline status reads must access only instantaneous in-memory state.
- **HOST-03** Await an owner's existing async entry point when it already offloads blocking work; otherwise use the host's `blocking::run_blocking` wrapper or an owned blocking task with explicit join-failure mapping. Never nest wrappers merely for uniformity or call `block_on`/`block_in_place` on a runtime thread.
- **HOST-04** Hold the Creator repository connection lock for exactly one bounded repository call, never across a loop, network round trip, or encoding pass (guard-across-await itself is RUST-ARCH-06); batch and export paths use `creator-runtime` batch queries, not per-row detail loaders.
- **HOST-05** Let startup own the exclusive data inspection/upgrade lifecycle after process ownership checks; expose no manual upgrade command. Business handlers require Ready; startup failure remains blocking.
- **HOST-06** Route background work through the existing supervisor (RUST-ARCH-05); this Skill owns the trigger edges, single-flight admission, debounce, and where the outcome is recorded.
- **HOST-07** Land every terminal background outcome where its audience can reach it without taking the worker's lock. Work a product decision makes imperceptible to the user stays in the supervisor's own record and the service log: no status field, service event, CLI line, or Web surface reports it. Project an outcome only where the user must act on it.
- **HOST-08** Let `try_lock` skip optional work only; it never decides correctness, startup readiness, admission, or status.
- **HOST-09** Give long-lived supervised blocking work cooperative cancellation and bounded shutdown accounting; keep short one-shot calls bounded and joined without inventing a cancellation protocol for each call. Never treat aborting the caller as stopping a running blocking task.
- **HOST-10** Shut down in order: close intake, cancel, bounded drain, timeout classification.

## Related Skills

- onecrew-rust-architecture — Supervisor mechanics or a lock crossing await change beyond host wiring.
- onecrew-peer-adapters — A command or MCP operation contract changes.
- onecrew-creator — The repository needs a batch query or a narrower lock scope.
- onecrew-web-ui — The UI must render a background status projection.
- onecrew-native-cli — OS window, permission, or packaging mechanics change.
- onecrew-verify — Host responsiveness evidence is claimed.

## Router

| Intent | Read |
|---|---|
| A command stalls, a page spins, or a synchronous call sits in an async body | [Command Execution and Blocking](references/command-execution-and-blocking.md) |
| Startup or MCP-triggered background sync must run silently, with no user-visible surface | [Background Work Supervision](references/background-work-supervision.md) |
| Any host change claims responsiveness evidence | [Verification Matrix](references/verification-matrix.md) |

## Workflow

1. Name the stalled command or trigger, the thread it runs on (main, tokio worker, blocking pool), and every lock it takes.
2. Read the routed reference; keep the command DTO, React surface, and OS mechanics with their owners.
3. Reuse the existing async owner or blocking wrapper, bound each lock hold to one repository call, and wire background work through the supervisor and its recorded outcome.
4. Extend behavioral or source-contract coverage for the execution boundary that changed and select the affected verification rows.
5. Report host responsiveness with source-contract evidence plus a real-app observation, or an explicit `BLOCKED` row.

## When NOT to use

- Do not use it for a HTTP or MCP DTO, schema, or parity change with unchanged execution.
- Do not use it for React state, query freshness, or layout work.
- Do not use it for `cfg` branches, window chrome, installers, or signing.
- Do not use it for generic supervisor or lock-across-await design (RUST-ARCH-05/06) that changes no trigger edge, admission, projection, or shutdown wiring.
