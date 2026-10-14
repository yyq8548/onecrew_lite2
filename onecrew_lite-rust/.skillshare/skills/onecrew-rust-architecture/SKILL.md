---
name: onecrew-rust-architecture
description: Shape Rust modules, APIs, visibility, and async ownership when structural boundaries change. Excludes known-owner edits that preserve structure and lifecycle.
---

# OneCrew Rust Architecture

## Goal

Keep Rust boundaries cohesive, public APIs narrow, and asynchronous ownership
explicit without changing behavior accidentally.

## Hard Constraints

- **RUST-ARCH-01** Always base a crate or module boundary on one named
  responsibility or owned invariant, never a line-count target.
- **RUST-ARCH-02** Keep crate roots and intentionally public or aggregate
  boundary modules as thin facades and construction surfaces. Private cohesive
  parents may retain implementation they genuinely own.
- **RUST-ARCH-03** Keep internal modules private and expose only visibility
  required by evidenced callers.
- **RUST-ARCH-04** Use explicit crate-root re-exports for stable public paths
  and reserve `include!` for generated source.
- **RUST-ARCH-05** Keep task spawn, cancellation, join, timeout, panic, and
  unexpected-exit outcomes inside one supervisor lifecycle.
- **RUST-ARCH-06** Never hold a synchronous mutex guard across `.await`;
  justify any async guard that crosses `.await`.
- **RUST-ARCH-07** Revalidate generation, lease, or ownership after every
  asynchronous gap before committing an outcome.
- **RUST-ARCH-08** Preserve the placement owner's existing transaction and
  commit boundaries during structural changes and prove their equivalence.
- **RUST-ARCH-09** Prove public, serialization, repository, error, idempotency,
  audit, secret, concurrency, and test equivalence for structure-only
  refactors.
- **RUST-ARCH-10** Never widen API, alter persisted interpretation, or change
  runtime policy merely to make a refactor compile.
- **RUST-ARCH-11** Never treat one aggregate entry point as proof of one
  responsibility. When branches have different canonical identifiers, DTOs,
  dependencies, failure semantics, or independently growing follow-up
  operations, keep the parent as a thin facade and move branch mapping,
  behavior, and invariant tests into private sibling modules.
- **RUST-ARCH-12** Classify every moved test before changing its path: private
  invariant tests stay in the owning source module tree; fixtures shared by
  private siblings stay in a parent test-support module with at most
  `pub(super)` visibility; only public-API consumer tests belong in a crate's
  top-level `tests/` directory. Never publish production helpers to make a
  test move compile.
- **RUST-ARCH-13** Distinguish cross-crate `pub` API and deliberately stable
  facade paths from crate-internal `pub(crate)` implementation paths. Update
  all evidenced in-crate callers when an internal owner moves; never preserve
  unused historical paths with compatibility re-exports, dummy constants,
  lint allowances, or test-only references. Use the routed visibility pattern.

## Scenario Router

```rust
// ❌ BAD: preserve an unused historical pub(crate) path and manufacture a use.
pub(crate) use commands::{CommandInput, run_command};
const _: Option<CommandInput> = None;

// ✅ GOOD: update the known in-crate composition root to the real owner.
mod feature {
    pub(super) mod commands;
}
fn register() {
    let _ = feature::commands::run_command;
}
```

```rust
// ❌ BAD: expose the implementation tree as the cross-crate API.
pub mod implementation;

// ✅ GOOD: keep implementation private and re-export the intentional API.
mod implementation;
pub use implementation::Client;
```

```rust
// ❌ BAD: flatten child-owned HTTP commands through a facade for uniformity.
pub(crate) use control_commands::cancel;
fn bad_handlers() {
    let _ = feature::cancel;
}

// ✅ GOOD: the composition root names the restricted command owner directly.
pub(super) mod control_commands;
fn good_handlers() {
    let _ = feature::control_commands::cancel;
}
```

## Related Skills

- onecrew-architecture-placement — Ownership or dependency direction
  changes.
- onecrew-local-data-evolution — Persisted schema, marker, fingerprint,
  or encoding changes.
- onecrew-rust-quality — Error, panic, unsafe, lint, public API quality,
  rustdoc, traits, or Cargo workspace policy changes.
- onecrew-verify — Run scoped Rust evidence after implementation.

## Router

| Intent | Read |
|---|---|
| Crate/module/API/visibility or Rust unit/integration test placement | [Rust Crate, Module, and Public API](references/rust-crate-module-and-api.md) |
| `pub` versus restricted visibility, re-exports, composition-root paths, or compatibility-path pressure | [Rust Visibility and Re-exports](references/rust-visibility-and-reexports.md) |
| Spawned tasks, cancellation, leases, or shutdown | [Rust Async Supervision](references/rust-async-supervision.md) |
| Behavior-preserving code/test reorganization; pair with the module/API pattern when the route selects both | [Structure-Only Rust Refactor Evidence](references/structure-only-rust.md) |

## Workflow

1. Inventory current callers, public paths, serialization, state transitions,
   transactions, and tests.
2. State the responsibility and invariant of every retained or new module;
   for an aggregate, name the facade and every independently changing branch
   before editing code.
3. Freeze the public API and behavior-equivalence contract before moving code.
4. Implement the smallest coherent structural change with explicit supervisor
   and transaction ownership.
5. Add boundary tests and run scoped verification through
   `onecrew-verify`.

## When NOT to use

- Do not use it only because a Rust file is large.
- Do not use it to decide a product-layer owner; use
  `onecrew-architecture-placement`.
- Do not use it for a localized Rust behavior change with unchanged module,
  API, lifecycle, and transaction boundaries.
- Do not use it for generic Rust error/panic, unsafe-proof, lint, rustdoc,
  common-trait, or Cargo workspace quality with unchanged structure.
