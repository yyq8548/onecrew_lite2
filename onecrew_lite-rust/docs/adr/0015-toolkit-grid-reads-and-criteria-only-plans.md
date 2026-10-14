# ADR 0015: Toolkit-grid reads and criteria-only search plans

## Status

Accepted on 2026-09-12. Records the toolkit-grid model adopted by
`docs/plans/toolkit-grid-decoupling-2026-09-12/`. Amends ADR 0012's
provider-read failure rule and supersedes the verb list in its 2026-09-11
amendment where the two differ. ADR 0012's registry placement in
`mcp-adapter`, its closed toolkit list and its rule that the shared Web uses
explicit typed routes rather than a generic executor are unchanged by this
record; they are the subject of ADR 0016. Every other ADR 0012 and ADR 0013
decision remains accepted.

Amended on 2026-09-13 after the cut 1 boundary review. The dated amendment
below takes precedence over the original statements about runtime registration,
shortlist contents, provider failure text and complete removal of Creator ports.

The final P5 scope amendment below supersedes the shortlist proposal entirely:
the user cancelled independent candidate staging for this delivery.

Amended on 2026-09-14 by the refresh retirement amendment at the end of this
record. It takes precedence over the original statements about Creator
formatting at refresh and the `refresh` verb.

## Context

Every platform read reached its provider through the Creator runtime, every
search page was persisted as a Creator candidate snapshot before the user had
chosen anything, and a confirmed search plan carried run state: a candidate
ledger, a shared page pool and a novelty flag on every hit. One adapter crate
implemented the Creator-owned provider ports, so a provider change touched
Creator models and a Creator change touched provider code. The user asked for
that coupling to be removed without a new abstraction: no normalized
cross-platform model, no new crate, port or trait.

The plan-governed search confirmation (ADR 0012, amendment 2026-09-10) is the
user's authority over which provider criteria may run. Its budget was a
second, implicit authority that stopped paging on counters the user never
confirmed.

## Decision

### Peer cells and one runtime

The application is a grid of peer cells: `onecrew`, `creator`, `email` and one
toolkit per platform (`x`, `instagram`, `telegram`, `youtube`). A cell does
not import another cell's models or helpers. The runtime, `platform-runtime`,
registers cell methods and calls them through that registration; it owns
connection admission and governed dispatch. MCP (`mcp-adapter`) and the Web
(`local-service` typed routes) are peer projections: each calls the same
runtime function once, and neither owns a second business model.

### Provider data as returned

A toolkit cell method is `connection admission -> facade`. It returns the
provider's own type, page and identifiers as the facade returned them. There
is no normalization on read and no shared cross-platform hit, profile or error
model. Where a projection needs a field, the runtime picks it from the
provider type; toolkit types never enter `creator-runtime`.

### Criteria-only search plans

A confirmed search plan sets criteria and parameters only. It holds approved
scope tokens and one expiry
(`crates/platform-runtime/src/search_plan/authority.rs`); a scope token is
derived from the compiled scope — operation, schema revision, connection and
the compiled arguments including the frozen page size
(`crates/mcp-adapter/src/operation_registry/execution/invocation.rs`).
Nothing is stored under, keyed by or lifetime-bound to a plan: no page pool,
no candidate ledger, no novelty, no budget, no stop reason.

A plan-governed call is admitted when its compiled scope equals an approved,
unexpired scope token. A changed criterion is refused with
`search_plan_scope_mismatch`; the surviving codes are
`search_plan_confirmation_required`, `search_plan_invalid_or_expired`,
`search_plan_scope_mismatch` and `search_plan_approval_unavailable`
(`crates/domain/src/search_authorization.rs`). The cursor is stripped before
the scope is compiled, so a repeated page of a confirmed scope is admitted.
Paging is bounded by the per-call count, the provider and the plan's TTL, and
by nothing else. The `plan_budget` result field, `hits[].plan_novelty`, the
codes `search_plan_page_budget_exhausted` and
`search_plan_candidate_budget_reached`, and the Router sentence that read them
are retired.

### Creator formatting at save and refresh

Creator formatting — identity resolution, contact classification and route
persistence — happens only inside save and refresh, owned by
`creator-runtime`. The pre-save shortlist is process-local in
`platform-runtime`, is not keyed by a plan, and expires on restart. No search
page is persisted before the user saves.

