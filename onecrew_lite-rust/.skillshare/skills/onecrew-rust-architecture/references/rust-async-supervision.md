# Rust Async Supervision

Use this reference before adding or reorganizing spawned tasks, async state
machines, retry loops, leases, cancellation, or shutdown behavior.

Upstream: [Tokio shared state](https://tokio.rs/tokio/tutorial/shared-state),
[Tokio graceful shutdown](https://tokio.rs/tokio/topics/shutdown),
[Tokio JoinHandle](https://docs.rs/tokio/latest/tokio/task/struct.JoinHandle.html),
[SQLite atomic commit](https://sqlite.org/atomiccommit.html).
The generation-token, unknown-effect, and supervisor panic-outcome rules below
are OneCrew Local correctness policies, not Tokio requirements.

## Critical Rules

**RUST-ARCH-06.** Never hold a synchronous mutex guard across `.await`. The
runtime's own shape is `crates/platform-runtime/src/account/lifecycle.rs`:
every `pending_account_connections.lock()` lives inside a scoped block that
extracts what the async step needs, and the guard is gone before any await.

```rust
// ❌ FORBIDDEN: sync guard held across the provider await — blocks the
// executor thread and deadlocks any other path that needs the same lock.
let mut flows = self.pending_account_connections.lock().unwrap();
let flow = flows.get_mut(&input.flow_id).unwrap();
let result = self.tg_poll_qr_login(&flow.token).await; // guard still alive

// ✅ CORRECT: lifecycle.rs extracts the token in a scoped block, releases
// the guard, then awaits; the flow is re-locked and revalidated afterwards.
let qr_token = {
    let mut flows = self
        .pending_account_connections
        .lock()
        .map_err(|_| "operator account connection lock poisoned".to_string())?;
    let flow = flows
        .get_mut(&input.flow_id)
        .ok_or_else(|| "operator account connection flow not found".to_string())?;
    /* … read token, set phase to Starting … */
    token
};
let result = self.tg_poll_qr_login(&qr_token).await;
```

A guard that must cross `.await` needs an async mutex and a written
justification that serialized access to an asynchronous resource is the
invariant. Local-service startup must retain exclusive ownership throughout its
blocking preparation and upgrade worker. The protected resource is the platform
lifecycle itself: releasing early would allow business intake mid-upgrade. Do not release guards or platform owners before
provider, supervisor, or upgrade work ends.

**RUST-ARCH-07.** Revalidate ownership after every asynchronous gap before
committing an outcome. In `operator_account_connection_status`, after
`tg_poll_qr_login(...).await` returns, the flow map is locked again, the flow
is looked up again by id, and `flow.cancel_requested` decides whether the
result completes the flow or is discarded and cleaned up.

```rust
// ❌ FORBIDDEN: committing a pre-await conclusion; a cancel or restart that
// happened during the await is silently overwritten by a stale success.
complete_flow(flow_snapshot_from_before_the_await, &profile);

// ✅ CORRECT: lifecycle.rs re-locks, re-fetches, and re-checks intent
// before the outcome commits.
let flow = flows.get_mut(&input.flow_id).ok_or_else(/* not found */)?;
if flow.cancel_requested {
    let live = flow.live.take(); // ownership handed out for awaited cleanup
    mark_flow_canceled(flow);
    live
} else {
    if let Some(profile) = completed_profile {
        complete_flow(flow, &profile);
    }
    None
}
```

**RUST-ARCH-05 (supervisor panic edge).** Every owned task must be joined, and
a join panic must become one explicit terminal supervisor outcome before the
supervisor lifecycle ends. Dropping a `JoinHandle` detaches the task and loses
the only place that can distinguish normal exit, cancellation, and panic.

```rust
// ❌ FORBIDDEN: detachment discards panic and unexpected-exit ownership.
tokio::spawn(run_child());

// ✅ CORRECT: the supervisor retains and classifies the join outcome.
let child = tokio::spawn(run_child());
match child.await {
    Ok(Ok(())) => SupervisorOutcome::Completed,
    Ok(Err(error)) => SupervisorOutcome::Failed(error),
    Err(join) if join.is_panic() => SupervisorOutcome::Panicked,
    Err(join) => SupervisorOutcome::JoinFailed(join),
}
```

Generic decisions about typed owner errors, adapter projection, and contextual
`unwrap`/`expect` belong to `onecrew-rust-quality`; this reference owns
only the task-lifecycle mechanics that ensure a panic is observed truthfully.

## Quick Reference

| Concern | Rule |
|---|---|
| Spawn owner | One object retains every `JoinHandle` or tracker; dropping a handle detaches, it does not stop |
| Detachment | A deliberate ownership choice with a named cleanup owner, never a side effect of returning |
| Join | Collect every spawned task's result (`crates/codex/src/app_server/supervisor.rs` awaits `stderr_drain.finish()` after the child exits) |
| Cancellation | One mechanism (token/channel) that every owned child observes |
| Timeout | A bounded drain with a designed fallback, e.g. `INTERRUPT_ACK_TIMEOUT` in `crates/codex/src/app_server/supervisor.rs` |
| Panic | Join every owned task and convert panic/unexpected exit to one terminal supervisor outcome |
| Stale-result guard | Generation, operation id, lease owner, or cancel flag re-checked after every `.await` |
| State shape | Mutually exclusive phases as a strict enum, never correlated `Option` fields |
| External effect | Persist intent before at-least-once effects; never blindly replay an unknown effect |
| Shutdown | Stop new work → notify children → bounded drain → designed fallback → one terminal outcome |
| Durable recovery | A persisted checkpoint plus an ownership rule decides what resumes after process loss |

## Common Patterns

### The lock-and-await timeline

For a remote or long asynchronous operation the default sequence is:
acquire → inspect/mutate → persist intent or capture generation → release →
await the external operation → reacquire → validate generation/operation/
lease ownership → persist or publish the terminal outcome → release.
`account/lifecycle.rs` follows this shape in status, finalize, and cleanup.

### Interruption is a protocol, not a dropped future

`run_codex_turn_streamed` (`crates/codex/src/app_server/supervisor.rs`) stops a turn by writing
`turn/interrupt` and then continuing to poll until `turn/completed` arrives;
only after `INTERRUPT_ACK_TIMEOUT` (5s) does it kill the process, and it
still reports the truthful `CodexTurnStatus::Interrupted`. Dropping the
future kills the child too (`kill_on_drop`), but that is documented as the
unclean path: the turn is left unclosed and in-flight approval requests are
orphaned. An interrupt that arrives before the turn id exists is recorded as
intent and sent once the id lands — the module's tests pin both behaviors.

### Hand ownership out of the lock for awaited cleanup

`cleanup_connection_flows_async` collects expired `LiveConnection` values out
of the locked map (`flow.live.take()`), drops the guard, then awaits
`cleanup_live_connection` per value. Async cleanup never runs under the sync
lock, and each live resource has exactly one owner at every instant.

### Strict phase enums with honest terminal states

`OperatorAccountConnectionPhase` (AwaitingSecretInApp / ReadyToBegin /
Starting / AwaitingUserInApp / Finalizing / Completed / Canceled / Failed)
gates every transition, and `CodexTurnStatus` keeps Interrupted distinct from
Completed and Failed — the process exit code alone cannot tell an interrupted
turn from a finished one, which is why the enum exists. Distinguish permanent,
transient, and unknown-effect failures; an executor that may have crossed an
external-effect boundary needs stable idempotency, readback/reconciliation,
or a needs-attention outcome before any replay.

### Boundary tests to keep

Cover every applicable case: duplicate start and one-owner enforcement;
cancellation before and during the external await (see the
`crates/codex/src/app_server/supervisor.rs` test that interrupts before the
turn id exists); late success/failure after restart;
panic and unexpected task exit; lease loss or generation mismatch before the
terminal commit; cooperative shutdown and timeout fallback; recovery from a
durable checkpoint; wake/sleep and wall-clock changes when deadlines persist.

## Checklist

- [ ] Every spawned task has a named owner that joins or deliberately
      detaches it with a cleanup owner.
- [ ] No synchronous mutex guard is alive across any `.await`.
- [ ] Every async guard crossing `.await` has a written invariant
      justification (lifecycle/upgrade-style serialization).
- [ ] Ownership, generation, or cancel intent is revalidated after every
      asynchronous gap before the outcome commits.
- [ ] Stale outcomes from an older generation cannot overwrite a newer run,
      restart, or cancellation.
- [ ] Every owned task join classifies panic and unexpected exit into one
      truthful terminal supervisor outcome.
- [ ] Cancellation follows the protocol: stop intake, notify, bounded drain,
      designed fallback, one terminal outcome.
- [ ] Unknown external effects are reconciled or surfaced, never replayed
      blindly.
- [ ] The boundary test list above is covered for every applicable case.
