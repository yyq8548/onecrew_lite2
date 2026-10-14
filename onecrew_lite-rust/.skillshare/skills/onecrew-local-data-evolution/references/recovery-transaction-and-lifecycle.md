# Recovery, Transaction, and Lifecycle

Keep the upgrade recoverable across concurrent processes, SQLite WAL state,
transaction failures, and runtime reconstruction. The execution machinery is
`crates/creator-runtime/src/repository/sqlite/schema/upgrade/` (orchestrator and
backup), the directory lock and public projection are
`crates/platform-runtime/src/data_upgrade.rs`, and service startup owns the upgrade boundary in
`crates/local-service/src/startup.rs`.

## Critical Rules

The full adjacent chain runs in one `BEGIN IMMEDIATE` transaction with
exactly one commit. A per-step commit strands a mid-chain failure between
released versions with no reopenable identity.

```rust
// ❌ FORBIDDEN: an edge that owns its own transaction — a later edge's
// failure can no longer roll the database back to the source version.
fn migrate(connection: &Connection) -> Result<(), String> {
    connection.execute_batch("BEGIN IMMEDIATE; ALTER TABLE ...; COMMIT;")
        .map_err(|error| error.to_string())
}

// ✅ CORRECT: the orchestrator owns the only transaction; steps receive it
// (schema/upgrade/orchestrator.rs, simplified).
let transaction =
    connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
apply_forward_migrations(&transaction, from_version, CURRENT_SCHEMA_VERSION)?;
validate_current_schema(&transaction)?;
validate_integrity(&transaction)?;
transaction
    .execute_batch(&format!("PRAGMA user_version = {CURRENT_SCHEMA_VERSION};"))?;
transaction.commit()?; // the single commit
```

The recovery point precedes migration writes and is taken from the locked
snapshot through the SQLite online-backup API, so committed WAL content is
included and no writer can commit between the backup and the migration.

```rust
// ❌ FORBIDDEN: copying the main file — WAL content committed but not yet
// checkpointed is silently missing from the "backup".
std::fs::copy(&db_path, &backup_path)?;

// ✅ CORRECT: hold the immediate transaction (RESERVED write lock), then
// back up through a second read-only connection and validate the result
// (schema/upgrade/orchestrator.rs + backup.rs, simplified).
let backup_source =
    Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
create_online_backup(&backup_source, &backup_path)?; // Backup::step(-1)
if inspect(&backup_path) != expected_upgrade_required_status {
    cleanup_backup_artifacts(&backup_path);
    return Err(/* LOCAL_DATA_BACKUP_INVALID, storage_upgraded: false */);
}
```

Unsupported or newer persisted state is rejected with typed recovery
guidance; a version marker is a claim about structure, and rewriting the
claim never repairs the structure.

```rust
// ❌ FORBIDDEN: "repairing" a database the build cannot understand by
// bumping the marker — every consumer now trusts a lie.
connection.pragma_update(None, "user_version", CURRENT_SCHEMA_VERSION)?;

// ✅ CORRECT: fail closed with a typed status the Web can present
// (schema/inspection.rs).
return CreatorDatabaseStatus::Unsupported {
    version,
    reason: "local business data was created by a newer application version"
        .into(),
};
```

## Quick Reference

| Concern | Rule |
|---|---|
| Sequence | Service process ownership, read-only preflight, startup admission, directory lock, locked revalidation, backup, one transaction, one commit, postflight, reconstruction |
| Writer ownership | `transaction_with_behavior(TransactionBehavior::Immediate)` before the first mutation; busy maps to a typed `Busy` failure, never a queue |
| Steps | Receive the existing transaction; never begin, commit, checkpoint, or open a second write transaction |
| Backup | Same directory as the source, unique name, `create_new` with mode `0o600`, `Backup::step(-1)`, checkpointed standalone, validated by `inspect` |
| Backup failure | Remove incomplete artifacts; the source stays authoritative; never overwrite an existing recovery point |
| Locks | Local-service holds process ownership; `.local-data-lifecycle.lock` (fslock) serializes storage construction and upgrade across processes; hold it for the complete open or upgrade operation |
| Revalidation | Under the lock, re-run version, integrity, and impact checks; a changed impact aborts with `LOCAL_DATA_SOURCE_CHANGED` |
| Failure boundary | `storage_upgraded` is true only after the commit; post-commit failures keep the current format and the backup |
| Startup gate | Local-service startup alone executes required supported upgrades in `spawn_blocking`; business requires Ready; HTTP and Web expose read-only status |
| MCP | No upgrade, migration, reset, backup, or raw-database operation is exposed by the compiled MCP registry |
| Copy | Runtime projections remain secret-free and storage-agnostic; Web fixed presentation belongs to locale catalogs, with stable support codes kept separate |

