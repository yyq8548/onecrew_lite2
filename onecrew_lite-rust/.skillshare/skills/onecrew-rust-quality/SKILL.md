---
name: onecrew-rust-quality
description: Improve Rust errors, unsafe proofs, public API docs, lints, and feature contracts when those concerns change. Excludes routine Rust edits and vendor protocol policy.
---

# OneCrew Rust Quality

## Goal

Make Rust failure, safety, API, documentation, lint, and workspace policy
explicit at the narrowest responsible boundary without turning guidance into
blanket bans or changing product behavior by accident.

## Hard Constraints

- **RUST-QUALITY-01** Use meaningful typed errors at public or reusable owner
  boundaries when callers need classification, recovery, retry, audit, or
  sources. Do not mechanically replace every private leaf `Result<_, String>`;
  project stable, secret-free errors only at the actual wire boundary.
- **RUST-QUALITY-02** Choose `Result`, panic, `unwrap`, and `expect` from
  recovery responsibility and proved invariants. Do not impose a blanket ban.
- **RUST-QUALITY-03** Keep each `unsafe` block narrow, state its complete proof
  in a nearby `SAFETY` comment, and expose a safe abstraction that maintains
  the invariant. The comment and wrapper do not substitute for proof.
- **RUST-QUALITY-04** Reject crate-, module-, and group-wide lint suppression
  used to hide ordinary debt. Use the narrowest justified `expect` or `allow`
  with a reason. Never enable all `clippy::restriction`; adopt `pedantic` only
  deliberately, with reviewed scoped exceptions, or cherry-pick its lints.
- **RUST-QUALITY-05** Give public APIs meaningful errors, applicable common
  traits, and `Errors`, `Panics`, or `Safety` documentation only when the
  contract has those conditions. Do not require examples on every DTO.
- **RUST-QUALITY-06** Treat rustdoc, doctests, rustc, Clippy, tests, and
  formatting as distinct evidence. Do not copy exact commands from Verify or
  duplicate rustfmt style rules.
- **RUST-QUALITY-07** Distinguish explicit Cargo policy, inherited policy,
  effective resolver/feature behavior, and implicit defaults. Absence is a
  finding to classify, not permission to invent policy.
- **RUST-QUALITY-08** Analyze MSRV, lints, features, resolver, and publish
  posture without changing manifests, dependencies, lockfiles, release state,
  or toolchains unless that implementation is within the user's authorized scope.
- **RUST-QUALITY-09** Keep module trees, visibility/re-export design, test
  placement, supervisors, transactions, and structure-only work in Rust
  Architecture.
- **RUST-QUALITY-10** Keep native OS facts in OneCrew Native CLI and cryptographic
  invariants/vendor compatibility in X Chat Crypto Vendor; pair this Skill
  only for generic Rust proof and API quality.
- **RUST-QUALITY-11** Route general comment truth to Code Commentary. This
  Skill owns the Rust-specific need for `SAFETY`, `Errors`, `Panics`, and
  safety-condition documentation.
- **RUST-QUALITY-12** Use Verify for exact commands, evidence identities, and
  final execution; readiness never proves product code quality.

## Scenario Router

```rust
// ❌ BAD: the owner discards variants and context into its public model.
fn load_record(id: RecordId) -> Result<Record, String> { /* ... */ }

// ✅ GOOD: callers need variants, so the owner keeps them and maps once.
fn load_record(id: RecordId) -> Result<Record, LoadRecordError> { /* ... */ }
fn to_wire(error: LoadRecordError) -> AdapterError { /* ... */ }
```

```rust
// ❌ BAD: broad suppression hides unrelated debt and never expires.
#![allow(dead_code)]

// ✅ GOOD: one audited exception is narrow, named, and observable.
#[expect(dead_code, reason = "constructed only by the generated registry")]
struct RegistryMarker;
```

