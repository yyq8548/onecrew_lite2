# Host Verification Matrix

Evidence layers for a service host change. A responsiveness claim needs a
boundary test plus an observation on the running application; a
compile pass alone proves nothing about thread placement.

## Preconditions

- Record the touched host files (`crates/local-service/src/**`), the
  commands or triggers whose execution changed, and whether any DTO, React
  surface, or OS mechanic also changed (those route to their owners).
- Confirm the package name: the local-service crate is
  `onecrew-local-service` (`crates/local-service/Cargo.toml`).
- Resolve the exact argv of the local-service crate test lane and the clippy lane,
  plus reuse rules, through `onecrew-verify`; this matrix owns the
  evidence rows, not the command inventory.
- Note whether a running application is available for observation; without
  one, the observation rows are `BLOCKED`, not skipped.

## Workflow

1. Select every affected row:

   | Change | Automated evidence | Observation |
   |---|---|---|
   | Command execution boundary changed | Tests for the existing async owner or blocking wrapper, then the local-service crate test lane | The page that calls the command stays interactive during a long call |
   | Lock hold narrowed or a loop replaced by a batch query | Repository test for batch/transaction behavior; host boundary coverage | Unrelated pages stay usable while an export or sync runs |
   | Trigger edge, debounce, or single-flight change | Service trigger and supervisor admission tests | Concurrent MCP connections do not duplicate snapshot work |
   | Status projection added or changed | Serialization test for the projection; desktop typecheck and vitest only if the TypeScript client changed | The UI shows the outcome without polling the worker lock |
   | Shutdown order or timeout change | Supervisor tests for cancel, panic, join failure, and timeout classification | Quit during a sync exits within `APPLICATION_SHUTDOWN_TIMEOUT` |
   | Any host change | The clippy `-D warnings` lane for the local-service crate | None |

2. Run the focused rows while implementing; keep one Cargo command per target
   directory at a time.
3. Web typecheck and vitest are untouched by a pure host change. Run them
   only when a generated contract or typed client changed, and hand that lane
   to `onecrew-peer-adapters` or `onecrew-web-ui`.
4. At handoff, run or reuse one complete evidence identity for the desktop
   crate and record exact argv, cwd, and diff fingerprint.
5. Label each observation with the build (debug or release), the action taken,
   and the timing seen; mark an unavailable observation `BLOCKED`.

## Failure recovery

- A source-contract test fails on a file you did not change: inspect the
  baseline and current owner before classifying the failure. A ratchet allowlist may
  only shrink; never widen it to make the lane pass.
- Clippy denies a warning in the new wrapper: fix the wrapper; do not allow the
  lint at the call site.
- The observation shows a stall the tests did not catch: add the missing
  contract assertion before fixing the code so the regression stays caught.
- No running application is available: report the observation rows as
  `BLOCKED` with the reason; never substitute a unit test as runtime proof.
- A row needs packaging, installation, or a live provider: check the existing
  task authorization and owner boundary; request only missing scope.

## Verification

- Every selected row has a passing automated result, an identical reused
  `PASS`, or an explicit `BLOCKED` record with a reason.
- The report states which command or trigger changed thread placement, which
  lock hold was bounded, and what the running application showed.
- No compile-only, mock, or readiness result is described as responsiveness
  proof.
- Cargo, clippy, and any frontend lane appear with exact argv and cwd.

## Stop condition

Stop when every affected row is evidenced or explicitly blocked, the desktop
crate test and clippy lanes pass on the final diff, and the responsiveness
claim is backed by a real-app observation or an honest `BLOCKED` row. User
acceptance ends the verification loop.
