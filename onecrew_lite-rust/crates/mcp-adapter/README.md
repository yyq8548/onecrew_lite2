# MCP adapter

`onecrew-mcp-adapter` is the authenticated loopback MCP inbound adapter.
It projects high-level Creator and operator-account use cases supplied by the
shared application runtimes as a fixed meta protocol with stateless catalog
reads and bounded process-local typed results.

## Registry responsibilities

The registry's module entries assemble narrow private owners:

- `operation_registry/registration/` separates declaration policy, builder
  accumulation, cross-entry validation, and the sealed registry/search index.
- `operation_registry/execution/` separates typed binding, blocking dispatch,
  failure projection, and the preflight/approval/invocation sequence.
- `operation_registry/v2/contract/` owns each wire contract family;
  `v2/projection/` owns call, batch, choice, terminal, and exact-value projection.
- `domains/connections/` owns provider connection workflows and directories;
  `domains/platform_reads/` owns platform read bindings, their schemas and the
  shared read failure projection.
- Invariant tests remain beside their source owners. Shared private fixtures
  live in parent test support, and aggregate tests are grouped by the behavior
  they prove rather than collected into one registry-wide file.

`contract_snapshot_tests` can explicitly export complete declarative registry
metadata, policies, schemas/revisions, guides, playbooks, Get projections and
outer tool schemas via `MCP_REGISTRY_SNAPSHOT_OUT`. It builds a temporary local
fixture without invoking operations or providers; compare the resulting JSON
before and after structural moves alongside the behavior tests.

## Public surface

`OperationRegistryHandle::build(platform, catalog)` compiles the registry once
per application graph from an explicit shared `CapabilityCatalog`. Catalog
operation declarations own identity, toolkit, title and summary; MCP supplies
protocol scopes, policies, schemas and typed bindings. Sealing requires exactly
one binding for every declaration supporting MCP and rejects dangling bindings.
`LocalMcpService::new` and `McpAdapter::start` take that handle with the
shared runtime; `start` also takes server options and returns an
`McpAdapterHandle`, which exposes the endpoint, completion observation,
on-demand setup prompt issuance, and orderly stop. The handle's
`toolkit_directory` is the MCP navigation projection. HTTP navigation reads the
shared runtime catalog directly. `probe` performs protocol-level health verification.

Every ordinary endpoint exposes the same five model-visible tools:
`onecrew_tool_search`, `onecrew_tool_get`, `onecrew_tool_execute`, `onecrew_result_present`,
and `onecrew_connection_list`. The additional App-only `onecrew_result_state` tool is
hidden from the model and projects the authoritative process-local result
owner. Toolkit-owned
registered operations retain exact typed contracts and bind directly to
`OperatorAccountWorkflow`, `CreatorRuntime`, or the owning runtime service; the
generic executor never passes an untyped provider envelope below the adapter.

Tool search accepts one independent outcome in normalized English. The caller
translates non-English intent before the call, splits unrelated actions into
separate searches, and passes named toolkits as explicit filters. Registry
construction derives separate immutable in-memory Tantivy indexes for operations
and workflow-hint sources using English stop words, stemming, BM25, and boosted registry
fields. Exact IDs bypass lexical ranking; surface, toolkit, domain, and effect
filters remain authoritative registry checks and are enforced again after
recall. The index has no persisted files, model, network request, session state,
or downloadable artifact.

Navigation uses fixed, URL-selected surfaces over one shared runtime,
listener, QR store, and bearer boundary: the complete `/mcp` surface,
`/mcp/compact` for the compact allowlist, and
`/mcp/domains/{accounts,creators,outreach,mail}`. Routing is exact
and unknown sub-paths return 404. Every ordinary surface pins the same
five-tool model-visible manifest per connection while Search/Get
filters entries by that
surface's registry allowlist. The documentation-only `/mcp/skills` surface is
read-only and exposes only Search/Get. Search always returns compact complete
toolkit guides for represented or inferred toolkits plus bounded workflow
hints; those hints are navigation, never executable Plan objects. Get returns
exact current contracts for one to eight operation IDs. Execute prevalidates a
whole batch before invoking an owner and mints a `result_ref` only when user
input or an explicit presentation was requested. Only `onecrew_result_present`
mounts that execute-minted result. Exact inventories, closure invariants,
and schema pins live in the adapter's tests, not here.

