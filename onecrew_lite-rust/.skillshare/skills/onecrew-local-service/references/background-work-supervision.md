# Background Work Supervision

Make startup and MCP-triggered background work (such as Creator snapshot sync)
run silently: no stalled page, no unbounded lock hold, no user-visible surface
for work the product calls imperceptible, and an ordered shutdown.

## Preconditions

- Find every trigger point for the work. The Creator snapshot has one host
  entry point, `ServiceState::trigger_snapshot_edge` in `state.rs`, reached
  from three edges: the application graph opening while an MCP client is
  already attached, `mcp_connected`, and `mcp_disconnected`. No source-contract
  test enumerates those edges today, so a new trigger either adds one or states
  why the edge needs none. Task execution has no local auto-dispatch loop:
  external Codex or MCP agents claim due work through the task contract.
- Name the supervisor that owns the work. `CreatorSnapshotSupervisor` in
  `creator_snapshot_supervisor.rs` owns snapshot tasks with `trigger`,
  `close_intake`, `shutdown`, `SNAPSHOT_START_DELAY`, and
  `APPLICATION_SHUTDOWN_TIMEOUT`; `ServiceState::trigger_snapshot_edge` and
  `ServiceState::clear_application` in `state.rs` are the host wiring around it.
- Confirm who is allowed to see the outcome. Creator snapshot sync is
  imperceptible to the user by product decision: `log_snapshot_outcome` writes
  every terminal state to the service log, the `#[cfg(test)]` `diagnostic()`
  accessor keeps the record for this module's own tests, and no CLI, HTTP, MCP,
  or Web surface reports it. `SnapshotObserver::creator_snapshot_state_changed`
  in `state.rs` implements the lifecycle trait without publishing. Pair with
  `onecrew-web-ui` only when a product decision makes background work visible.
- Confirm the repository entry points the work calls and whether a batch query
  exists in `creator-runtime` for each bulk read.
- Keep the command contract, React surface, and OS mechanics with their owners;
  this workflow changes host wiring only.

## Workflow

1. Reuse the owner's async boundary before adding a blocking wrapper.
   `sync_creator_snapshot` in `creator_snapshot_supervisor.rs` awaits
   eligibility through `can_sync_now` in `creator-sync`, uses the
   `list_creators_async` precheck to skip empty exports, builds the bundle
   through `spawn_blocking`, maps export/join failures to recovery, and awaits
   upload. The host export command uses `blocking::run_blocking`; import
   confirmation awaits the Creator async owner directly.
2. Bound every lock hold to one repository call and replace loops of single
   calls with one `creator-runtime` batch query. Hand a missing batch query to
   `onecrew-creator` rather than looping in the host.
3. Make the trigger single-flight and debounced. The snapshot supervisor
   already rejects `run_id <= last_triggered_run_id` as `AlreadyTriggered`
   and refuses after `close_intake` as `IntakeClosed`; a new trigger reuses
   that admission rather than adding a second gate. Delay a startup trigger
   until the platform is open so it never competes with first paint.
4. Preserve both admission and run outcomes, and keep the record where its
   audience is. The supervisor records `Started`, `AlreadyTriggered`, and
   `IntakeClosed` admissions as well as terminal outcomes (`Completed`,
   `Cancelled`, `Panicked`, `JoinFailed`, `ShutdownTimedOut`). A rejected
   admission must not erase the active run or last terminal result, and status
   readers must not wait on worker execution. For imperceptible work the record
   stops at the supervisor and the service log: do not reintroduce a status
   field, service event, or CLI line to surface it.
5. Keep the shutdown order the supervisor already enforces: close intake,
   send cancel to every owned task, drain each task with
   `tokio::time::timeout(shutdown_timeout, reaper)` inside `join_owned_task`,
   and classify a timeout instead of waiting forever.
   `ServiceState::clear_application` in `state.rs` closes intake before the
   drain; new background work joins that report.
6. Split responsibility with `onecrew-rust-architecture`: RUST-ARCH-05
   and RUST-ARCH-06 own generic supervisor and lock-across-await design; this
   workflow owns the desktop trigger edges, admission, outcome recording,
   shutdown wiring, and the silence of the work, wherever the file lives.

## Failure recovery

- A page stalls during sync: find the synchronous repository or encoding call
  on the tokio worker and move it into `spawn_blocking`; do not add a UI
  timeout or a retry to hide it.
- A command unrelated to the sync stalls: the worker pool is drained by a lock
  hold; shorten the hold to one repository call and check every loop.
- A trigger fires twice or never: inspect the `run_id` monotonic admission and
  the intake flag before adding another guard; never use `try_lock` to decide
  whether the work should run.
- Shutdown exceeds its budget: inspect which child, reaper, or blocking
  segment is still running. Preserve timeout classification, bound one-shot
  work, and add cooperative cancellation where a long-lived worker supports
  it; aborting an async caller does not stop a running blocking closure.
- An imperceptible background outcome needs explaining: read the service log
  for the `log_snapshot_outcome` line and the receiving remote side. Diagnose
  it there rather than adding a projection the product decided against.
- A user-visible background outcome is missing: the result was discarded or
  written to a place no query reads; route it into the projection its product
  decision requires and hand the rendering to `onecrew-web-ui`.

## Verification

- Use the `creator_snapshot_supervisor.rs` source-contract tests
  (`snapshot_start_is_debounced_and_cancel_wins_before_the_runner`,
  `lifecycle_observer_receives_running_and_every_recorded_terminal_outcome`,
  and `runtime_snapshot_sync_prechecks_empty_store_before_exporting_on_the_blocking_pool`)
  for trigger, observer, and blocking-pool ordering; `blocking.rs` tests for
  async-owner or blocking-wrapper command boundaries; and runtime tests for
  export behavior. Do not require a literal `spawn_blocking` at a command that
  correctly awaits an async owner.
- Assert that `close_intake` precedes the drain in the shutdown path.
- Supervisor unit tests for admission, intake closure, and timeout
  classification (`one_run_is_triggered_at_most_once`,
  `application_shutdown_closes_intake`, and
  `shutdown_timeout_aborts_and_classifies_an_unresponsive_reaper` in
  `creator_snapshot_supervisor.rs` are the precedent); add cancel, panic, and
  join-failure cases beside them when the change touches those paths.
- Keep imperceptible work out of the public contract: the `LocalServiceStatus`
  schema root exported by
  `login::contract_manifest_tests::web_creator_sync_contract_manifest_is_exact_and_exportable`
  carries no snapshot field, and the generated Web contracts stay current.
- Real-app observation: switch tabs and open a list while the sync runs;
  export while the list stays usable; quit during a sync and confirm the app
  exits within the shutdown timeout with the outcome in the service log.
- Run the matrix rows from the verification reference and report an
  unobservable row as `BLOCKED` rather than assumed.

## Stop condition

Stop when every trigger is single-flight and debounced, every blocking step
runs on the blocking pool under one bounded lock hold per repository call,
every outcome is recorded where its audience can reach it — the service log for
imperceptible work — shutdown closes intake and drains within the timeout, the
source-contract tests pass, and the real-app observation or an explicit
`BLOCKED` record exists.
