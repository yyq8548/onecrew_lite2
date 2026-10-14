# Schema Path and Impact

Design the persisted-format path before writing migration SQL. The machinery
this reference describes lives under
`crates/creator-runtime/src/repository/sqlite/schema/` (facade `schema.rs`,
current DDL in `schema/current.rs`, edges in `schema/migrations/`, execution
in `schema/upgrade/`) and is mirrored in miniature by
`crates/local-vault/src/store/schema.rs`.

## Critical Rules

Released durable identity is immutable. A version number, marker, structural
fingerprint, and canonical DDL that shipped may never change meaning under the
same number; evolution appends a new adjacent edge and bumps
`CURRENT_SCHEMA_VERSION` instead of editing what released databases already
contain.

```rust
// ❌ FORBIDDEN: editing the released v22 -> v23 edge in
// schema/migrations/v22_to_v23.rs — databases already at v23 silently
// diverge from the frozen DDL and the fixtures stop proving anything.
const REPLY_TRIAGE_V23_SQL: &str = "
CREATE TABLE reply_triage (
  id TEXT PRIMARY KEY,
  priority TEXT,  -- column added after release under the same version
  ...";

// ✅ CORRECT: append the next immutable adjacent step and register it.
// schema/migrations/mod.rs — existing edges stay byte-identical.
mod v_previous_to_next;
const FORWARD_MIGRATIONS: &[ForwardMigration] = &[
    // ...all released edges unchanged...
    ForwardMigration {
        from_version: PREVIOUS_SCHEMA_VERSION,
        to_version: NEXT_SCHEMA_VERSION,
        apply: v_previous_to_next::migrate,
    },
];
// schema.rs remains the single source for the target identity; read its
// current value from source instead of copying it into guidance.
```

```rust
// ❌ FORBIDDEN: tolerating released rows that lack the field — the format
// forked silently and every consumer now sees two shapes.
#[derive(Serialize, Deserialize)]
struct BindingStrategy {
    #[serde(default)] // papers over rows written before the field existed
    max_daily_outreach: u32,
}

// ✅ CORRECT: an adjacent edge rewrites the persisted JSON deterministically
// and the typed impact reports it. This is what the historical v13 -> v14
// edge did (the Campaign tables it touched were dropped in v36): it
// normalized every campaign binding strategy into the typed shape and
// surfaced the count as `normalized_campaign_binding_strategies`.
fn migrate(connection: &Connection) -> Result<(), String> {
    // rewrite each campaigns binding-strategy row into the typed v14 shape,
    // inside the orchestrator's single outer transaction
}
```

The current version number appears as a literal exactly once, in
`schema.rs` (`pub const CURRENT_SCHEMA_VERSION: i64`, re-exported as
`onecrew_creator_runtime::CREATOR_SCHEMA_VERSION`). Historical edge literals
(frozen DDL, `from_version` numbers, test-fixture `PRAGMA user_version = N`)
stay literal because they name released versions that never move.

```rust
// ❌ FORBIDDEN: a second current-version literal — every bump must now
// hunt down stragglers, and a missed one ships a lie.
assert_eq!(status.to_version, COPIED_CURRENT_VERSION);

// ✅ CORRECT: reference the one constant; a bump touches exactly the
// constant plus the new adjacent edge.
assert_eq!(status.to_version, schema::CURRENT_SCHEMA_VERSION);
```

## Quick Reference

| Concern | Rule |
|---|---|
| Layering | Current DDL in `schema/current.rs`, one module per edge in `schema/migrations/`, execution in `schema/upgrade/`; `schema.rs` stays a thin facade |
| Release identity | Version, marker, fingerprint, and canonical DDL are one immutable tuple; changing any of them means a new version |
| Reset | Only with positive release-and-storage proof that no persisted user data can exist; uncertainty is potentially persisted |
| Adjacent steps | Every transition is one `from -> from + 1` edge; longer paths run through the ordered registry, never a direct `11 -> 14` transform |
| Registry validation | `apply_forward_migrations` rejects gaps, skips, and chains that miss the target version |
| Normal open | `schema::initialize` may identify `UpgradeRequired` but never executes the registry |
| Version literal | `CURRENT_SCHEMA_VERSION` is the only current-version literal; frozen historical numbers stay literal |
| Release mapping | Every new edge's module header must record the app release that ships it (for example `v22 -> v23 (app 0.2.9)`); older edges that predate this convention stay frozen as they are |
| Impact | Named per-category counts built from persisted facts, deterministic so the locked revalidation can compare equality |
| Blockers | Ambiguous ownership, lineage, or attribution returns a typed block; never guess an owner or lower an invariant |
| Owners and adapters | Data-directory inspection, vault compatibility, and the lock live in `platform-runtime`; local-service startup owns upgrade admission and execution; Web and MCP project read-only status and recovery, without SQL or upgrade actions |

