# ADR 0012: Stateless MCP operation registry

## Status

Superseded for public MCP discovery/execution/presentation mechanics by ADR
0013 on 2026-08-30. Its typed registry, owner, secret, effect, confirmation,
idempotency, audit, readback, and native-host decisions remain accepted.

Originally accepted to supersede the flat-manifest, handwritten catalog, public
index/detail/action, and generic-executor prohibitions in ADRs 0006 through
0008. Their runtime ownership, peer-adapter, secret, effect, confirmation,
idempotency, audit, readback, and native-host decisions remain accepted.

## Context

The complete MCP surface currently publishes 115 tools and serializes to
480,102 bytes before the JSON-RPC envelope. Output schemas alone account for
365,462 bytes. Adding a typed operation currently repeats public identity in
routers, metadata, surface manifests, workflow guidance, plugin instructions,
approval configuration, and widget allowlists.

The application owners are already typed. The context growth belongs to the
Agent-facing adapter contract, so it does not justify a session object, a
runtime-owned schema registry, untyped provider requests, or a second business
model.

## Decision

Every ordinary MCP surface publishes exactly five model-visible meta tools:

- `onecrew_tool_search` performs bounded deterministic local search;
- `onecrew_tool_get` returns one exact operation, concrete playbook, or toolkit
  guide entry, and may prepare an opaque presentation reference without
  mounting an App;
- `onecrew_result_present` explicitly presents one server-issued reference to the
  user and is the sole App-bearing model-visible tool;
- `onecrew_tool_execute` invokes one executable operation; and
- `onecrew_connection_list` returns secret-free tagged connection references.

One additional interaction-state tool is visible only to the mounted App. It
is not a model verb, invokes no business owner, and cannot authorize an
operation outside its exact interaction binding.

`mcp-adapter` owns one compiled, stateless operation registry. Meta tools unify
discovery, detail, execution, and connection protocol; they do not unify
business contracts. Each operation belongs to exactly one closed toolkit
(`x`, `instagram`, `telegram`, `youtube`, `crm`, `email`, or `task`) and retains
that toolkit's locator, pagination, connection, failure, input, and output
contract. The registry carries stable public operation IDs, exact input and
output schemas, bounded search metadata,
surface allowlists, effect and secret classification, connection requirements,
approval class, and erased bindings over typed Rust requests and outcomes.
Search, get, present, and execute enforce the same surface allowlist. Search and
ordinary get do not inspect live account availability, and no catalog search
remembers prior discovery. Presentable get variants create only a bounded
process-local interaction record; `onecrew_result_present` makes that record
available to one App without copying its full payload into model context.

Search may cross toolkit boundaries without merging schemas. Alongside at most
six operation matches, it may return one bounded concrete playbook containing
a use case, toolkit set, primary and supporting operation IDs, optional bounded
steps and pitfalls, and difficulty. A playbook may be toolkit-specific or
genuinely cross-toolkit,
but it owns guidance only: it carries no operation schema, runtime policy, or
executable binding, and execute rejects its ID.

When the caller supplies an explicit toolkit filter, search may also attach one
compact toolkit guide. Its definition contains stable platform background:
purpose, connection model, identifier semantics, capability categories, and
limitations. Exact get additionally derives that toolkit's complete
surface-visible operation directory from the compiled registry, exposing only
operation ID, title, and canonical summary. The directory is current-surface
navigation rather than a copied capability catalog: it does not participate in
operation/playbook lexical ranking, carry schemas or safety policy, prescribe
ordered work, or authorize execution. `onecrew_tool_get` returns the guide and
directory, and execute rejects the guide ID. A caller still gets the selected
operation's exact current detail before execution.

Nested JSON is allowed only in `onecrew_tool_execute.arguments`. Before any
owner call, the adapter resolves an exact registered ID, checks the surface,
validates against that entry's exact schema, and decodes into its registered
Rust input. The binding then calls the existing typed runtime owner or shared
secret-free builder. Untyped JSON never reaches a runtime or provider facade.

RMCP's native `ToolRouter::call` remains the outer dispatcher for the five
model-visible meta tools and the hidden App-only interaction route. Registry
lookup and typed binding dispatch happen inside the execute handler and do not
copy RMCP route lookup, extraction, or invocation.

This is a clean replacement. New private modules are divided by responsibility
(schema, search, execution, approval, interaction, and domain registration),
with a thin construction facade. Registrations do not wrap legacy MCP handlers.
The legacy surface remains frozen only as an equivalence oracle until one
atomic cutover; there are no aliases, fallbacks, dual public surfaces, or a
single monolithic domain registry.

`onecrew_tool_get` is permanently Agent-only. `onecrew_result_present` is the only
model-visible App-bearing meta tool; external Apps read and transition the
same process-local interaction owner through a hidden App-only route and
submit operation cards through the fixed, widget-free executor using their
exact interaction binding. Agent and hook execution uses direct authority.
The shared Web does not call MCP: a closed interaction kind maps to an explicit
typed HTTP action over that same interaction owner and typed business owner. A
generic JSON HTTP executor is prohibited.