### Verbs

Each verb carries one meaning. This list supersedes the verb list in ADR
0012's 2026-09-11 amendment where the two differ; `list`, `check`, `refresh`,
`resolve` and `prepare` -> `confirm` | `send` keep that amendment's meaning.

- `search`: a toolkit provider search returning provider data as returned.
- `get`: a read of one object by locator, including a provider read;
  `telegram.community.get` resolves a provider target.
- `extract`: a page read that returns what was found on the page;
  `onecrew.link_hub.extract` returns links and emails as found, with no
  Creator vocabulary.
- `record`: a Creator-side write of a provider receipt with no provider call:
  `onecrew.outreach.relationship.record`,
  `onecrew.creator.content_reply.record` and
  `onecrew.outreach.message.record`. It is the verb `onecrew.triage.record`
  already uses.
- `discover` and `enrich` retire with their operations.

### Provider writes

A provider write is a toolkit method: `connection admission -> facade write ->
facade readback`, returning the provider's receipt and readback as returned.
A toolkit write carries its own follow gate and accepted target kinds in its
own terms. The runtime's governed dispatch wraps it with the owner-parameter
check (`confirmed`), the in-flight guard, the durable idempotency claim and
receipt, the unknown-effect states and the audit row. The Creator record
method is idempotent by its receipt and calls no provider. On MCP the toolkit
write's result carries `next_tool` naming the record operation, and
`completed` in the ledger means provider-verified, not Creator-recorded. On
the Web one composed runtime function runs the governed write and then the
record, and the route calls it once.

### Provider error text (amends ADR 0012)

ADR 0012 required typed provider-read failures to be compressed into a closed
secret-free category, with no transport text crossing the adapter boundary.
For toolkit cells this record replaces that rule: provider error text may
cross the adapter boundary after the toolkit's own `safe_message` redacts URL
userinfo, query strings and proxy or credential values. Transports stringify
errors built from credentialed proxies
(`crates/ig-transport/src/transport.rs`, `crates/tg-transport/src/error.rs`),
so the redaction is a per-toolkit obligation. No shared error taxonomy is
introduced; secret-free public DTOs, logs and audits remain required.

### Superseded records

- ADR 0012, amendment 2026-09-11, the verb list: superseded by the verbs
  above where the two differ. Its naming rule — dotted operation IDs,
  underscored fixed tools, toolkit-prefix enforcement, `onecrew` operations
  declaring no connection — is unchanged.
- ADR 0012's provider-read failure rule: amended as above.
- `docs/plans/layered-mcp-platform-surface/README.md`, "Creator discovery,
  target resolution, reply triage, and outreach idempotency remain CRM
  workflows rather than platform primitives": superseded. Search, target
  resolution and administrator extraction are toolkit primitives; reply
  triage and the Creator's own record identities stay with the Creator.

## Consequences

- Retired operation IDs, guarded by the registry's production-inventory
  absence lists and the plugin's retired-token test:
  `onecrew.creator.community_administrators.discover`,
  `onecrew.outreach.relationship.update`, `onecrew.outreach.message.send`,
  `onecrew.creator.content_reply.send`, and the snapshot-keyed
  `onecrew.creator.contact.enrich`, `onecrew.creator.link_hub.enrich` and
  `onecrew.creator.contact.confirm`. Their replacements are the toolkit
  methods and record operations named above, `telegram.community.get`,
  `telegram.community.administrators.list` and `onecrew.link_hub.extract`.
- `plan_budget`, `plan_novelty`, `search_plan_page_budget_exhausted` and
  `search_plan_candidate_budget_reached` are retired tokens: the registry's
  absence guards and `apps/web/src/lib/codex-marketplace.test.ts` refuse
  them, and the plan-governed searches rotate their schema revisions. The
  Router keeps only `next_cursor` paging guidance.
- A confirmed plan can page until its TTL expires; there is no server page
  cap and no candidate cap. The user's confirmation is the only search
  authority.
- Toolkit crates depend on no runtime, on no other platform's crates and on
  no schema derivation; `creator-runtime` holds no provider read or write
  port and no pre-save provider state once the plan's phases land.
