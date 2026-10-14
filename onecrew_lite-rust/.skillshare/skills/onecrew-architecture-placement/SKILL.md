---
name: onecrew-architecture-placement
description: Choose a runtime, crate, trust, or lifecycle owner when placement is unresolved or changes. Excludes implementation within a known owner and structure-only Rust refactors.
---

# OneCrew Architecture Placement

## Goal

Choose one canonical owner and the smallest justified boundary before
implementation begins.

## Hard Constraints

- **PLACEMENT-01** Always read the stable system map and affected public facades before assigning ownership.
- **PLACEMENT-02** Always select owners by dependency, protocol, trust, lifecycle, persistence, or reusable-facade evidence; never by file length or naming symmetry.
- **PLACEMENT-03** Keep `creator-runtime` as the Creator application owner,
  `platform-runtime` as the shared local-workflow owner for operator sessions,
  governed provider dispatch, data-directory admission, and secret-free
  public projections, and `onecrew-codex` as the Codex CLI protocol,
  process-supervision, and plugin-staging owner.
- **PLACEMENT-04** Keep secrets in `local-vault` and expose only opaque references or safe projections outside that boundary.
- **PLACEMENT-05** Keep provider mechanics in `transport -> client -> platform facade` and platform behavior (input constraints, native models, protocol-native failure evidence) in the platform crate; `platform-sessions` builds sessions from stored secret material. Runtime owns admitted toolkit functions over one platform-independent read orchestration, fresh save preparation, and effect composition, while Creator owns canonical policy and persistence. Runtime implements the retained Email gateway and conversation read port using shared credentials/sessions. Do not add a candidate cache, cross-platform selection framework, per-platform failure classification, or replacement adapter layer (ADR 0017).
- **PLACEMENT-06** Keep HTTP and MCP as peer adapters over the same runtime use case; never route one through the other.
- **PLACEMENT-07** Introduce ports, repositories, adapters, or crates only for an evidenced boundary with a real caller.
- **PLACEMENT-08** Assign one owner for every invariant, transaction, durable fact, and lifecycle; consumers must not restate its policy.
- **PLACEMENT-09** Treat current source, tests, manifests, and owner documentation as executable placement evidence; never freeze a full module inventory here.
- **PLACEMENT-10** Stop placement work when the owner is unambiguous and hand implementation to the owning workflow.

## Workflow

1. Read `ARCHITECTURE.md`, the affected facade roots, and
   [Capability Placement and Wiring](references/capability-placement-and-wiring.md)
   for the owner table, boundary evidence, and canonical call chains.
2. Classify the proposed boundary by dependency, protocol, trust, lifecycle,
   persistence, and reuse.
3. Record the canonical caller-to-owner path, transaction owner, secret
   boundary, durable state, and peer-adapter impact.
4. Reject speculative layers and select the smallest existing owner that can
   preserve the invariant.
5. Name any related skill that must accompany implementation.
6. Verify that no second owner or adapter-local policy was introduced.

## Output Contract

- Owner and boundary evidence
- Canonical call and dependency path
- Trust, lifecycle, persistence, and transaction owners
- Related skills needed for implementation
- Rejected alternatives and reason

## Related Skills

- onecrew-rust-architecture — Rust module, API, supervisor, or structure changes.
- onecrew-peer-adapters — HTTP or MCP projections change after placement is decided.
- onecrew-local-data-evolution — A persisted shape or durable version changes.
- onecrew-native-cli — Native OS mechanics or distribution changes.

## When NOT to use

- Do not use it for a localized change inside an already established owner.
- Do not use it as the detailed Rust module/API workflow; use
  `onecrew-rust-architecture`.
- Do not use it as the implementation owner for Creator, providers, adapters,
  persistence evolution, UI, or native distribution.
