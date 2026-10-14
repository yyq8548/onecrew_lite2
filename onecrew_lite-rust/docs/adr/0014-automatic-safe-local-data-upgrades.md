# ADR 0014: Automatically run preflight-approved local-data upgrades

## Status

Accepted. This amends ADR 0004 and records the product behavior implemented by
`f1e4a94e` (`feat: automate safe local data upgrades`).

## Context

The read-only composite preflight already distinguishes a backup-recoverable,
non-destructive upgrade from blocked, unsupported, busy, or failed storage.
Requiring another confirmation for the safe case delays startup without adding
authority: the application has already proven that it can create a recovery
point and that no remote data is deleted.

React StrictMode and repeated status delivery must not create an upgrade loop,
and a failed or blocked upgrade still needs a visible human recovery path.

## Decision

`local-service`, started by the OneCrew CLI, automatically starts exactly one
local-data upgrade attempt during
startup when all of these server-owned facts are true:

- preflight state is `upgrade_required` with at least one target;
- `recoverable_backup == true`;
- `remote_data_deleted == false`.

The client claims the one automatic attempt before dispatch. StrictMode,
rerender, or repeated status reads cannot start a second attempt. The runtime's
existing lifecycle/data-directory single-flight remains the mutation authority.

Safe automation does not weaken the storage protocol: the owner revalidates
after the write lock, creates and validates a WAL-consistent backup, applies one
transaction, validates complete structural and business invariants before
commit, then performs an independent postflight readback.

Blocked, unsupported, failed, or incomplete states do not auto-retry. The CLI
and shared Web show the typed recovery message and retain the user-owned
retry/continue or exit path. Ordinary runtime open still validates only and
never migrates.

## Consequences

- Safe local data follows the application forward without a redundant startup
  confirmation.
- Failure remains visible and non-looping; automatic retry is not inferred.
- Backup/readback evidence, not client optimism, defines a safe upgrade.
- ADR 0004 remains authoritative for forward-only evolution, recovery points,
  validation, and fail-closed behavior, but not for mandatory confirmation of
  a preflight-approved safe upgrade.
