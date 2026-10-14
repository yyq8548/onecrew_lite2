# Creator runtime

`onecrew-creator-runtime` is the platform-neutral application boundary for
canonical Creator saving, identity and contact resolution, relationship and communication state,
email, and durable Creator work. Desktop and MCP callers reach the same use cases
through peer adapters.

## Public surface

`CreatorRuntime` is the application facade. It exposes typed use cases for:

- read-only canonical save preparation and confirmed execution, retaining real
  selection, provider-read, and historical discovery provenance;
- canonical Creators, contact observations, confirmed routes, and resolved
  targets;
- relationships, conversations, messages, content interactions, and email;
- templates, drafts, confirmation, idempotency, and audit;
- read-only database inspection and explicit supported upgrade execution.

Public contracts are re-exported at the crate root. Provider search and profile
reads belong to `platform-runtime` toolkits; Creator preparation receives their
explicitly selected canonical facts. Outreach exposes readonly intent preparation,
versioned claims, verified receipts, and transactional records; the platform
runtime owns the external effects. Email retains its gateway; conversation
synchronization has a thread-only read port. Contract DTOs are imported from
`email-contracts` and `dm-contracts`, without a Creator re-export path.
Persistence enters through repository traits
in `repository`. Creator publishes its own best-effort invalidations after
committed operations and mailbox sync run transitions; the publisher is crate
private. Platform account/credential changes have a separate runtime bus.
Local-service merges both streams into the existing wire, assigning positive
sequence numbers only there; zero/All remains a forced-refresh sentinel.
These hints are not operation success receipts or a durable event feed.

The runtime does not expose provider clients, browser sessions, raw account
secrets, or adapter-specific response types.

## Workflow state

There is no single global Creator state machine. Each durable workflow owns typed
states and transitions in its domain module:

- save preparation keeps immutable canonical inputs separate from persisted Creators;
- profile fields are display facts; contact extraction consumes explicit text/link
  evidence with its actual source URL. Repeated copies of the same fact are read
  once, while observations from different sources remain distinct. YouTube video
  descriptions yield public email addresses, excluding promotional links and
  mentions; channel descriptions and About links retain their own contact rules;
- contact observations remain distinct from confirmed routes and resolved
  communication targets;
- external writes use confirmation and idempotency records;

Durable transitions are repository-backed; prepared saves are held by the
platform runtime's result owner. Adapters must not synthesize a second
workflow state from UI or transport conditions.

## Public contract conventions

New and migrated Agent-facing contracts follow these conventions, decided in
[ADR 0006](../../docs/adr/0006-mcp-contract-evolution-and-fixed-surfaces.md):

- Growing local list reads return `Page<TSummary>` bounded by a per-resource
  `PageLimits` (default plus clamped maximum) and continue through an opaque
  local cursor from `models::paging`, which binds every cursor to its issuing
  query context. Provider searches and their native cursors bypass this Creator
  paging contract through the platform runtime's typed toolkits.
- Stable resources pair a summary list with a keyed detail read; callers
  reference persisted or discovered state by stable IDs instead of resubmitting
  snapshots.
- Updates carry an expected `u64` revision and reject stale writes; creates
  carry an idempotent request identity instead of an optional-ID upsert.
- Durable guarded operations use a prepared operation with a typed operation ID, a
  persisted intent fingerprint, an explicit expiry, and a confirm step whose
  input is the operation ID plus, at most, a bounded selection among the
  options the prepare step issued (and an idempotency key where the effect is
  external); a confirm never resubmits caller-owned snapshots. The contact-route delete flow is the reference shape: a dedicated
  operation table, fingerprint re-derivation on confirm, and typed
  `Stale`/`Expired` terminal results instead of silent re-execution.
- Creator save uses a read-only canonical prepare. The runtime result owner
  holds its immutable prepared value and review until confirmation selects
  the issued rows and routes. Execution writes Creator facts and its replay
  receipt in one transaction; preparation writes no reservation and reads no
  discovery snapshot.
- External writes progress through the staged idempotency claim states and
  must represent an unprovable provider effect explicitly rather than
  reporting failure or success; recovery never repeats a remote write without
  proof.
- Outcomes the caller must distinguish are typed result enums; ordinary
  failures remain secret-free string errors.

## Storage

The SQLite repository owns Creator business state. It stores public business data,
provider identifiers, audit facts, idempotency records, and opaque references to
accounts, service credentials, and mailboxes. Secrets belong to `local-vault`.

Opening a fresh store creates the current schema. Opening an existing store
validates its declared identity and structural fingerprint. Ordinary open does
not migrate. Read-only inspection classifies storage before the enclosing
platform runtime opens it for mutation; supported migration execution is
backup-first and explicit.

Repository code is grouped by domain beneath `repository/sqlite`; schema
creation, released-format validation, adjacent migrations, backup, and upgrade
orchestration remain separate concerns.

Schema 37 removes the retired discovery snapshots and TikTok One binding table
through the backed-up 36-to-37 edge. Historical DDL and payloads stay frozen in
the migration references. The edge rewrites route-delete impacts and marks old
unresolved outreach claims without declaring their remote effects absent.
Current searches, saves, and Creator deletion no longer depend on snapshots.

## Failure model

Public operations return typed domain results where the caller must distinguish
states, and secret-free string errors for ordinary repository or capability
failures. Provider email failures additionally classify terminal rejection from
retryable transport behavior.

Unknown remote effects are never reported as confirmed success. External write
paths preserve confirmation, idempotency, and audit evidence and require
readback where supported. A failed process-local invalidation notification does
not turn an already committed business operation into a failure.

Storage inspection and upgrade use typed status/error projections so Desktop
can distinguish fresh, ready, upgradeable, busy, unsafe, corrupt, and
post-commit recovery outcomes without parsing SQLite errors.

## Dependency rules

- Keep Creator models and use cases independent of Tauri and MCP.
- Keep migrated provider reads and searches in `platform-runtime` toolkits.
  Keep outreach effects outside Creator. Retain the Email gateway and the
  thread-read port for conversation sync; do not restore target,
  relationship, or content gateways.
- Keep secrets and account connection mechanics in `platform-runtime` and
  `local-vault`.
- Add business persistence through the owning repository trait and SQLite
  module; do not write Creator tables from an adapter.
- Treat schema or persisted-shape changes as local-data evolution, not an
  incidental refactor.

See the root [architecture map](../../ARCHITECTURE.md) and
[local-data ADR](../../docs/adr/0004-explicit-forward-local-data-evolution.md).
