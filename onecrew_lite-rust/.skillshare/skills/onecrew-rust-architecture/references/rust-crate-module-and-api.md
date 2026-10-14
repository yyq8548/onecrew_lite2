# Rust Crate, Module, and Public API

Use this reference when creating or splitting a Rust crate or module, changing
`lib.rs`, public paths, visibility, models, ports, repository implementations,
or test placement.

Upstream: [Rust Book on modules and privacy](https://doc.rust-lang.org/stable/book/ch07-02-defining-modules-to-control-scope-and-privacy.html),
[Rust Reference on visibility](https://doc.rust-lang.org/reference/visibility-and-privacy.html),
[Rust API Guidelines on future proofing](https://rust-lang.github.io/api-guidelines/future-proofing.html),
[Rust Book on test organization](https://doc.rust-lang.org/book/ch11-03-test-organization.html).
The thin-facade rule, responsibility/invariant seams, aggregate-branch split,
test-placement decision, and import conventions below are OneCrew Local
policies. The upstream documents provide Rust's module, privacy, restricted
visibility, re-export, and unit/integration test mechanisms; they do not impose
this repository's particular architecture.

## Critical Rules

**RUST-ARCH-01 / RUST-ARCH-03.** A module boundary is one named responsibility
with the narrowest visibility its evidenced callers need — never a line-count
target, and never `pub` because it compiles faster. Private sibling concerns
follow one-way dependencies; lower-level modules do not reach back into their
supervisor or composition owner.

```rust
// ❌ FORBIDDEN: a cross-module helper opened to the whole crate "to be safe";
// every crate consumer can now couple to a wire-translation detail.
pub fn translate_app_server_item(item: &serde_json::Value) -> Option<serde_json::Value> { /* … */ }

// ✅ CORRECT: crates/codex/src/app_server/protocol.rs opens the
// helper only to its sibling consumer (supervisor.rs) via the parent module.
pub(super) fn translate_app_server_item(item: &serde_json::Value) -> Option<serde_json::Value> { /* … */ }
```

**RUST-ARCH-02 / RUST-ARCH-04.** Crate roots and intentionally public or
aggregate boundary modules stay facades: private `mod` declarations plus
explicit `pub use` re-exports, so internal module movement never changes
consumer paths. A private cohesive parent may keep implementation when it owns
one invariant; being a parent is not itself evidence that it must be empty.
`crates/codex/src/lib.rs` declares private `app_server`, `cli`, and `plugin`
modules and re-exports the stable surface.

```rust
// ❌ FORBIDDEN: exposing the module tree as the public path; every internal
// reorganization now breaks `use onecrew_codex::app_server::…`.
pub mod app_server;

// ✅ CORRECT: crates/codex/src/lib.rs — private module, explicit
// re-export of exactly the intended surface.
mod app_server;
mod cli;
mod plugin;
pub use app_server::{
    CodexApprovalDecision, CodexApprovalReply, CodexThreadOutcome,
    CodexThreadSpec, CodexTurnControl, CodexTurnStatus,
    run_codex_turn_streamed,
};
pub use cli::{CodexCli, CodexCliMissing, resolve_codex_cli};
pub use plugin::{ensure_codex_plugin, stage_codex_marketplace};
```

## Quick Reference

| Concern | Rule |
|---|---|
| Boundary justification | Cohesive behavior, an invariant, dependency direction, transaction or lifecycle ownership, or a test seam — file length is only a symptom |
| New crate | Only for an independently meaningful dependency, protocol, trust, lifecycle, persistence, or reusable-facade boundary with real consumers; otherwise a private module |
| `lib.rs` content | Crate docs, private `mod` declarations, explicit `pub use` re-exports, small construction wiring, crate-level facade methods |
| Visibility ladder | private → `pub(super)` → `pub(crate)` → `pub`, chosen from an explicit caller inventory |
| Domain/model role | Canonical IDs, values, states, transitions, invariants, intentional wire DTOs; never SQLite, provider transport, HTTP, or MCP policy |
| Port role | The smallest capability or repository contract its consumer use case needs; never SDK or storage details |
| Application/runtime role | Use-case sequencing, policy, confirmation, idempotency, audit, terminal outcome; never transport-shaped DTOs |
| Repository adapter role | Schema, SQL, row mapping, conditional writes, one atomic persistence boundary; never network calls |
| Provider adapter role | Mapping canonical ports to provider facades; never Creator policy or persistence orchestration |
| Supervisor role | Spawned-task ownership, cancellation, join, timeout, panic, unexpected exit, observable terminal state; never domain decisions |
| Public DTO moves | Inventory Serde attributes, JsonSchema names, error text, trait impls, and public paths first |
| Imports | Explicit production imports; `use super::*` only for adjacent `#[cfg(test)]` modules |
| `include!` | Generated source only; never hides a large handwritten implementation |

Do not create every role mechanically: add a port only for real dependency
inversion, a repository only for durable atomic state, an adapter only to
translate two existing contracts.

## Common Patterns

### Boundary record before any split

Write one sentence per proposed module:
`[module] owns [responsibility] and preserves [invariant]`. A long file with
one invariant may remain cohesive, while a short file mixing policy, SQL,
transport, and supervision is already wrongly grouped.

### Module-per-concern at an intentional facade

`crates/codex/src/app_server/mod.rs` holds only the module doc, private
`mod` declarations, and `pub use` lines per concern. The crate-root
`#[cfg(test)] mod tests_support` helper (`fake_codex`) is opened only far
enough for private sibling tests to reuse it. No implementation lives in the
intentional facade. A private parent that still owns one cohesive invariant may
retain that implementation; split it only when independently changing concerns
provide the seam.

### One aggregate operation can contain several module responsibilities

A single public list, inventory, or dispatch method is one use case, not
automatic evidence that all of its branches share one implementation
responsibility. Split branch modules when they own different canonical IDs,
DTOs, dependencies, failure semantics, or follow-up operations; keep branches
together only when those invariants genuinely move together.

```rust
// ❌ FORBIDDEN: the facade owns every independently changing branch merely
// because callers receive one aggregate response.
impl Runtime {
    pub fn inventory(&self) -> Inventory {
        Inventory {
            operator_accounts: map_operator_accounts(self.sessions.list()),
            service_credentials: join_vault_and_service_refs(self.vault.list()),
        }
    }
}

// ✅ CORRECT (schematic): mod.rs owns the response and orchestration;
// private siblings own branch-specific mapping and tests.
mod operator_accounts;
mod service_credentials;

impl Runtime {
    pub fn inventory(&self) -> Inventory {
        Inventory {
            operator_accounts: operator_accounts::summaries(self),
            service_credentials: service_credentials::summaries(self),
        }
    }
}
```

Do not turn this into a mechanical one-provider-per-file rule. Two branches
may share a module when they share the same owner, identifier family,
dependencies, failure model, and test invariant. Conversely, a small file is
already wrongly grouped when those properties differ. Split invariant tests
with their branch modules; keep only aggregate serialization and orchestration
tests beside the facade.

### Adapter composition stays separate from operation ownership

`crates/mcp-adapter/src/tools.rs::surface_tool_router` starts from the V2
router, removes execution/result/connection routes for the read-only Skills
surface, and applies registry-owned metadata. Business operations remain in
their registry and runtime owners. Do not restore retired account/mail/task
router families as a module-splitting example: transport composition and
domain ownership are different concerns.

### Repository split by invariant, not by CRUD verb

A stateful engine separates by owned invariant — for example schema creation,
durable definitions, occurrence history, lease CAS, and checkpoints as
sibling files under one `repository/` facade that still owns the atomic
transaction. Keep two responsibilities together when splitting them would
obscure one transaction (see RUST-ARCH-08).

### Test placement follows the invariant

Classify each test before moving it:

| Test role | Placement | Visibility |
|---|---|---|
| Private invariant/unit test | owning source module's `#[cfg(test)]` subtree | private ancestor access |
| Fixture shared by private sibling tests | parent source `tests.rs` or test-support child | private or `pub(super)` |
| Public consumer/integration contract | crate-level facade test or top-level `tests/` | public crate API only |

Never move a private test to top-level `tests/` by widening a helper to
`pub(crate)` or `pub`. Prefer `feature.rs` plus `feature/child.rs` for nested
source; `feature/mod.rs` remains valid Rust.

## Checklist

- [ ] Every module has one named responsibility and owned invariant.
- [ ] An aggregate facade contains only response composition and shared
      invariants; independently changing branches and their tests live in
      private sibling modules.
- [ ] `lib.rs` is a facade: private `mod` declarations plus explicit re-exports.
- [ ] Public paths and wire contracts are inventoried and intentionally
      preserved or changed.
- [ ] Visibility is no wider than required callers; cross-module helpers open
      to `pub(super)` and no further without evidence.
- [ ] No `pub mod` exposes an internal tree as a consumer path.
- [ ] One atomic use case still has one transaction owner.
- [ ] Tests remain adjacent to the invariant or exercise only the public API.
- [ ] No `use super::*` outside adjacent test modules; no `include!` for
      handwritten source.
