# Peer Adapter Feature Packet

## Intent and stop

- User intent:
- Explicit exclusions:
- Stop condition:

## Source truth

- Runtime owner and typed use case:
- Current HTTP projection:
- Current MCP registry/surface projection:
- Nearest tests and manifests:

## Contract

- Canonical input/result:
- HTTP input/output mapping:
- MCP operation ID, schemas, toolkit, surfaces, and connection:
- Effect, confirmation, idempotency, readback, and audit semantics:
- Failure and recovery semantics:
- Secret-bearing inputs and secret-free public projections:

## Generated contract pipeline

- Explicit consumer roots and Serde direction:
- Canonical schema identity and pinned generator dependencies:
- Consumer-local config, manifest, types, and validators:
- Determinism and forbidden environment-dependent content:
- Staging validation, atomic replacement, and rollback path:
- Non-mutating drift check and generator failure-path tests:
- Handwritten normalization and semantic refinements retained outside generated files:

## Authority and UI boundary

- Direct Agent/hook authority:
- Explicit App presentation:
- App-only interaction state:
- React/widget work routed elsewhere:

## Implementation

1. Confirm the typed runtime owner exists.
2. Change the smallest adapter mapping and registry responsibility.
3. Add branch-adjacent schema, closure, failure, replay, and parity tests.
4. Keep generated resources and frontend consumers in their owners.

## Evidence

- Focused implementation checks:
- Final evidence identity:
- Reused evidence, if any:
- Blocked native/live/provider lanes:
- Review identity or reuse decision:

## Critical refusals

- No GUI-through-MCP route.
- No adapter-local business model or generic provider envelope.
- No secret projection or unconfirmed external write.
- No compatibility alias, legacy wrapper, or generic JSON HTTP executor.
- No hand-edited generated artifact, partial output, nondeterministic content,
  ordinary-build source generation, or cross-consumer generated import.