- ADR 0012's registry placement in `mcp-adapter`, its closed toolkit list and
  its rule that the shared Web uses explicit typed routes rather than a
  generic executor are unchanged here; ADR 0016 decides them.
- The decision bodies of ADR 0012 and ADR 0013 are dated records and are not
  rewritten.

## Amendment 2026-09-13: typed owners and a read-only catalog

The original peer-cell wording combined application composition, provider
capabilities, protocol registration and durable Creator semantics. Moving those
concerns into one runtime registry would retain their coupling. The revised
[execution plan](../plans/toolkit-grid-decoupling-2026-09-12/README.md) therefore
keeps typed application use cases with their actual owners and replaces P12's
shared generic executor with a neutral, immutable capability catalog.

The catalog will own stable navigation identifiers and descriptions. MCP retains
its schemas, schema compilation, JSON-to-typed bindings, execution, search,
guides and playbooks. HTTP routes call typed owners directly. A catalog entry
does not authorize execution or imply that both adapters implement an endpoint.
These are target boundaries; this amendment does not claim that P12 has landed.

Platform facades own provider semantics, including meaningful read-failure
classification. Runtime toolkit functions own local input and connection
admission, session or credential acquisition, and local outcome recording.
They return provider types without creating canonical Creator records. MCP
projects public failure classifications and local commit outcomes, with no
client/transport error destructuring. Provider error bodies, source chains and
session diagnostic text are discarded at this boundary. This supersedes the
original permission to expose arbitrary redacted provider prose.

The public-web link-hub capability owns bounded fetching and parsing in a
private runtime module. It has no LocalPlatform, Creator, shortlist or registry
dependency. Its toolkit entry delegates to that capability. Parsing uses the
final redirect URL for relative references and preserves the initial URL as
source provenance. Creating parallel transport/client/facade crates for this
one public-web capability is not required.

P5 shortlist stores only explicitly selected candidate locators, connection
references, provenance, bounded notes and revisions in an off-the-shelf memory
cache. It owns no provider objects, callbacks, formatting, networking or save
authority. Presentation and save preparation fetch authoritative typed profiles
through the appropriate runtime entry. Save preparation produces the exact
immutable payload held by the result owner; confirmation consumes that payload
without refetching the provider or selection cache. Cache eviction and expiry
cannot revoke or manufacture confirmation authority. The cache is best-effort
process state; the result and Creator owners retain confirmation, claims,
idempotency, audit and transactions.

Creator retains the narrow email and DM-access ports needed by its existing
mail and conversation-sync use cases. P10 removes their adapter crate by moving
concrete implementations to the composition runtime. It does not promise zero
provider I/O in every Creator use case. Likewise, the runtime's Telegram login
state machine and the mailbox lifecycle callback remain until their real
lifecycle contracts are replaced. Cargo dependencies, runtime callbacks and
shared object ownership are reviewed separately; a shared Arc alone is not a
reverse dependency.

## Final P5 scope amendment: direct save preparation

The user cancelled the independent shortlist feature after reviewing the
additional coupling and state it introduced. P5 does not provide a candidate
cache, staging key, handler registry or prerequisite add/list/remove operation.
The earlier shortlist paragraphs are historical proposals, not instructions for
the current delivery.

Save preparation accepts explicitly selected platform locators, necessary
connection references, real provenance and the user's notes, tags and quality
judgment. The runtime obtains fresh public provider data and asks Creator for a
read-only canonical preparation. The existing result owner holds that complete
immutable preparation and its accurate review. Confirmation consumes it once
and invokes the Creator transaction, without fetching or formatting it again.
Review row identifiers and result binding revisions remain confirmation facts;
they are not candidate-cache identifiers.

The 1000-item Creator business batch, eight-call MCP envelope, provider
concurrency and payload/transport budgets remain separate. Removing shortlist
does not cancel snapshot retirement: P5 removes save-time snapshot dependence,
P6 stops active search snapshot writes, and P7 later removes the table and
historical rows through the planned backed-up v36-to-v37 upgrade after its
dependencies are ready. Those completion states must be reported separately.

## Amendment 2026-09-14: refresh retirement

