# Rust Quality Eval Rubric

## Scoring

- Exact quality surface, recovery owner, and adjacent owner classification: 15
- Contextual typed owner error and stable secret-free boundary projection: 15
- Contextual panic/unwrap/expect decision: 10
- Unsafe proof, narrow block, safe wrapper, and cleanup ownership: 15
- Scoped lint suppression and deliberate lint-group policy: 10
- Public API, applicable traits, rustdoc, and doctest contract: 10
- Cargo declaration/inheritance/default/features/MSRV/publish analysis: 15
- Exact verification delegation, blockers, and stop condition: 10

## Pass Criteria

Score at least 90/100 with zero critical failure. The answer must classify the
requested quality surface, preserve adjacent owner boundaries, teach the
relevant Rust contract, and distinguish a scoped recommendation from
authorized implementation. Do not award points for an evaluator independently
discovering rules absent from the Skill and selected reference.

## Critical Fail Criteria

- Keeps `String` at a public/reusable owner boundary whose callers require
  classification, exposes secrets, or erases a required retry/unknown-effect
  distinction at a public boundary.
- Treats a `SAFETY` comment, wrapper, lint, test, or readiness result as proof
  of soundness or overall code quality.
- Creates a blanket unwrap/expect/panic ban, enables all restriction, treats
  pedantic as mandatory or exception-free, or suppresses a broad lint group
  to pass a gate.
- Requires docs/examples/traits mechanically on every DTO or duplicates
  rustfmt style rules.
- Changes module structure, native mechanics, crypto-vendor behavior, product
  behavior, manifests, dependencies, lockfiles, MSRV, publish, or release state
  without that action being in scope.
- Executes or repeats verification outside the selected Verify evidence lane.
