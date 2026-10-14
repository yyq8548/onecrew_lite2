---
name: onecrew-local-data-evolution
description: Evolve persisted schemas, markers, or encodings with forward upgrades and recovery. Excludes current-schema queries and proven storage-equivalent source moves.
---

# OneCrew Local Data Evolution

## Goal

Evolve local data through a recoverable owner-controlled path. Runtime open
validates only; local-service startup owns required preflight-approved safe upgrades, without a webpage.

Readiness validates anchors and evaluation assets. Changed domain paths are
scope evidence, not an authorization or migration-correctness verdict; retain
them for review and run the evolution matrix on the actual delivery.

## Hard Constraints

- **DATA-01** Activate for every durable-format identity or potentially persisted shape change; absence of a migration request is never reset evidence.
- **DATA-02** Allow reset only with positive release and storage proof that no persisted user data can exist; otherwise require evolution.
- **DATA-03** Keep fresh creation on the current schema and normal open free of migration writes; reject unsupported versions explicitly.
- **DATA-04** Represent each supported transition as one immutable adjacent forward step and execute longer paths through the ordered registry.
- **DATA-05** Never change released DDL, markers, fingerprints, encodings, fixtures, or migration edges in place; append a new version instead.
- **DATA-06** Map every schema-version bump to a new application release version and keep the release-to-schema contract auditable.
- **DATA-07** Separate current schema construction, historical transforms, planning, and upgrade orchestration into distinct owners.
- **DATA-08** Complete read-only preflight and typed impact before backup/write, then revalidate version, safety, and impact under the data lock.
- **DATA-09** Create a private WAL-consistent online backup before migration writes and preserve it as recovery evidence.
- **DATA-10** Apply the full adjacent chain in one `BEGIN IMMEDIATE` transaction with exactly one commit and typed pre/post-commit failures.
- **DATA-11** Only local-service startup executes required upgrades after exclusive owner/old-process checks. Compatible data starts directly. Preserve preflight, backup, adjacent upgrades, validation, and recovery. Inspecting/Upgrading/Ready/Failed gates business; failures block business even after partial commits. Data status is read-only; expose no manual CLI/HTTP/MCP/web upgrade action.
- **DATA-12** Keep upgrade DTOs, support codes, logs, audits, and UI copy secret-free; fail closed on ambiguous lineage, ownership, impact, or target state.

## Related Skills

- onecrew-creator — Creator facts, preservation semantics, or repository behavior changes.
- onecrew-native-cli — Backup permissions, native paths, or lifecycle differ by OS.
- onecrew-web-ui — The startup upgrade or recovery presentation changes.
- onecrew-verify — Run the complete evolution matrix before handoff.

## Router

- Version identity, adjacent registry, reset proof, typed impact: [Schema Path and Impact](references/schema-path-and-impact.md)
- Lock, backup, transaction, failure semantics, Web gate: [Recovery, Transaction, and Lifecycle](references/recovery-transaction-and-lifecycle.md)
- Fresh/current/predecessor/failure/desktop/MCP evidence: [Verification Matrix](references/verification-matrix.md)

## Workflow

1. Inventory every durable store and classify the change using release, distribution, and storage evidence.
2. Freeze preserved facts, destructive impact, source identity, target identity, and ambiguity stops.
3. Update fresh construction, then append immutable adjacent migration and registry entries.
4. Implement read-only planning and the locked backup/transaction/postflight sequence.
5. Wire the local-service startup/recovery gate, lifecycle reconstruction, and secret-free outcomes; use the existing one-attempt admission policy instead of adding another confirmation or retry loop.
6. Run focused affected rows during implementation, then run or reuse the
   complete matrix once on the final diff before handoff.

## When NOT to use

- Do not use it for current-schema entity/query work with no persisted identity or encoding change.
- Do not use it for a proven structure-only module move with byte- and behavior-equivalent persisted data.
- Do not authorize destructive reset without the positive evidence required by `DATA-02`.