```rust
// ❌ BAD: the keyword is treated as the proof and raw bytes escape.
let bytes = unsafe { std::slice::from_raw_parts(ptr, len) };

// ✅ GOOD: an owning guard and checked range support one proved operation.
let buffer = NativeBuffer::read()?;
if buffer.len() > isize::MAX as usize { return Err(NativeError::BufferTooLarge); }
// SAFETY: buffer owns one non-null, aligned allocation of `len` initialized
// bytes; the checked range cannot wrap, no mutation occurs during this borrow,
// and Drop keeps the allocation live until the slice is copied.
let bytes = unsafe {
    std::slice::from_raw_parts(buffer.as_ptr(), buffer.len())
}.to_vec();
```

```rust
// ❌ BAD: ordinary caller-controlled failure is asserted away.
let input = parse_request(raw).unwrap();

// ✅ GOOD: production propagates recovery; the fixed fixture states its invariant.
let input = parse_request(raw)?;
#[test]
fn fixture_is_valid() {
    parse_request(FIXTURE).expect("checked-in request fixture must remain valid");
}
```

```rust
// ❌ BAD: missing fields are silently declared to be project policy.
let policy = WorkspacePolicy::implicit_defaults();

// ✅ GOOD: analysis separates declarations, inheritance, and unresolved gaps.
let report = WorkspacePolicyReport::from_manifests(&metadata)?;
report.require_explicit_decision(PolicyArea::MsrvLintsAndPublish)?;
```

## Router

- Typed errors, panic recovery, boundary mapping, unwrap/expect context:
  [Error, Panic, and Boundaries](references/error-panic-and-boundaries.md)
- Unsafe blocks, proof obligations, safe wrappers:
  [Unsafe Proof and Safe Abstractions](references/unsafe-proof-and-safe-abstractions.md)
- Public API, rustdoc, doctests, and applicable common traits:
  [API, Rustdoc, and Traits](references/api-rustdoc-and-traits.md)
- Cargo lints, suppression, features, resolver, MSRV, and publish analysis:
  [Cargo, Lints, Features, and MSRV](references/cargo-lints-features-and-msrv.md)

## Related Skills

- onecrew-rust-architecture — hand off module/API-path structure,
  visibility, test placement, supervisor lifecycle, and transactions.
- onecrew-native-cli — pair when generic unsafe proof sits inside
  OS-specific mechanics whose native contract remains its owner.
- onecrew-x-chat-crypto-vendor — pair when quality work touches crypto
  code but wire, storage, binding, or vendor behavior remains its owner.
- onecrew-code-commentary — pair when a Rust-specific proof or failure
  contract also changes a general mechanism claim in comments or owner docs.
- onecrew-verify — select and run focused or final evidence.

## Workflow

1. Classify the exact quality surface, caller boundary, and adjacent owner.
   For skill maintenance or a readiness diagnosis, run
   `node .skillshare/skills/onecrew-rust-quality/scripts/check-readiness.mjs`.
   It checks anchors/assets and reports dirty paths; it does not infer intent
   or prove code quality. Existing authorized work is not a blocker.
2. Inspect overlapping changes before editing; preserve unrelated work and
   resolve only ambiguity that prevents the requested change from proceeding.
3. Read only the routed reference and inventory existing contracts and policy.
4. Make the narrowest behavior-preserving quality change or return a scoped
   recommendation when implementation was not authorized.
5. Hand structural, native, crypto-vendor, product, persistence, and release
   decisions to their owners.
6. Use Verify for focused checks and one final owner evidence identity.

## Reporting

Report the affected contract, the narrow change or recommendation, and relevant
verification. Include unresolved policy, unsafe obligations, or blockers only
when the task exposes them; no fixed report packet is needed for a local fix.

## When NOT to use

- Do not use it for a module split, visibility/re-export design, test move,
  async supervisor, lease, transaction, or structure-only refactor.
- Do not use it to change product/domain behavior, persisted formats, native OS
  mechanics, cryptographic vendor behavior, dependencies, or release state.
- Do not use it merely to run Cargo, Clippy, rustdoc, tests, or final gates.
