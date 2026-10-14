# Architecture

This is the stable system map and decision index for OneCrew (Agent Platform Local).
Current interfaces, schemas, state machines, inventories, and failure details
live with their executable owners.

## System shape

OneCrew is one Rust CLI executable with embedded React and plugin assets.
`crates/onecrew` parses commands and delegates to the independent
`crates/local-service` application host. That host owns startup, shared runtime,
authenticated human HTTP, raw MCP/control IPC, and graceful drain.

The optional Workspace download pairs this CLI with the independent Library
executable. Its explicit `workspace` launcher obtains each service's ready URL
and opens the workbench and Library in the system browser. It does not own
accounts, tables or Campaigns, change data profiles, or run searches; both
services retain their existing startup, lock and authentication owners. Packaging
is described in `development/scripts/WORKSPACE_PACKAGE.md`.

```text
React webpage -> typed HTTP ----+
                                +-> shared runtimes -> provider facades
CLI / agent -> raw MCP / IPC ---+         |                 |
                                         +-> Creator/vault +-> clients/transports
CLI explicit plugin commands -> minimal codex integration
```

HTTP and MCP are peer inbound adapters. They translate their protocols into
the same runtime use cases; neither calls the other protocol endpoint. Provider
protocol mechanics remain below application-facing facades.

## Dependency direction

Dependencies point inward:

1. Transports own provider protocol, authentication headers, network policy,
   retry, signing, and provider error decoding.
2. Clients own typed provider operations and provider decoding.
3. Provider facades expose application-facing capabilities.
4. `platform-runtime` implements Creator's Email gateway and conversation read
   port using its shared credential and session owners and the provider facades.
5. `creator-runtime` owns Creator use cases, canonical DTOs, repository ports, and
   business persistence.
6. `creator-sync` owns browser-pairing enrollment, the strict Worker B protocol,
   vault credential handoff, and quarantined Creator snapshot synchronization.
7. `platform-runtime` composes vault, Creator runtime, Creator Sync, providers,
   operator accounts, and service credentials.
8. `codex` isolates explicit plugin packaging/install/status/setup from application
   runtimes. Explicit Web-only discovery draft preparation in
   `platform-runtime/src/discovery_draft/codex.rs` invokes the locally signed-in
   Codex CLI for tool-free structured planning under bounded admission. Shared
   intent validation and approval remain authoritative; MCP does not recursively
   launch Codex. Plugin lifecycle remains owned by the `codex` crate.
9. HTTP and MCP project the same owners; React remains presentation state.

A platform crate owns its platform's behavior: input constraints,
platform-native models and protocol-native failure evidence.
`platform-sessions` is the one place that builds platform sessions from stored
secret material, including egress rules such as Telegram's SOCKS5 requirement;
session management stays with its current owners. Shapes shared across
platforms live in contract crates, and a trait
exists only for an operation several platforms implement. The runtime writes
platform-independent orchestration once. HTTP and MCP own their wire; they do
not classify platform failures and do not depend on platform crates. See
[ADR 0017](./docs/adr/0017-platform-behavior-lives-in-platform-crates.md).

## Owner boundaries

| Owner | Stable responsibility |
|---|---|
| `creator-runtime` | Creator use cases, public Creator DTOs, persistence, confirmation, idempotency, and audit |
| `creator-sync` | Browser-pairing enrollment, transient installation credentials, strict Worker B HTTP, vault handoff, and quarantined snapshot lifecycle |
| `platform-runtime` | Application composition, local account and credential workflows, governed dispatch, and the immutable capability catalog |
| `platform-sessions` | Vault-backed profile restoration and shared provider-session reuse |
| `schema-manifest` | Canonical JSON Schema, JavaScript-safe integer bounds, and stable contract identities |
| `codex` | Explicit plugin packaging/install/readback, cached status/refresh, and setup |
| `mcp-adapter` | Authenticated loopback transport, MCP schemas/bindings/execution/search/guide, and secret-free Agent-facing projection |
| `onecrew` | Thin CLI, embedded web/plugin assets, and explicit integration commands |
| `local-service` | Application host, HTTP and raw MCP/control IPC, startup, readiness, lifecycle and drain |
| MCP App UI | Isolated iframe source, Host SDK presentation state, strict result client, and self-contained bundle |
| `local-vault` | Encrypted operator profiles and service credentials |
| Provider facades | Application-facing provider capabilities without UI or persistence ownership |
| Provider clients/transports | Provider protocol implementation and typed provider decoding |

The runtime capability catalog is an immutable identity and navigation inventory,
without schemas or execution handlers. MCP binds its protocol operations to that
catalog; HTTP declares its own routes to typed application owners. A directory
entry does not create an executable HTTP route.

Rust owners define public contracts. Selected adapter manifests generate
TypeScript artifacts through `development/schema-codegen`; this is not a claim
that every DTO is generated. The adapter's codegen configuration defines the
covered roots and output, and `schema-manifest` does not own TypeScript output
or adapter behavior.

