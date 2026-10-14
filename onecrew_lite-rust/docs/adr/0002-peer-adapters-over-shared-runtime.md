# ADR 0002: Peer adapters over a shared runtime

## Status

Accepted.

## Context

Browser HTTP, CLI/control IPC, and MCP expose different protocols to different
callers, but all need the same business behavior. Letting one adapter call the
other would make protocol availability part of the application layer and would
encourage duplicate validation and authorization rules.

## Decision

HTTP and MCP/control adapters are peer inbound adapters over
`platform-runtime::LocalPlatform` and `crm-runtime::CrmRuntime`. Neither
adapter calls the other. The shared Web UI uses typed HTTP and never uses MCP
as an internal API; the CLI uses the same service owners through its explicit
commands and control IPC.

Adapters translate protocol DTOs, acquire their lifecycle scope, invoke typed
application use cases, and map typed results. Provider clients and raw provider
methods are not exposed as a generic adapter surface.

## Consequences

- A use case has one application implementation regardless of caller.
- Secret, confirmation, idempotency, and audit rules cannot differ by adapter.
- Adapter tests focus on protocol mapping and surface policy.
- New inbound protocols integrate at the same application boundary rather than
  being tunneled through an existing adapter.

## Amendment (2026-08-19)

This is a historical snapshot of the retired native host.

"Neither calls the other" is a protocol claim: neither adapter invokes the
other's endpoint, and the Web UI never calls the MCP endpoint.

The then-current desktop also linked `mcp-adapter` as a library for shared
secret-free form projections and embedded templates. That code sharing did not
turn either adapter into the other's protocol client.

## Amendment (2026-08-27)

The generic-inspector cutover retired the campaign/task/choice form projection
builders; they were not a compatibility boundary. The desktop then linked
`mcp-adapter` to own the loopback server lifecycle and serve the adapter's
exact-current embedded App template. The retired desktop host's actions were
closed typed Tauri peers over the same runtime owners: they did not call the
public generic executor or derive a Tauri command from an operation ID.