## Common Patterns

### One module per adjacent edge

Keep a simple edge in one module and split a data-dependent rebuild by
preflight/plan/rebuild/backfill/validation responsibility. Steps assume the
orchestrator's transaction: no edge begins, commits, or stamps the version.

### The registry is the only path

`FORWARD_MIGRATIONS` in `schema/migrations/mod.rs` is an ordered slice of
`ForwardMigration { from_version, to_version, apply }`. A database at 11
reaching 23 runs every edge in order as one user-visible upgrade and one
transaction. `apply_forward_migrations` fails on a missing edge, an edge that
does not step by exactly one, and a chain that stops short of the target.

### Normal open identifies, never executes

`schema::initialize` creates the current schema only when the database is
empty (`table_count == 0 && user_version == 0`), validates a current-version
database without writes, and returns an upgrade-required error directing the
caller to start the local service for inspection, backup, and upgrade of a
supported predecessor. Versions below `EARLIEST_MIGRATABLE_SCHEMA_VERSION` (in
`schema/inspection.rs`) and versions above current get a typed rejection —
never a marker bump, never an in-place repair.

### Fresh creation lands on current

`create_current_schema_unchecked` in `schema/current.rs` builds the complete
current format and stamps the version via
`pragma_update(None, "user_version", CURRENT_SCHEMA_VERSION)`. A fresh
install must produce a database indistinguishable from a fully migrated one;
`schema_contract` tests prove this equivalence, and the migration registry
never runs on the fresh path.

### Release-to-schema mapping lives with the edge

Every new edge module opens with a frozen header comment naming the
transition and the product release that ships it, plus the design reasoning
worth keeping with the DDL (`v21_to_v22.rs` and `v22_to_v23.rs` are the
models; older edges that predate this convention stay frozen without one).
Schema versions and application versions are independent sequences: every
schema bump requires a new product version, but a product-only release keeps
its schema version. Never reuse an already-distributed app version with a
different schema identity, and keep internal path-crate Cargo versions
independent of schema bumps unless the crate is a separately published
artifact.

### Typed impact from persisted facts

`CreatorDatabaseUpgradeImpact` (`creator-runtime/src/models/data_upgrade.rs`) carries
named counts — `deleted_tasks`, `deleted_task_idempotency_records`,
`cleared_reply_triage_task_refs`, and the rest — never a prose
summary. `collect_chain_migration_impact` builds it read-only, running only
the per-edge preflights that apply to the source version. Classify every row
as preserved unchanged, preserved after a deterministic transform, deleted by
an explicit product decision (and counted), or blocked.

### Blocked rows fail closed

A row with multiple plausible owners, a missing required parent, a dependency
cycle, cross-Creator sharing, or un-attributable communication history
returns a `MigrationPreflightBlock` carrying a
`CreatorDatabaseUpgradeBlockReason` variant and a stable support code, without
writes. Do not pick a likely owner, silently detach data, or lower an
invariant to make the migration pass.

### Durable-store discovery

Before declaring an upgrade safe, enumerate durable stores and lock/backup
artifacts from current runtime owners and source. Do not preserve a file
inventory in guidance. `platform-runtime/src/data_upgrade.rs` owns the public
local-data state; Web copy never names files or schema internals. Vault
credentials stay untouched, and a Creator migration never rewrites remote state.

### Evolution record

Produce this before implementation:

```text
current schema owner:
supported source versions:
adjacent registry edges:
fresh/current open behavior:
preserved durable facts:
typed destructive impact:
typed blockers:
current validator:
```

## Checklist

- [ ] Change classified with release, distribution, and storage evidence; reset only with positive proof per DATA-02.
- [ ] Released DDL, markers, fingerprints, fixtures, and edges untouched; the change is a new appended version.
- [ ] New edge is exactly `from -> from + 1`, registered in order, and frozen with a header naming its app release.
- [ ] No `#[serde(default)]` or other shim tolerating an old persisted shape; a migration transforms it instead.
- [ ] `CURRENT_SCHEMA_VERSION` is the only new current-version literal; consumers reference the constant.
- [ ] Fresh creation produces the current format with no registry involvement.
- [ ] Impact counts are named, deterministic, and built from persisted facts; ambiguous rows return typed blocks.
- [ ] Every current durable-store owner was discovered from source; vault secrets and remote state stay untouched.
- [ ] The evolution record is written before implementation starts.
