# Rust Visibility and Re-exports

Use this pattern when a module move changes paths, restricted visibility, a
facade, or a composition root. Rust's official rules are the authority:
[visibility and privacy](https://doc.rust-lang.org/reference/visibility-and-privacy.html),
[use declarations](https://doc.rust-lang.org/reference/items/use-declarations.html),
and the Rust Book's
[re-exporting guidance](https://doc.rust-lang.org/book/ch07-04-bringing-paths-into-scope-with-the-use-keyword.html).

## Critical Rules

Preserve cross-crate `pub` and explicitly approved facade contracts. Treat
restricted visibility as an in-crate implementation boundary: enumerate and
update its real callers rather than inventing compatibility consumers. A
re-export without a real caller or intentional API reason is not preservation;
dummy code that merely marks it used is evidence that the boundary is wrong.

## Quick Reference

| Surface | Preservation rule |
|---|---|
| Cross-crate `pub` API | Preserve or intentionally version as a breaking API change |
| Explicitly documented stable facade path | Preserve while that contract remains approved |
| `pub(crate)` | Internal to the current crate; update all evidenced in-crate callers when ownership moves |
| `pub(super)` / `pub(in ...)` | Open only to the smallest ancestor scope required by real callers |
| Private item | Keep private and colocate its invariant tests |

`pub use` is intentional API design: it exposes a caller-facing path that is
different from the implementation tree. It is not a requirement to preserve
every historical restricted path. Because all legal `pub(crate)` callers are
compiled from the current crate, source inventory plus all-target checks can
enumerate them; an imagined future internal caller is not compatibility
evidence.

## Common Patterns

### Correct boundary

```rust
// ❌ BAD: an internal owner moved, so flatten every historical item back
// through the parent and create dummy uses to silence unused warnings.
pub(crate) use commands::{CommandInput, run_command};
const _: Option<CommandInput> = None;

// ✅ GOOD: the composition root names the real owner. The child opens only as
// far as the parent/sibling caller requires.
mod feature {
    pub(super) mod commands;
}

fn register() {
    let _ = feature::commands::run_command;
}
```

Keep a facade re-export when an external `pub` consumer or an explicitly
declared stable internal facade needs that path. Remove it when it has no real
consumer and exists only to emulate the pre-refactor file layout. Never use an
anonymous constant, dead-code helper, lint allowance, compatibility alias, or
test-only reference to manufacture evidence that an unused re-export matters.

### Composition roots

HTTP handler inventories, router builders, dependency injection setup, and
other crate-internal composition roots may reference restricted owner modules
directly. This makes ownership visible and does not expose the child outside
its allowed ancestor scope. Route through a facade only when the facade is the
intentional caller contract, not as a universal layering ritual.

## Checklist

- Inventory every current caller with source search and compile all targets.
- Classify each old path as external `pub`, explicitly stable facade, or
  ordinary restricted implementation.
- Preserve Serde/JsonSchema/wire names independently from Rust source paths.
- Preserve external `pub` and approved facade paths with explicit re-exports.
- Update current in-crate consumers to the new restricted owner path.
- Reject unused compatibility re-exports and dummy-use compile assertions.
- Confirm child visibility is no broader than its real parent/sibling callers.
- Run scoped fmt, strict clippy, tests, and any public API or command-inventory
  regression owned by the affected crate.
