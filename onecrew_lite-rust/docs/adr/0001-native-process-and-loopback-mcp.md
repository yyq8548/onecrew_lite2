# ADR 0001: OneCrew CLI host and authenticated Web/MCP adapters

## Status

Accepted.

## Context

The shared Web UI, the OneCrew CLI, and connected agents need the same accounts
and CRM capabilities. Running a separate worker or remote service would
introduce another lifecycle, secret boundary, and source of state divergence
for a local-first product.

## Decision

The OneCrew CLI executable and its `local-service` host own the process. Rust
platform engines, authenticated HTTP for the shared Web UI, and raw MCP/control
IPC run in that host. The HTTP listener uses the service's local bearer
authentication; `onecrew serve` starts and drains it, and `onecrew mcp --stdio`
exposes the MCP adapter for an agent.

`local-service` owns construction, health supervision, recovery, and shutdown
of the shared runtime and MCP adapter. Listening is insufficient evidence of
health: the controller uses a protocol handshake and tool-discovery probe.

There is no remote MCP service, Python runtime, or worker subprocess in the
supported architecture.

## Consequences

- The shared Web UI, CLI, and agents share one runtime graph and one local-data
  lock.
- Service shutdown must cancel and join MCP/HTTP work before releasing runtime
  state.
- Local HTTP, stdio MCP, and bearer checks are security boundaries, not
  deployment hints.
- A future out-of-process design would require a new ADR and an explicit secret
  and lifecycle model.
