# Error, Panic, and Boundaries

Use this pattern for Rust owner errors, panic decisions, `unwrap`/`expect`, and
adapter error projection. Official guidance: the Rust Book on
[recoverable errors with `Result`](https://doc.rust-lang.org/book/ch09-02-recoverable-errors-with-result.html)
and [when to panic](https://doc.rust-lang.org/book/ch09-03-to-panic-or-not-to-panic.html),
plus the Rust API Guidelines on
[meaningful error types](https://rust-lang.github.io/api-guidelines/interoperability.html#error-types-are-meaningful-and-well-behaved-c-good-err)
and [failure documentation](https://rust-lang.github.io/api-guidelines/documentation.html#function-docs-include-error-panic-and-safety-considerations-c-failure).
These are scoped recommendations, not a ban on panics or shortcuts.

## Critical Rules

Recoverable failures use `Result` so the caller can choose policy. At a public
or reusable boundary, use a meaningful typed error when callers need to match
classification, recovery, retry, audit, or sources. Preserve those facts until
the explicit public boundary maps them to a stable, non-sensitive error.
Do not mechanically replace every private leaf `Result<_, String>`: a local,
deliberately contained string can be sufficient when no caller decision or
source is lost.

```rust
// ❌ BAD: one public string model loses distinctions callers need.
pub fn read_record(id: RecordId) -> Result<Record, String> {
    storage_read(id).map_err(|error| error.to_string())
}

// ✅ GOOD: the reusable boundary preserves actionable variants.
pub fn read_record(id: RecordId) -> Result<Record, ReadError> {
    storage_read(id).map_err(ReadError::Storage)
}

pub fn read_public(id: RecordId) -> Result<Record, PublicError> {
    read_record(id).map_err(PublicError::from_read_error)
}
```

Use panic for a genuinely unrecoverable invariant violation, not ordinary
caller-controlled failure. `unwrap` and `expect` are contextual: production
paths propagate recoverable failure; a fixed test fixture or construction
invariant may use `expect` whose message explains why success is guaranteed.

## Quick Reference

| Situation | Quality decision |
|---|---|
| Caller can recover or choose policy | Return `Result<T, E>` |
| Public/reusable boundary needs matching, retry, audit, or source meaning | Keep a typed enum with the needed context |
| Private leaf has no downstream classification or source need | A deliberately contained string may be sufficient |
| Wire boundary needs a stable failure shape | Map once at that boundary |
| Impossible state proves an internal invariant broke | Panic may be appropriate; document the invariant |
| Checked-in fixture or static construction is audited | Contextual `expect` may be clearer than `unwrap` |
| External input, lock, clock, filesystem, or user state | Usually propagate operational failure; use panic/`expect` only for a proved unrecoverable invariant with an explicit recovery owner |
| Public function has recoverable errors or intentional panics | Document `# Errors` and/or `# Panics` as applicable |

## Common Patterns

### Typed owner error, stable public projection

Name variants for decisions consumers actually make. Preserve the underlying
source when it is safe and useful internally. At a public adapter, translate
to stable codes/messages that do not expose paths, sensitive values, raw
external payloads, or opaque debug text. Do not flatten retryable failures into
generic failure when callers need that distinction.

### Panic audit by recovery responsibility

For each `panic!`, `unwrap`, and `expect`, record the input source, the proved
invariant, the recovery owner, and what boundary receives the panic. Convert
ordinary operational failure to `Result`; keep a panic only when continuing
would violate a stronger invariant and the containing task or process has a
truthful terminal outcome.

### Documentation follows the real contract

Add `# Errors` for meaningful failure families and `# Panics` for intentional
documented panic conditions. Do not list hypothetical panics from arbitrary
caller-provided trait implementations, and do not add empty boilerplate to
infallible DTO accessors.

### Tests may be intentionally strict

Tests and examples can use `expect` when the invariant is the subject of the
test or the fixture is fixed. The message states the invariant, not a generic
"should work". Tests for runtime failure still exercise the typed error branch.

## Checklist

- [ ] Typed errors are used where public/reusable callers need distinctions.
- [ ] Private leaf strings are deliberate and do not discard caller decisions.
- [ ] Public projection is stable, non-sensitive, and preserves required status.
- [ ] Each panic/unwrap/expect has a classified invariant and recovery owner.
- [ ] Caller-controlled and operational failures propagate through `Result`.
- [ ] `expect` messages state why the audited invariant must hold.
- [ ] Applicable public APIs document `Errors` and `Panics` without boilerplate.
- [ ] Task-supervision and join-lifecycle mechanics remain a separate
      architectural concern from error-model quality.
