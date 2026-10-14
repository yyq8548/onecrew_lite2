# Verification Matrix

Run affected rows while implementing and run or reuse the complete applicable
matrix once on the final persisted-format diff.

## Preconditions

- The change was classified and implemented through this skill; source,
  target, and impact identities are frozen.
- A focused fixture exists for each supported predecessor and each
  data-dependent blocked branch. Predecessor fixtures are built by the
  `downgrade_reference_to_vNN` helpers in
  `crates/creator-runtime/src/repository/sqlite/schema/migrations/validation.rs`
  against frozen DDL; a new edge owes a new downgrade helper and tests.
- Exact test filters are resolved from current source, not from memory: the
  creator-runtime suite lives in
  `src/repository/sqlite/tests/schema_contract.rs`, the platform-runtime
  suite in the `tests` module of `src/data_upgrade.rs`, and service
  lifecycle tests in `crates/local-service/src/startup.rs`
  and `state.rs`; automatic admission belongs to service startup tests, with
  read-only projection tests in the webpage.
- No live-account, packaging, signing, `cargo clean`, or destructive action
  is part of this matrix; those remain explicit user actions.

## Workflow

1. Enumerate the supported window from source: `FORWARD_MIGRATIONS` in
   `schema/migrations/mod.rs`, `EARLIEST_MIGRATABLE_SCHEMA_VERSION` in
   `schema/inspection.rs`, and `CURRENT_SCHEMA_VERSION` in `schema.rs`.
   Every enumerated predecessor and blocked branch must map to a test.
2. Prove every row of the state and path matrix below with a focused test or
   recorded manual evidence.
3. Prove every row of the preservation and impact matrix below.
4. Prove every row of the surface and lifecycle matrix below.
5. During implementation, run only the affected state, path, impact, and
   lifecycle rows. For the final diff, ask `onecrew-verify` to select
   one complete owner gate per touched crate/application and reuse an
   identical prior `PASS` identity.
6. When Rust DTOs, lifecycle composition, HTTP commands, or frontend gate
   contracts changed, fan out through the owning consumers and hand-audit any
   cross-language mirror that no compiler checks. Standard commands remain in
   the verification owner, not this domain matrix.
7. Assemble handoff evidence: exact command, working directory, evidence
   identity/reuse source, exit result, fixture identity, and the behavior each
   executed row proved.

## State and Path Matrix

| Case | Required evidence |
| --- | --- |
| No local data | Fresh current stores are created; no backup or migration registry runs |
| Current version | Normal open succeeds without writes; explicit upgrade returns already current without a backup |
| Same-version marker, fingerprint, or structural drift | Fail closed without writes or reset; require a new version and adjacent edge |
| Reused application version with a changed schema identity | Fail release validation; bump the product version and record the release-to-schema mapping before packaging |
| Proven unshipped proposed format, with the predecessor still current | Record positive release and storage evidence; remove the proposal rather than adding a reverse edge; fresh creation and current open stay on the predecessor; explicit upgrade returns already current without a backup |
| No-data status cannot be proven | Treat as potentially persisted and return UpgradeRequired or Blocked; never reset |
| Each adjacent predecessor | One edge transforms to its exact successor and passes successor validation |
| Oldest supported to current | Ordered adjacent edges run inside one transaction and produce one recovery point |
| Unsupported older version | Preflight returns blocked or unsupported before backup and writes |
| Newer version | Preflight returns a typed newer-version failure before backup and writes |
| Unsafe or ambiguous legacy facts | Preflight returns the exact typed block reason and leaves storage byte-semantically unchanged |
| Active WAL content | The recovery point and upgraded database both contain the latest committed WAL rows |
| Concurrent application or data-directory owner | The second opener or upgrader returns busy without backup or writes |
| SQLite writer already active | `BEGIN IMMEDIATE` ownership fails safely without partial migration |
| Unwritable directory | Preflight or locked revalidation returns not writable without artifacts |
| Insufficient space | Space validation returns a typed failure without backup or writes |
| Backup creation or validation failure | The source remains authoritative and incomplete backup artifacts are removed |
| Any adjacent step failure | The outer transaction rolls back every earlier edge and reports `storage_upgraded = false` |
| Final integrity validation failure | The outer transaction rolls back and the old version remains reopenable |
| Post-commit store inspection failure | The current schema remains committed, the backup remains valid, and `storage_upgraded = true` |
| Post-commit runtime construction failure | Restart retries initialization without running a migration or restoring old data |

## Preservation and Impact Matrix

| Concern | Required evidence |
| --- | --- |
| Operator accounts and connections | Public references and configured connection state remain unchanged |
| Vault credentials and secrets | Secret rows remain unchanged and no secret enters logs, DTOs, fixtures, or UI copy |
| Creator origins and community sources | `platform_search` Creator origins keep their discovery, operator-account, and observation-source references, candidate origins keep their discovery and operator-account references, and `operator_account` / `creator_contact_route` community-administrator sources keep their operator-account references, unless listed in approved typed impact |
| Creator business facts | Preserved facts keep stable IDs, provenance, ordering, and relationships |
| Intentional local deletion | Every deleted category and count exactly matches preflight impact and locked revalidation |
| Remote provider state | No remote email, message, relationship, or account mutation occurs |
| Backup | The recovery point opens independently, passes integrity checks, and has private permissions |

## Surface and Lifecycle Matrix

| Surface | Required evidence |
| --- | --- |
| Service startup | Runtime construction is gated until data is fresh, ready, or successfully initialized after upgrade |
| Service automatic admission | Only backup-recoverable, non-remote-deleting preflight with at least one target triggers one attempt; Concurrent callers and repeated status delivery do not duplicate it |
| Web recovery | Blocked, unsupported, failed, or incomplete states never auto-loop; user retry/continue or exit uses the existing lifecycle boundary |
| Web error copy | Failure text is secret-free, storage-agnostic, actionable, and keyed by a stable support code |
| MCP router | No upgrade, migration, reset, backup, or raw-database tool is registered |
| Single instance | A second Web invocation cannot create a second interactive runtime |
| Data-directory lock | A non-Web process still cannot open or upgrade the same data directory concurrently |
| Restart | Completed and post-commit-recovery states reopen the current format without compatibility branches |

## Failure recovery

- Diagnose a failed row at its owner: a wrong impact count in the edge's
  preflight, a rollback gap in the orchestrator, leaked copy in the
  platform-runtime projection. Never weaken a validator, widen a filter, or
  delete a fixture to turn a row green.
- A predecessor fixture that no longer round-trips means the frozen DDL or a
  downgrade helper drifted; restore the frozen artifact rather than adapting
  the assertion to the drifted output.
- Mark an environmental omission (missing tool, platform, or credential) as
  blocked coverage with the exact missing dependency; it is never a pass.
- If any gate exposes an unrelated pre-existing failure, report it
  separately; do not absorb it into this change or skip the gate.

## Verification

- Every supported predecessor, destructive-impact branch, backup failure,
  transaction rollback, and post-commit recovery path has row-level
  evidence.
- One complete final owner gate passed or was identically reused for every
  touched crate/application, with affected fan-out when a shared contract
  changed.
- The desktop DTO mirror audit happened by hand whenever a Rust upgrade DTO
  changed shape.
- The evidence report states exactly what passed, failed, was blocked, and
  was not run.

## Stop condition

Stop when every applicable matrix row has evidence or a recorded blocked
status, the focused and fan-out gates for the touched scope are green, and
the handoff report is written. Packaging, signing, and live external
mutation are out of scope and remain explicit user actions.