## Common Patterns

### The required sequence

1. Inspect every durable store read-only (`inspect_data_dir`): no files,
   backups, locks, or write transactions created.
2. Return `Fresh`, `Ready`, `UpgradeRequired` with typed impact, or
   `Blocked`/`Unsupported` with a typed reason and support code.
3. Local-service startup automatically claims the required supported upgrade after
   owner and old-process checks. Compatible data starts directly. No frontend or
   MCP action dispatches an upgrade; no binary-version-only trigger exists.
4. The lifecycle coordinator blocks ordinary account, Creator, mail, and
   outreach operations for the duration.
5. Acquire `.local-data-lifecycle.lock` while the service owns its process
   lease; a CLI, test process, or stale binary can still share the directory.
6. Under the lock, re-run version, safety, impact, writable-directory, and
   space checks (`probe_backup_environment` requires roughly twice the
   database-plus-WAL size plus a fixed margin).
7. Open the `BEGIN IMMEDIATE` transaction, revalidate the source, take and
   validate the online backup, run the full chain, validate the final
   format, stamp `user_version`, commit once.
8. Re-inspect all durable stores and reconstruct shared runtimes under the
   service startup gate before admitting business work.
9. Retain the recovery point and return a secret-free result distinguishing
   completed, already current, and applied-but-initialization-failed.

### Failure semantics by phase

| Failure point | Storage result | Required report |
|---|---|---|
| Preflight, lock, space, or backup | Original format authoritative | `storage_upgraded = false`; no partial backup or migration write |
| Migration SQL or final validation | Transaction rolls back to the original format | `storage_upgraded = false`; retry is safe after the blocker is fixed |
| Commit succeeded, postflight or reconstruction failed | Current format authoritative, backup retained | `storage_upgraded = true`; retry initialization, never the migration |

Never auto-restore a backup after commit, reopen the old format as a
fallback, or continue with partially initialized runtimes. On next start the
database is current; only postflight initialization retries.

### Busy is an answer, not a queue

Exclusive service ownership and the data-directory lock reject concurrent writers.
Revalidate preflight under the upgrade lock, and preserve SQLite busy/locked errors
as typed failures. Do not silently queue a second upgrader.

### Startup owns the gate

Local-service startup is the sole application executor of shared
`LocalPlatform::upgrade_data_dir`. Inspecting, Upgrading, Ready, and Failed are
service projections. Business requires Ready; failures remain blocking even if
storage committed. Limited status and MCP diagnostics need no webpage.
`data status` is read-only, and HTTP/MCP expose no upgrade, backup, or reset action.
Frontend code never inspects or modifies SQLite. Keep public recovery output
secret-free; do not leak filenames or internal schema markers to ordinary UI copy.

### Recovery record

Capture these states in tests and handoff evidence:

```text
preflight state and impact:
lock owner and revalidation:
backup location class and independent validation:
transaction begin and commit point:
pre-commit failure result:
post-commit failure result:
postflight store inspection:
runtime reconstruction and startup readiness:
```

## Checklist

- [ ] Preflight is read-only and returns a typed state before any file, lock, or backup exists.
- [ ] The data-directory lock is held for the upgrade and revalidation reruns under it; a changed impact aborts.
- [ ] The backup is online-API, WAL-inclusive, private, uniquely named, taken under the write lock, and validated by `inspect`.
- [ ] Exactly one `BEGIN IMMEDIATE` transaction and one commit cover the whole chain; steps never manage transactions.
- [ ] `user_version` is stamped by the orchestrator inside the transaction, never by a step and never as a repair.
- [ ] Every failure carries a typed kind, stable support code, and correct `storage_upgraded` boundary.
- [ ] Post-commit failures keep the current format and the backup; nothing auto-restores or falls back to the old format.
- [ ] Process ownership prevents duplicate service startup; storage contention returns typed busy at the file lock and SQLite writer lock.
- [ ] Only local-service startup executes upgrades; CLI, HTTP, MCP, and Web expose no upgrade action, and status copy stays secret-free.
