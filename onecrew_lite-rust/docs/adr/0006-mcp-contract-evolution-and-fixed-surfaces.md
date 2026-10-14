# ADR 0006: MCP contract evolution and fixed surfaces

## Status

Accepted in part. The runtime ownership and safety decisions remain accepted;
ADR 0012 supersedes the flat business-tool manifests, handwritten workflow
catalog, and generic-executor prohibition.

## Context

The authenticated MCP/HTTP adapters expose typed application workflows, but the current
surface mixes bounded workflow entry points with unbounded aggregate reads,
caller-reconstructed persisted snapshots, and external-write continuations
that do not all bind one exact intent.

Adding more tools or hiding tools dynamically would not correct those runtime
contracts. It would also make tool discovery depend on mutable MCP session
state even though application continuity already belongs to runtime flow IDs,
operation IDs, idempotency records, and durable business state.

The current main branch deliberately excludes the previously developed Tasks
and Agent Jobs surface. RMCP protocol Tasks and any future product task runtime
are separate capabilities and must not be introduced by an MCP transport or
surface-maintenance change.

## Decision

MCP contract changes begin in the owning application runtime. `crm-runtime`
owns CRM reads, writes, confirmation, idempotency, remote-effect state, audit,
and persistence. `platform-runtime` owns operator sessions, account and
credential workflows, and their secret-free public state. `local-vault` owns
durable secrets. HTTP and MCP remain peer inbound adapters over those owners;
the OneCrew CLI invokes the same owners through `local-service`.

Growing collections use bounded summary pages with opaque cursors. Stable
resources expose keyed detail reads. A caller refers to persisted or discovered
complex resources by stable IDs instead of resubmitting internal snapshots or
provider envelopes.

External writes bind explicit confirmation and idempotency to one frozen,
fingerprinted intent. Provider execution and readback use typed, recoverable
effect states, including an explicit unknown-effect state when completion
cannot be proven. Conversational credential input is a deliberate product
capability: connection and credential tools accept raw secrets as MCP tool
inputs over the authenticated loopback boundary. The enforced invariant is
that secrets never leave: secret-bearing inputs stay deserialize-only, every
error path is redacted, and secret material appears in no output projection,
log, audit, fixture, or persisted store other than `local-vault`.

The repository follows its clean current-state rule. Tool aliases, fallback
input shapes, duplicated DTOs, or a parallel legacy business model require an
explicit, evidence-backed compatibility requirement. In their absence,
consumers migrate together at one typed contract boundary.

MCP navigation uses fixed, URL-selected surfaces over one shared runtime and
authentication boundary:

- a complete `/mcp` surface;
- a compact surface containing complete workflow continuation closure; and
- fixed domain surfaces for accounts, campaigns, creators, outreach, and mail.

Every canonical endpoint uses exact routing. Unknown paths fail instead of
falling back to a broader surface. A read-only workflow catalog describes
entry tools, required continuations, prerequisites, effects, and available
surfaces. It is adapter-owned navigation metadata, not a generic executor or a
runtime scheduling allowlist. Tool availability does not change dynamically
within a connection.

RMCP transport upgrades remain separate from application contract changes.
The server remains tools-only unless a later ADR authorizes another protocol
capability. Modern protocol health and legacy compatibility are verified by
separate tests so fallback cannot mask a failed modern path.

Exact tool inventories and changing schemas remain executable source and test
facts rather than ADR content. The surface baseline tests
(originating as the Phase 0 pre-migration baseline) pin the currently accepted
contract and are intentionally updated with each accepted contract change.

## Consequences

- An MCP or HTTP projection correction may require a canonical runtime DTO and
  matching adapter updates; an adapter cannot invent a second read model.
- Persisted revisions, prepared operations, fingerprints, effect states,
  cursors, strategies, markers, or indexes require explicit local-data
  evolution before implementation.
- Compact and domain endpoints cannot ship until exact routing, shared state,
  authentication coverage, and workflow-closure tests exist.
- Secret-accepting connection tools keep deserialize-only inputs and redacted
  error paths on every surface that includes them.
- A future product task runtime requires its own owner and contract work; the
  RMCP Tasks extension is not enabled as an accidental substitute.
- A generic untyped operation executor and dynamic tool loading are not
  supported architecture. The public `onecrew_tool_execute` route accepts only
  exact registry schemas and typed owner bindings.


## Amendment (2026-08-19)

The product task runtime this ADR deferred was later retired. Task tables,
task-run scheduling, and the former `crm_task_*` public family are historical
plan material and are not part of the current OneCrew CLI, shared Web, or MCP
surface. The RMCP Tasks extension remains unused. ADR 0012's exact-schema,
typed-binding stateless operation executor is the only generic route; arbitrary
or untyped execution remains prohibited.

## Amendment (2026-09-10)

The `/mcp/domains/campaigns` surface and the Campaign owner it served were
retired on 2026-09-10 with Creator schema v36 (release 0.6.0). The fixed domain
surfaces are now accounts, creators, outreach, and mail; exact routing returns
404 for the retired path. The Creator owner named `crm-runtime` above is
`creator-runtime`. The Choice operation `task.choice.request` became
`onecrew.ask_user_question` on 2026-09-09. The decision body above is a dated
record and is not rewritten.
