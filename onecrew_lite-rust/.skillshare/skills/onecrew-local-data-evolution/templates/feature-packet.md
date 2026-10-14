# Local Data Evolution Feature Packet

## Intent and identity

- User intent:
- Persisted store and owner:
- Source format identity:
- Target format identity:
- Release/storage evidence:
- Explicit exclusions and stop condition:

## Preserved and destructive facts

- Facts preserved byte-for-byte or semantically:
- Typed destructive impact:
- Ambiguity and unsupported-state stops:
- Secret/provenance constraints:

## Evolution path

- Fresh construction:
- New adjacent edge:
- Ordered registry path:
- Read-only preflight:
- Revalidation under lock:
- WAL-consistent backup:
- One transaction and commit point:
- Postflight validation:

## Lifecycle and recovery

- Web confirmation gate:
- Single-instance/data-lock boundary:
- Peer/supervisor shutdown and reconstruction:
- Pre-commit failure behavior:
- Post-commit initialization failure behavior:
- Recoverable backup record:

## Evidence

- Focused migration tests:
- Fresh/current/predecessor/failure/recovery rows:
- Web gate and MCP-absence rows:
- Final evidence identity and reuse source:
- Native/live blockers:
- Review identity or reuse decision:

## Critical refusals

- No reset without positive release/storage proof.
- No marker-only repair, compatibility default, or in-place old migration edit.
- No MCP upgrade surface or secret-bearing recovery output.
- No runtime claim from readiness, compilation, or mocks.