## Trust and persistence

Creator business records and encrypted secrets are separate stores. Creator retains
public data and opaque references; only Rust decrypts a credential for the
operation that needs it. HTTP and MCP outputs remain secret-free.

External writes require trusted confirmation or grant, stable idempotency,
readback where supported, and preserved unknown-effect evidence. The loopback
adapter is authenticated; setup capabilities never substitute for its durable
runtime credential.

Persisted formats evolve through adjacent upgrades. Runtime open validates only.
After exclusive owner/old-process checks, local-service startup runs read-only
inspection and any required supported upgrade with backup, adjacent steps,
validation, and typed recovery. Compatible data starts directly; a changed binary
alone does not trigger an upgrade. Inspecting/Upgrading/Ready/Failed projects the
startup state. Business requires Ready; limited MCP/status remains available
before Ready. Failure blocks business even after partial commits.

There is no manual upgrade command, HTTP/MCP route, or webpage trigger. Data
status is read-only. Stable business data under io.onecrewai.onecrew is separate
from plugin caches. Historical directory relocation remains an explicit offline
procedure. A running binary mismatch requires finishing work and stopping the old
service before restart; automatic hot replacement is not implemented.

## Lifecycle

Local-service owns its complete process/runtime graph, startup upgrade work,
background supervisors, authenticated intake, and bounded shutdown. The CLI can
start/reuse it without a browser. Cancellation does not drop the actual owner of
ongoing filesystem, provider, or subprocess work.

Creator repository reads, writes, and CPU work have separate bounded admission
before `spawn_blocking`. Read capacity matches the SQLite reader pool; writes
are serialized. Locks and permits remain with the complete blocking operation,
and provider I/O does not run under a repository lock. Overload and shutdown
are typed dispatch failures; a lost running result is not proof of no effect.

Detailed lock order, upgrade sequencing, health criteria, and recovery behavior
belong to owner source/tests, owner READMEs, and the relevant Skill reference.

## Owner documentation

- [Creator runtime](./crates/creator-runtime/README.md)
- [Platform runtime](./crates/platform-runtime/README.md)
- [Codex integration](./crates/codex/README.md)
- [MCP adapter](./crates/mcp-adapter/README.md)
- [Web application](./apps/web/README.md)
- [MCP App UI](./apps/mcp-ui/README.md)

## Architecture decisions

- [ADR 0001: Native process and loopback MCP](./docs/adr/0001-native-process-and-loopback-mcp.md)
- [ADR 0002: Peer adapters over a shared runtime](./docs/adr/0002-peer-adapters-over-shared-runtime.md)
- [ADR 0003: Separate business and secret storage](./docs/adr/0003-separate-business-and-secret-storage.md)
- [ADR 0004: Explicit forward local-data evolution](./docs/adr/0004-explicit-forward-local-data-evolution.md)
- [ADR 0005: Vendored X Chat cryptography](./docs/adr/0005-vendored-x-chat-cryptography.md)
- [ADR 0006: MCP contract evolution and fixed surfaces](./docs/adr/0006-mcp-contract-evolution-and-fixed-surfaces.md)
- [ADR 0007: Operator skills guidance layer](./docs/adr/0007-operator-skills-guidance-layer.md)
- [ADR 0008: Codex plugin and MCP Apps overlay](./docs/adr/0008-codex-plugin-and-mcp-apps-overlay.md)
- [ADR 0010: The ledger is the only scheduler](./docs/adr/0010-the-ledger-is-the-only-scheduler.md)
- [ADR 0011: Email threads follow RFC 5322, not the recipient](./docs/adr/0011-email-threads-follow-rfc-5322.md)
- [ADR 0012: Stateless MCP operation registry](./docs/adr/0012-stateless-mcp-operation-registry.md)
- [ADR 0013: Result-first MCP tool router](./docs/adr/0013-result-first-mcp-tool-router.md)
- [ADR 0014: Automatically run preflight-approved local-data upgrades](./docs/adr/0014-automatic-safe-local-data-upgrades.md)
- [ADR 0015: Toolkit-grid reads and criteria-only search plans](./docs/adr/0015-toolkit-grid-reads-and-criteria-only-plans.md)
- [ADR 0016: Neutral capability catalog and typed peer adapters](./docs/adr/0016-neutral-capability-catalog-and-typed-peer-adapters.md)
- [ADR 0017: Platform behavior lives in platform crates](./docs/adr/0017-platform-behavior-lives-in-platform-crates.md)

## Documentation rule

Keep this file stable and small. Put decision rationale in ADRs, current owner
interfaces and failure behavior in owner READMEs, agent workflows in Skills,
machine routing in the executable Skill graph, and changing schemas,
inventories, provider captures, and release facts in source, tests, manifests,
or dated evidence.
