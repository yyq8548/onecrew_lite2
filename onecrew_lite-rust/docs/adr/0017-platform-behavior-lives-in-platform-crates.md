# ADR 0017: Platform behavior lives in platform crates

## Status

Accepted on 2026-09-15. This record amends two earlier statements without
rewriting them:

- In the 2026-09-13 amendment to
  [ADR 0015](0015-toolkit-grid-reads-and-criteria-only-plans.md): "Platform
  facades own provider semantics, including meaningful read-failure
  classification" and "MCP projects public failure classifications".
- In [ADR 0016](0016-neutral-capability-catalog-and-typed-peer-adapters.md):
  the decision "changes no ... provider transport/client/facade boundary".

The [upper-layer realignment specification](../../.scratch/upper-layer-realignment/spec.md)
tracks delivery.

## Context

Platform knowledge had spread across the upper layers:

- `platform-runtime` repeated one toolkit module per platform: admission,
  session opening, the provider call and per-platform error text.
- Session material was rebuilt from stored secrets in several places, and the
  Telegram SOCKS5 egress rule was repeated in about seven.
- Read failures were renamed at every layer. Each platform crate defined its own
  read-failure categories, the runtime renamed them per platform, and
  `mcp-adapter` classified them again per platform.
- `mcp-adapter` depended directly on the X, Instagram and YouTube facades.
- The direct-message path already carried protocol-native evidence
  (`DmDiagnostic`), but reads did not reuse it.

A previous plan made the runtime the only translation layer, with
runtime-owned carriers and a runtime read-failure classification. That moved
the same knowledge up one layer instead of down. It was rolled back before
implementation.

## Decision

- **Platform crates.** A platform crate owns that platform's behavior: input
  validation and constraint constants, platform-native models and the
  extraction of protocol-native failure evidence.
- **Session construction.** `platform-sessions` is the only implementation
  that builds platform sessions from stored secret material, including egress
  rules such as Telegram's SOCKS5 requirement. Session construction does not
  move into platform crates. Session management stays with its current owners:
  connection admission, account selection, vault access and blocking dispatch,
  the session cache, login flow state, persistence and rollback, and Creator
  synchronization.
- **Shared contracts.** A shape that several platforms share lives in a
  contract crate. Failure evidence for reads and direct messages reuses
  `dm_contracts::DmDiagnostic`: the platform, protocol, origin, native code,
  HTTP status, RPC code and an owner-written message.
- **Failure categories.** OneCrew does not name categories for third-party
  failures. Closed branches remain only at real decision points: write refusal
  versus unknown effect, service-credential health, connection-check status,
  and transport retry.
- **Traits.** A trait exists only for an operation that several platforms
  implement. A single-platform operation stays a concrete platform function.
- **Runtime.** The runtime writes platform-independent orchestration once and
  does not re-implement platform behavior per platform. That orchestration
  covers connection admission, session acquisition, search authorization,
  credential health commits, platform-native data serialization and local
  outcome names.
- **Inbound adapters.** Inbound adapters own protocol schemas and wire. They do
  not classify platform failures per platform and do not depend on platform
  crates. They may use platform data and input types that the runtime
  re-exports; platform methods, sessions and errors stay invisible to them.
- **Scope.** These rules bind new code and any code a change moves. Untouched
  modules migrate when a later change reaches them.

## Consequences

- **Adding a platform read** changes the platform crate, one runtime
  delegation, the MCP binding with its schema, and the catalog declaration. A
  new or finer platform failure changes only that crate's evidence extraction.
- **Public failure presentation** for platform reads follows protocol semantics:
  no response, 401, 429, RPC 420 and 5xx. Accidental per-platform differences in
  that presentation disappear. A YouTube failure after a committed credential
  health mark keeps its applied effect.
- **Enforcement** comes from one guard, the Cargo dependency allowlist of
  `mcp-adapter`. No public-surface reachability lint is added. Review and owner
  tests cover the rest.
