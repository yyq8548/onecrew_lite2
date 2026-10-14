# Rust Quality Feature Packet

## Intent and classification

- User intent:
- Quality surface: error/panic, unsafe, API/rustdoc/traits, lints, or Cargo policy
- Current recovery or policy owner:
- Adjacent Architecture, Native, Crypto Vendor, Commentary, or product owner:
- Explicit exclusions and stop condition:

## Current contract

- Typed owner error and current boundary projection:
- Panic/unwrap/expect sites and proved invariants:
- Unsafe operations, proof obligations, safe abstraction, and cleanup owner:
- Public API, applicable traits, rustdoc, and doctest surface:
- Lint groups, suppressions, scope, reasons, and toolchain policy:
- Workspace members, inheritance, resolver/features, MSRV, and publish posture:

## Proposed quality delta

- Narrow behavior-preserving change or recommendation:
- Error/source/redaction and wire mapping:
- SAFETY proof and safe-wrapper lifecycle:
- Lint expectation/allow rationale and expiry behavior:
- Applicable public documentation and trait changes:
- Cargo policy gaps, affected packages, and separately authorized follow-up:

## Evidence and stop

- Readiness status and product-domain dirty paths:
- Focused branch/immediate-consumer evidence selected by Verify:
- Final owner evidence identity or reuse source:
- Blocked rustdoc, target, MSRV, native, live, or release lanes:
- Review identity or reuse decision:
- Confirmation that readiness did not grade product code quality:

## Critical refusals

- No owner `String` at a public/reusable boundary that needs actionable
  distinctions, and no secret-bearing public projection.
- No blanket panic/unwrap/expect ban, all restriction policy, or mandatory
  exception-free pedantic policy.
- No unsafe keyword/comment/test treated as proof without a safe abstraction.
- No mechanical docs/examples/traits on every DTO; no copied rustfmt rules.
- No manifest, dependency, lockfile, package, publish, or release mutation
  without separate authorization.