Attended confirmation cards recursively project every submitted scalar leaf,
including nested objects and arrays. Projection is
all-or-error: unsafe fields or runtime-bound overflow stop before any owner is
invoked. Requested terminal cards similarly project the complete bounded safe
result and server-derived resource references. If an owner has already run but
the optional result projection or mint cannot complete, `onecrew_tool_execute`
still returns the ordered operation outcomes and a closed
`presentation_failure`; it never hides the business outcome behind a generic
card error.

## Server state

The adapter itself owns one running server handle. Desktop owns the richer
start/recovery/degraded/faulted lifecycle around it. `platform-runtime` owns
the bounded result map behind every MCP Apps Host; a process restart,
expiry, or capacity eviction makes the old card read-only instead of restoring
pending authority from a transcript or browser store.
After a confirmation submit CAS, a synchronous terminal guard closes the result
on cancellation or panic: before owner dispatch it becomes a known-no-effect
failure, and once dispatch may have begun it becomes unknown rather than
remaining submitted indefinitely.

Setup is a separate capability state machine. A capability is issued only when
the healthy Desktop connection surface requests it, expires after a bounded
window, and is consumed atomically by one trusted terminal request. Consuming
or expiring it permits issuance of a new capability without replacing the MCP
bearer or restarting the server.

The setup URL never embeds the persistent bearer. The returned setup document
contains the Codex plugin install path plus generic connection facts for other
MCP clients; the bootstrap prompt remains short and instructs the Agent to
fetch it through host loopback networking.

## Storage and trust boundary

The adapter owns no business database, persisted interaction store, vault, browser
profile, provider session, or durable account state. It receives a bearer from
Desktop and protects the loopback MCP route with it. Setup requests use a
separate path, capability, host/origin policy, and trusted-request marker.

Structured results remain secret-free. Inputs may contain a secret only when a
typed application workflow requires it; the adapter delegates consumption and
does not persist or echo it.

## Failure model

Startup returns an error if binding or serving fails. Health probe errors
distinguish transport, HTTP, protocol, and tool-discovery failure. Task
completion observation distinguishes explicit stop, cancellation, unexpected
exit, and failure so the Desktop controller can decide whether recovery is
appropriate.

Tool failures preserve typed application failures where defined and otherwise
return bounded, secret-free errors. Platform read bindings report the runtime's
`PlatformReadError` through one projection in `domains/platform_reads/` that
follows protocol semantics and never branches on the platform. Local outcomes
keep fixed public codes. A platform failure without any platform response is
`network_unreachable` and recovers through the live connection check of the
toolkit the binding declares. Every other platform failure is
`provider_read_failed`: HTTP status or RPC code 401 reconnects, and 420, 429 or
5xx retries once later. A platform failure is retry-safe unless it follows a
local commit, which reports an applied effect, and it carries the platform's
evidence as `provider_evidence` in the direct-message diagnostic shape. The
frozen presentation fixture beside the platform read tests pins the exact
rows. Bindings inspect no platform, client or transport error. They use only
what the runtime re-exports from a platform: data and input types such as the X
advanced search criteria and the Telegram community locator, those types' own
validation methods, constraint constants and `accepts_*` checks. Reads go
through runtime toolkit functions, never a platform facade or session.
Raw provider bodies, request URLs, cookies, proxy values, and transport prose are
discarded below the adapter boundary. A listening socket is not success, and
browser or desktop automation is not a setup fallback.

## Dependency rules

- Keep business logic in `creator-runtime` or `platform-runtime`.
- Register only high-level typed operations backed by an owning runtime use
  case; do not expose raw domain or provider methods.
- Do not read vault files or instantiate provider clients here.
- Do not depend on platform crates; use only the platform data and input
  types, their validation methods, constraint constants and `accepts_*` checks
  that the runtime re-exports, and read through runtime toolkit functions.
- Keep persistent bearer generation and server lifecycle ownership in Desktop.
- Verify surface changes through MCP initialization and complete tool discovery.

See the root [architecture map](../../ARCHITECTURE.md), the
[native-process ADR](../../docs/adr/0001-native-process-and-loopback-mcp.md), and
the [peer-adapter ADR](../../docs/adr/0002-peer-adapters-over-shared-runtime.md).
Contract evolution, fixed surfaces, and clean migration boundaries are governed
by [ADR 0006](../../docs/adr/0006-mcp-contract-evolution-and-fixed-surfaces.md).
The workflow-hint layer and the single bundled Codex router are governed by
[ADR 0007](../../docs/adr/0007-operator-skills-guidance-layer.md); the stateless
MCP delivery contract is governed by
[ADR 0013](../../docs/adr/0013-result-first-mcp-tool-router.md).
