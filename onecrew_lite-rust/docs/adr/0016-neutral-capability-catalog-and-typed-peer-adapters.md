# ADR 0016: Neutral capability catalog and typed peer adapters

## Status

Accepted on 2026-09-14. Completes the catalog decision in the dated amendment
to [ADR 0015](0015-toolkit-grid-reads-and-criteria-only-plans.md) and amends
[ADR 0012](0012-stateless-mcp-operation-registry.md) where it couples shared
navigation to MCP registration. Other MCP protocol and safety rules remain.

## Context

HTTP toolkit navigation previously read the MCP operation registry. Moving
that registry wholesale into the application runtime would move JSON Schema,
protocol surfaces, search indexing and erased execution with it. It would also
make a listed capability look like an executable HTTP endpoint. The two inbound
adapters need common identities and descriptions, while each keeps its own
protocol admission and invokes the same typed application owners.

## Decision

`platform-runtime::catalog` owns an immutable `CapabilityCatalog`. One static
inventory declares `ToolkitId`, canonical `OperationId` constants, descriptions
and explicit `McpBinding` / `WebNavigation` support. The catalog contains no
handler, schema, session, application instance or execution state. Production
build validates uniqueness, identifiers, references and required descriptions.
Small tests supply explicit static declarations; production has no empty or
registry-derived fallback catalog.

The local service builds one `Arc<CapabilityCatalog>` and passes that exact
instance to MCP registration and HTTP navigation. The application graph cannot
become ready when registration fails. HTTP projects navigation directly from
the catalog, retaining its existing wire fields. Navigation support alone
neither requires an MCP binding nor creates an HTTP execution route.

MCP owns `McpOperationMetadata`, typed binding adapters, JSON schemas, its seven
protocol surfaces, search indexes, playbooks, guides and generic JSON execution.
Bindings reference canonical `OperationId` values. Registry sealing compares
handlers against the catalog's independently declared MCP set and rejects
missing, duplicate, unknown and non-MCP bindings. Toolkit guide descriptions
come from the catalog; MCP presentation and workflow text stay in MCP.

Web uses one `web_commands!` declaration to generate names, allowed argument
keys, execution classes and direct typed handler dispatch. Unknown commands
fail before host work admission. Business work, control work and stop retain
their distinct lifecycle behavior; stop must not drain a control slot that it
itself holds. The host owns a command's complete future through completion.

Creator repository operations expose concrete async owner methods with bounded
read, write or CPU admission. Both peers await those methods. Existing blocking
methods remain the actual mechanisms used inside admitted work. Dispatch errors
remain distinguishable from owner errors and started-work uncertainty. Vault
and OS segments use bounded blocking dispatch; no arbitrary repository closure
or shared `operation_id + Value` executor is exposed to Web.

## Consequences and verification

A new provider capability changes its provider facade, application admission,
catalog declaration and intended inbound binding. A new Web use case needs a
typed route. A Creator save mapping is added only when the product supports
saving that target. No generic provider registry or candidate cache is needed
to make those boundaries explicit.

Runtime dependency and catalog import checks reject protocol/executor ownership
moving inward. Behavioral tests cover independent registration completeness,
shared catalog identity, navigation-only declarations, command admission,
cancellation and blocking permit lifetime. Frozen directory fixtures and a
before/after full registry export verify protocol preservation. These checks
test separate claims; import checks alone do not prove lifecycle correctness.

This decision changes no persisted schema, confirmation authority, external
write policy or provider transport/client/facade boundary.