The [removal specification](../../.scratch/remove-creator-profile-refresh/spec.md)
found that `onecrew.creator.profile.refresh` composed a Creator read, a toolkit
read, a provider-to-Creator mapping and a Creator write inside one MCP binding.
This amendment retires that operation and records where Creator formatting
happens.

### Creator formatting only at save

This replaces *Creator formatting at save and refresh*. Identity resolution,
contact classification and route persistence happen only inside Creator save
preparation and confirmation, owned by `creator-runtime`. The
provider-to-Creator mapping exists only on the Creator save read and
preparation path. No layer owns an operation that reads a platform and then
writes a Creator.

### Retirement of `onecrew.creator.profile.refresh` and the `refresh` verb

`onecrew.creator.profile.refresh` retires together with the `refresh` verb, with
no hidden entry or compatibility alias. This supersedes "`refresh` re-reads and
saves onto a Creator" in ADR 0012's 2026-09-11 amendment, and it removes
`refresh` from the verbs that *Verbs* above keeps with that amendment's meaning.

An Agent composes independent calls instead:

- a platform read returns 平台原生资料 without any Creator write;
- an explicit follower count or display name change goes through
  `onecrew.creator.update`, keeping its validation, version rules and readback;
- an observation change such as avatar, handle, bio or homepage re-runs Creator
  save (`onecrew.creator.candidate.save` preparation and confirmation) for the
  same account, appending history while the Creator identity stays unchanged.

A platform read failure is not reported as a Creator update failure, and a
Creator update failure triggers no implicit platform re-read. The playbook
becomes `playbook.onecrew.creator-profile-update`, and
`onecrew.creator.profile.refresh` and `playbook.onecrew.creator-profile-refresh`
join the registry's retired-token guard. Stored observations with the
`profile_refresh` reason, follower counts and audit facts stay readable without
a data migration. Operator-account profile refresh in `platform-runtime` and
Codex plugin status refresh are unrelated and keep their names.

## Amendment 2026-09-16: the Creator business batch is 50 items

This supersedes "The 1000-item Creator business batch" in *Decision* above. The
separation it records is unchanged: the business batch, the eight-call MCP
envelope, provider concurrency and payload/transport budgets remain independent
limits. Only the business batch's own value changes, from 1000 to 50.

1000 was never reachable end to end. One review must project every row inside
`MAX_CREATOR_SAVE_BATCH_PROJECTION_BYTES` (4 MiB) while any single legal preview
may reach `MAX_PROJECTION_BYTES` (64 KiB), so 4 MiB / 64 KiB = 64 rows is the
largest batch for which *any* legal input still projects; at 1000 rows each row
would have 4,194 bytes, less than one legal 8 KiB field. A 1000-candidate tool
input is also not generatable within a model's output budget, and 1000 rows
cannot be reviewed by a person inside the 15-minute prepared TTL. The only test
that exercised 1000 rows overwrote `presentation.sections` directly and so never
ran the production projection; it now calls the real sealing path.

50 sits under the 64-row arithmetic ceiling, keeps a batch reviewable inside the
prepared TTL, and makes "find 100 creators" exactly two batches.

`MAX_MANUAL_CREATOR_BATCH_ITEMS` moves to 50 with it. Its binding constraint was
always `MAX_MANUAL_CREATOR_BATCH_PAYLOAD_BYTES` (1 MiB): one item carrying the
maximum 16 contact routes already exceeds 1 KiB, so a 1000-item manual batch
always failed on payload bytes first and the item count never took effect.

`MAX_CREATOR_SAVE_MATERIALIZED_BATCH_BYTES` moves from 64 MiB to 16 MiB. At
64 MiB the derived `MAX_CREATOR_SAVE_BATCH_OWNER_ACTION_BYTES`
(materialized + 2 x 4 MiB) was 72 MiB, above the result store's
`DEFAULT_MAX_TOTAL_PAYLOAD_BYTES` of 64 MiB, so an owner action that reached its
own legal ceiling could never be admitted. At 16 MiB it is 24 MiB, and a
compile-time assertion now keeps that relationship true.

The frozen navigation fixtures under `crates/platform-runtime/src/catalog/fixtures/`
carry the advertised limit inside operation summaries. They are updated here with
exactly the substitution applied to the catalog source, because the advertised
limit and the enforced limit must not diverge.
