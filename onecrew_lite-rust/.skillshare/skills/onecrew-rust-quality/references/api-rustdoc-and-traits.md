# API, Rustdoc, and Traits

Use this pattern when a public Rust API, error type, common trait set, rustdoc,
or doctest changes without changing module/public-path architecture. The Rust
API Guidelines are [recommendations, not mandates](https://rust-lang.github.io/api-guidelines/).
Apply their guidance on
[common traits](https://rust-lang.github.io/api-guidelines/interoperability.html#types-eagerly-implement-common-traits-c-common-traits),
[failure documentation](https://rust-lang.github.io/api-guidelines/documentation.html#function-docs-include-error-panic-and-safety-considerations-c-failure),
and [future proofing](https://rust-lang.github.io/api-guidelines/future-proofing.html)
within reason. Rustdoc's official books define
[documentation tests](https://doc.rust-lang.org/rustdoc/write-documentation/documentation-tests.html)
and [rustdoc lints](https://doc.rust-lang.org/rustdoc/lints.html).

## Critical Rules

Implement traits because the type's semantics support them and downstream
users benefit, not to satisfy a derive inventory. Public errors are meaningful
and well behaved; public docs explain contracts, not implementation trivia.
Add examples where they teach use or protect a contract, but do not require a
copied example or boilerplate section on every DTO, field, or trivial accessor.

```rust
// ❌ BAD: mechanical derives promise ordering, copying, and defaults with no semantics.
#[derive(Clone, Copy, Default, Eq, Ord, Hash)]
pub struct SecretHandle(u64);

// ✅ GOOD: traits match identity and safe observation; unsupported promises stay absent.
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct RecordId(String);
```

Public APIs must not gain fields, trait implementations, conversions, or panic
behavior merely because a lint or checklist mentions them. Changing a public
trait, feature, error enum, default, or MSRV may have SemVer consequences; keep
release/version execution outside this quality analysis unless authorized.

## Quick Reference

| Surface | Scoped decision |
|---|---|
| `Debug` | Usually useful; redact or omit secret-bearing values deliberately |
| `Display` / `Error` | Meaningful public error text and source chain where applicable |
| `Clone` / `Copy` | Only when duplication semantics and cost are appropriate |
| `Eq` / `Ord` / `Hash` | Only when equality/order/hash are stable domain semantics |
| `Default` | Only when one unsurprising valid default exists |
| `Send` / `Sync` | Reflect real thread safety; never force unsafe impls for a gate |
| Examples | Teach non-obvious use or shared workflow; reuse links instead of copying |
| `Errors` / `Panics` / `Safety` | Include only for applicable, intentional contract conditions |
| Doctest | Prefer executable examples; use `no_run`/`ignore` only with a reason |

## Common Patterns

### Trait audit by semantic promise

For each public type, list realistic downstream operations, then implement the
smallest applicable common traits. Because downstream crates cannot add foreign
trait implementations to the type, omissions matter; so do incorrect promises.
Review secrets, handles, floats, state machines, and large buffers carefully.

### Public error quality

Error enums expose distinctions callers can act on, implement `Display` and
`Error` where appropriate, preserve safe sources, and avoid sensitive raw
payloads. Add `Send + Sync + 'static` only when the actual boundary needs
those properties; they are useful constraints, not universal ones.

### Rustdoc that compiles its examples

Use doctests for compact public workflows. Keep examples deterministic,
secret-free, and independent of live services. A doctest passing does not prove
Clippy policy, platform behavior, or all feature combinations; those remain
separate evidence lanes.

### Future-proof only intentional public contracts

Seal a trait only when downstream implementation is intentionally forbidden.
Hide fields or return opaque/newtype values when implementation flexibility is
part of the contract. Passive data DTOs may intentionally expose fields; do not
apply library future-proofing mechanically to every adapter shape.

## Checklist

- [ ] Public traits and derives match real type semantics and costs.
- [ ] Secret-bearing values do not leak through `Debug`, `Display`, or errors.
- [ ] Public errors are meaningful and implement applicable standard traits.
- [ ] `Send`/`Sync` reflects actual thread safety; no gate-driven unsafe impl.
- [ ] Rustdoc documents applicable errors, panics, and safety conditions.
- [ ] Examples teach real use, compile when practical, and avoid live effects.
- [ ] Rustdoc lint/doctest evidence is reported separately from compilation.
- [ ] Public compatibility or version decisions remain separately authorized.