The interaction owner is a bounded in-memory state machine in
`platform-runtime`. It retains secret-free presentation state, CAS revisions,
submit/unknown-effect barriers, terminal results, cancellation, and
replacement only for the running process. Restart, expiry, or eviction returns
a read-only expired/absent state; transcript text, iframe storage, and old tool
results never reconstruct pending authority. Pre-0.4.1 cards are unsupported
for interaction and fail closed rather than entering a version compatibility
path. The process-local owner creates no persisted interaction artifact.

Each executable descriptor has exactly one approval class:

- `none`: no adapter approval step;
- `owner_guarded`: the typed owner enforces its existing confirmation,
  idempotency, audit, and readback policy; or
- `attended_prompt`: the execute call requests live approval before invoking
  the binding and fails closed when the turn has no approval channel.

No approval token or session is persisted. The mixed executor is not globally
destructive, because doing so would incorrectly deny unattended safe/task
operations. Descriptor effects stay conservative and never weaken owner
guards.

Campaign create/update use `attended_prompt` on the public MCP executor and
bind the negotiated approval to that exact invocation. The shared Web does not
reuse the generic approval path: its typed HTTP action binds the complete final payload to
the shared process-local interaction, closes the submitted barrier, and then
invokes the Campaign owner once. A Choice, preview, old card, or Agent-edit
request is not Campaign confirmation.

Typed provider-read failures are compressed below the provider facade into a
closed secret-free category, optional bounded HTTP status, effect certainty,
and recovery directive. Raw provider bodies, URLs, cookies, proxy values, and
transport text never cross the registry or CLI/Web adapter boundary. A local commit
whose outcome is ambiguous remains `unknown` and non-retryable.

X and Instagram profile operations accept handles only. This registry does not
add by-ID provider behavior or numeric-target heuristics. X post discovery
offers both the raw provider-query operation and a separate typed advanced
operation whose compiler is limited to the explicitly verified operator
subset; unsupported operator families remain the raw caller's responsibility.

## Consequences

- Registry size may grow without changing `tools/list` names or its fixed byte
  budget.
- Operation IDs and descriptor schemas are public contracts and are not
  silently renamed, reused, or repaired with compatibility shapes.
- Concrete registry playbooks are the canonical ordered use-case guidance. The
  bundled Codex overlay carries one short intent router and no per-use-case
  manual or wrapper aliases. That router normalizes non-English intent, splits
  independent outcomes into atomic capability queries with explicit toolkit
  filters, and keeps operation argument values out of search.
- Toolkit guides are optional stable background and a search-miss recovery
  aid. Their operation directories are generated from the live registry for
  the requested surface and remain navigation only; they are not Skills,
  playbooks, cached inventories, or execution authority.
- HTTP and MCP remain peer adapters calling typed owners directly; the CLI
  coordinates the same owners through `local-service`.
- The bounded App interaction map is process-lifetime state rather than an
  Agent session, persisted artifact, or source of business truth. No remote
  sandbox or tenancy model is introduced.
- Typed owner failures, secret-safe projection, confirmation, idempotency,
  audit, readback, and unknown-effect behavior remain authoritative.

## Amendment (2026-09-10)

The Campaign create/update operations, the Campaign owner they invoked, and the
`crm` toolkit prefix were retired on 2026-09-10 with Creator schema v36
(release 0.6.0); their attended-prompt approval path is historical. External
creator and content searches are now gated by the operation-bound
`onecrew.search.plan.prepare` confirmation, whose scopes bind exact registered
operation contracts. The Choice operation `task.choice.request` became
`onecrew.ask_user_question` on 2026-09-09. The decision body above is a dated
record and is not rewritten.

## Amendment (2026-09-11)

Naming is fixed. A dotted ID is a registry operation; an underscored
`onecrew_*` name is a fixed tool and never an operation. An operation's ID
prefix is its declared toolkit, enforced by
`RegistryBuildError::OperationToolkitMismatch`. OneCrew-owned operations keep
the `onecrew` prefix and declare no connection even when the Creator owner
reaches a provider on their behalf.

Each verb carries one meaning: `search` is plan-governed provider search
yielding candidates; `discover` returns new candidates from a known source;
`enrich` adds evidence to an existing candidate; `get` reads one; `list`
returns a bounded page; `check` probes one live connection; `refresh`
re-reads and saves onto a Creator; `resolve` classifies locally; and
`prepare` -> `confirm` | `send` is a two-step write. Human login flows live
under `connection.{browser,login,reconnect}.*`. A `next_tool` value or a
failure recovery reference names a registry ID or a fixed tool, never a
transport-local command name.

## Amendment (2026-09-14)

The 2026-09-14 amendment to
[ADR 0015](0015-toolkit-grid-reads-and-criteria-only-plans.md) retires
`onecrew.creator.profile.refresh` together with the `refresh` verb. It
supersedes "`refresh` re-reads and saves onto a Creator" in the 2026-09-11
amendment above; that amendment's naming rule and the verbs ADR 0015 keeps are
unchanged. The decision body above is a dated record and is not rewritten.
