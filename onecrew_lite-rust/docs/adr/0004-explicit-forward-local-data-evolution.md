# ADR 0004: Explicit forward local-data evolution

## Status

Accepted. The user-confirmation requirement for upgrades classified safe by
preflight is superseded by ADR 0014; the forward-only, backup-first, validation,
and fail-closed mechanics remain current.

## Context

OneCrew CLI releases can encounter persisted data created by older, current,
future, damaged, or locally modified builds. Automatic startup migration makes
failure hard to explain and can mutate the only copy before the user sees the
impact.

## Decision

Persisted-format evolution is forward-only and append-only between released
formats. Ordinary runtime open validates the current format and never migrates.
A supported upgrade is initiated by `local-service` after a read-only composite
preflight. ADR 0014 governs whether that initiation is automatic or user-led.

The upgrade path acquires exclusive lifecycle and data-directory ownership,
revalidates after locking, creates a recoverable WAL-consistent backup, applies
the complete adjacent migration chain transactionally, and validates the
result. Unsupported, future, corrupt, fingerprint-mismatched, or ambiguous
state fails closed rather than using compatibility aliases or fallback reads.

## Consequences

- Each released persisted format remains independently recognizable and
  testable.
- Upgrade impact and recovery information are product-visible.
- New schema or serialized-state changes require an adjacent migration or an
  explicit proof that no released user data can exist.
- An unshipped format proposal may be withdrawn only with recorded positive
  release and storage proof. Withdrawal keeps the last released/persisted
  format current; it does not add a reverse migration, marker repair, or
  compatibility reader.
- Application and data-format versions remain separate release identities. A
  later product-only release keeps the current data-format version unchanged.
- Migration mechanics remain below the CLI host; `local-service` owns lifecycle
  coordination, while the CLI and shared Web project the safe/blocked recovery
  state defined by ADR 0014.

## Amendment (2026-09-11)

Renaming `next_tool` guidance values to registry operation IDs on 2026-09-11
is not a persisted-format change: no schema version, marker, fingerprint, DDL,
index, or encoding moves, and no stored row is rewritten. Of the renamed
values only the contact-route delete operation persists one, inside the
terminal `result_json` of a completed stale or expired row. Those rows keep
replaying the name they were written with, which still points at the step a
user must rerun; new rows carry the registry ID. No alias, compatibility
reader, or backfill is added, and `CURRENT_SCHEMA_VERSION` is unchanged.
