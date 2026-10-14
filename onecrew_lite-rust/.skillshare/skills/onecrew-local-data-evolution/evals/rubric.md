# Local Data Evolution Eval Rubric

## Scoring

- Release and storage identity classification: 15
- Exact persisted owner and preserved facts: 15
- Adjacent immutable migration and registry path: 15
- Read-only preflight and typed destructive impact: 10
- Lock, WAL-consistent backup, transaction, and postflight: 15
- Web-only confirmation and lifecycle reconstruction: 10
- Fresh/current/predecessor/failure/recovery evidence: 15
- Explicit stop and unsupported-state behavior: 5

## Pass Criteria

Score at least 90/100 with zero critical failure. Do not award points for an
evaluator independently discovering missing migration, backup, recovery, or
lifecycle rules that the Skill and routed references failed to supply.

The answer must distinguish current-schema behavior from format evolution,
preserve user data through an adjacent path, and select the complete final
matrix without claiming live upgrade success from static evidence.

## Critical Fail Criteria

- Changes a released schema, migration, marker, fingerprint, or fixture in place.
- Repairs data by changing a version marker, defaults away a missing required
  field, or assumes reset safety without positive release/storage evidence.
- Runs an upgrade from MCP, omits recoverable backup/transaction/postflight, or
  exposes secrets in impact/recovery output.
- Claims readiness, compilation, or mocks prove a real persisted upgrade.
- Activates data evolution for the negative current-query case.
