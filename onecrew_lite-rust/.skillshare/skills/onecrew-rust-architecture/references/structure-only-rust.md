# Structure-Only Rust Refactor Evidence

Execute this procedure only after the architecture skill classifies a request
as a structure-only Rust refactor. Record concise source evidence; do not turn
the procedure into a redesign proposal.

The repository's worked example is commit `5cd122b` — "split the codex module
by concern": the 1556-line `codex_runner.rs` became seven sibling files under
`crates/platform-runtime/src/codex_runner/`, cross-module helpers opened to
`pub(super)` and no further, all 94 tests moved (not rewritten) to the module
owning what they test, and `lib.rs` untouched so the public surface was
identical by construction. A later crate extraction moved those responsibilities
to `crates/codex/src/app_server/`; the historical commit evidence
above intentionally retains its original path. Every step below has that
shape as its bar.

## Preconditions

- The request is classified structure-only: no behavior, API, persisted
  format, lifecycle, or policy change is intended.
- `git status --short` shows no unrelated dirty change overlapping the
  proposed files.
- The direct callers and immediate consumers of the moved code are
  enumerable from source.
- The preservation contract distinguishes cross-crate `pub` and explicitly
  stable facade paths from ordinary `pub(crate)` implementation paths; no
  hypothetical inactive in-crate consumer is treated as a compatibility API.
- Any persisted-format suspicion has already been routed to
  `onecrew-local-data-evolution` instead.

## Workflow

1. **Record the boundary.** State the extracted responsibility and its owned
   invariant in one sentence per module
   (`[module] owns [responsibility] and preserves [invariant]`). Name the
   parent facade, proposed child modules, direct callers, and immediate
   consumers, and show why the seam follows a dependency or invariant, not a
   target file size. List which private helpers move and which stay.
2. **Inventory the contract surfaces.** Before editing, record for each
   applicable surface the evidence a later diff must preserve:
   - Rust API: cross-crate `pub` paths, explicitly stable facade paths,
     re-exports, constructors, trait implementations, restricted visibility,
     and every current in-crate caller. Historical `pub(crate)` paths are not
     preserved without an evidenced current consumer or explicit contract.
   - Serialization: Serde attributes, field names, defaults, flattening,
     tagged enums, wire values, JsonSchema names.
   - Adapters: MCP tool and HTTP command names, typed inputs, outputs,
     errors, peer-adapter call paths.
   - Repository: schema version, table/index definitions, SQL ordering,
     transaction entry and commit points, rollback behavior.
   - Workflow: exact errors, idempotency reservations and completion, audit
     writes, change notifications.
   - Security: secret owner, creation, access, redaction, persistence,
     cancellation/shutdown cleanup, zeroization.
   - Concurrency: lock order, mutation points, lock release, each `.await`,
     reacquisition, cancellation, terminal-state checks.
   - Tests: existing assertions, failure branches, concurrency cases,
     fixtures, direct consumers covered.
3. **Prove visibility from callers.** For each moved or newly exposed item,
   record required callers, the smallest sufficient scope (private /
   `pub(super)` / `pub(crate)` / `pub`), and why a narrower scope fails.
   Record every production import explicitly and remove any `use super::*`
   outside tests. Reject a widening justified only by convenience or a
   compile error (RUST-ARCH-10).
   A composition root may call a restricted owner module directly. Do not
   flatten commands or DTOs back through a facade solely to hide the owner
   path, and do not add dummy constants, type aliases, or test-only uses to
   make an otherwise-unused re-export appear necessary.
4. **Prove async-ownership equivalence** (when the moved code contains
   spawned tasks or `.await` gaps). Draw the phase-transition table with
   terminal phases; record the lock timeline (acquire → inspect/mutate →
   release → remote `.await` → reacquire → validate ownership → commit);
   identify the operation id, generation, or cancel flag checked after each
   gap; prove stale outcomes cannot overwrite a newer run or a cancellation;
   confirm mutually exclusive outcomes stay a strict enum.
5. **Prove transaction equivalence.** Map begin, reads, writes, idempotency
   state, audit state, notifications, commit, and rollback. Confirm extracted
   helpers still receive the same `&Transaction` or equivalent unit of work,
   that no wrapper introduces an intermediate commit or second transaction,
   and that query ordering and affected-row expectations match before/after.
6. **Move the code without editing behavior.** Move tests with their
   invariants; a shared fake goes to a parent `#[cfg(test)]` helper opened
   `pub(super)` (the pattern of
   `crates/codex/src/lib.rs::tests_support`).
   Inspect the unformatted move for behavior edits before applying
   formatters.
7. **Review the diff for equivalence.** Compare derives, attributes,
   constants, match arms, error text, and side-effect order. Map every moved
   test assertion to its new location; explain any added assertion
   separately; confirm no stronger fixture, failure condition, or concurrency
   assertion became weaker. Confirm every retained re-export has either a
   current caller or an explicit stable-API reason, and that no compatibility
   shim preserves an unused internal path. Confirm the blast radius with
   `git diff --stat`.

## Failure recovery

- Ownership after an asynchronous gap cannot be proven from current source
  and tests → stop the extraction; the missing evidence is a finding, not a
  license to guess.
- One atomic use case would be split across transaction scopes → stop and
  keep the responsibilities together under the transaction owner.
- The move only compiles after widening visibility or adding a re-export →
  the boundary is wrong; redraw the seam instead of widening the API.
- A scripted edit or formatter touched more than the intended files (a
  killed script leaves truncated files; rustfmt on a module root rewrites
  the subtree) → restore from git and redo the move by hand.
- A gate fails → diagnose at the source and fix the move; never weaken a
  lint, delete an assertion, or loosen a type to make the refactor green.

## Verification

Run the narrow behavior-equivalence tests during the move. On the final diff,
use `onecrew-verify` to resolve and run or reuse the complete gate for
each touched crate; standard command spelling and evidence identity live there,
not in this owner reference. Record exact argv, cwd, status, and concise
evidence, and report environmental failures as blocked coverage.

## Stop condition

The procedure is finished when the boundary, visibility, async-ownership, and
transaction evidence are recorded; the diff shows moved code with unchanged
behavior, contracts, and test strength; focused checks and final owner evidence
are green or identically reused; and the final paths match the intended blast
radius.
Anything beyond the requested boundary is out of scope — stop rather than
continue restructuring adjacent modules.
