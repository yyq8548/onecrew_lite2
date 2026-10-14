# ADR 0007: Operator skills guidance layer

## Status

Accepted; MCP delivery mechanics superseded by ADR 0012 on 2026-08-27 and
their public V2 shape superseded by ADR 0013 on 2026-08-30.

## Context

An operation schema defines one call, but agents also need bounded guidance for
multi-step use cases: ordering, human decisions, connection selection, safety,
and recovery. Client-only prose cannot be the MCP authority because it versions
independently of the server contract.

The registry retained by ADR 0013 owns MCP discovery and guidance delivery.
`onecrew_tool_search` returns complete represented-toolkit guides plus bounded
workflow hints naming registered operation IDs; `onecrew_tool_get` batch-reads the
exact schemas and safety contracts the Agent will execute. Workflow hints
guide execution but are not gettable/executable entries or business state.

The bundled Codex plugin carries one short `reacher` router Skill. It submits
one normalized English capability intent per independent outcome to live
  tool search with explicit toolkit filters, keeps execution values out of
  the query, consumes returned workflow hints directly, and batch-reads only
  operations that it is about to execute. Three conditional references
retain stable connection, CRM-write, and task-lease safety semantics without
copying operation inventories. Plain MCP clients remain complete without the
plugin.

## Decision

- MCP workflow guidance is a bounded search-returned hint with closed toolkit
  and operation references. It is neither a second workflow model nor a
  separately gettable entry.
- Executable contracts stay on operation entries. Before execution, an agent
  batch-reads selected operations with `onecrew_tool_get` and uses their exact
  input schemas, `schema_revision`, connection requirements, and safety policy.
- Repository-local per-use-case Markdown guides and one-wrapper-per-guide
  pairing are retired without aliases. The live registry playbooks are the
  canonical ordered guidance for every MCP client.
- Plugin contract tests pin exactly one router, resolvable references, the
  absence of retired Skill names, and the stable safety semantics that are not
  operation-specific.
- Guidance remains secret-free. It may name secret handling policy but never
  embed credentials, examples that resemble credentials, or transient values.

## Where a rule belongs

Four channels reach a connected agent, ordered by how directly they govern the
current action:

1. A typed operation result: current facts and recoverable next actions.
2. Initialize instructions: short cross-cutting routing and safety rules.
3. A search-returned workflow hint: bounded ordered use-case guidance.
4. An operation detail: the exact schema, revision, connection, effect, and
   safety contract for one call.

The bundled router helps Codex select the same live route, but it never
substitutes for channels 3 and 4. Runtime-computed facts belong in results;
operation-specific requirements belong in the operation descriptor; ordered
multi-operation guidance belongs in a workflow hint.

## Consequences

- Adding or changing an MCP use case updates the registry workflow hint and its
  closure tests rather than adding a public manual tool.
- Plain MCP clients receive the same hints and exact schemas without a
  plugin.
- The CLI and shared Web do not own a parallel operator-manual reader or per-use-case
  Markdown inventory.
- The bundled router refreshes on the plugin's content-digest/new-session
  boundary, while workflow hints and operation schemas come from the running
  server immediately.

## Superseded delivery mechanics

The original 2026-08 decision served Markdown manuals through dedicated list
and get tools on every workflow surface, plus a documentation-only path. ADR
0012 removed the dedicated manual tools without aliases at the Phase 7 atomic
cutover. The `/mcp/skills` path remains, repurposed as a read-only catalog
search/get surface; the former manual delivery mechanism is not a current
compatibility contract.
