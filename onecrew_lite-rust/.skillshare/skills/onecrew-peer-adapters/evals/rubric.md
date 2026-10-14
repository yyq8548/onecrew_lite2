# Peer Adapter Eval Rubric

## Scoring

- Exact runtime owner and canonical typed use case: 20
- HTTP/MCP request and result mapping: 15
- Registry, surface, authority, and resource placement: 15
- Confirmation, idempotency, readback, and unknown-effect preservation: 15
- Secret-free schema, failure, log, and fixture boundary: 10
- Focused implementation sequence: 10
- Exact affected verification lanes: 10
- Explicit stop and blocked conditions: 5

For a codegen-specific case, score the 15-point HTTP/MCP mapping row against
explicit consumer roots and Serde direction, deterministic schema identity,
complete staging validation, atomic replacement or rollback, consumer-local
artifacts, and non-mutating drift evidence.

## Pass Criteria

Score at least 90/100 and trigger no critical failure. Give no extra credit for
repository archaeology that the Skill and selected references did not route.

The answer must identify one runtime owner, distinguish public adapter
projection from React/widget composition, preserve typed write-safety evidence,
and select only affected verification lanes.

## Critical Fail Criteria

- Routes Web through loopback MCP or invents adapter-local business policy.
- Exposes secret material, bypasses confirmation/idempotency/readback, or treats
  request acceptance as remote completion.
- Reintroduces a legacy handler wrapper, public compatibility alias, guessed
  operation execution, or a generic JSON HTTP executor.
- Makes an authority-bearing result field optional, accepts unknown result
  keys, or treats omitted optional fields as authorization to skip semantic
  state checks.
- Runs a second JSON-RPC bridge client in one MCP App iframe or changes
  production repeatedly without first isolating the Host boundary.
- Claims readiness, compilation, or a mock proves live provider/native behavior.
- Changes product code in the negative routing case.
- Hand-edits generated contracts, introduces time/machine/random content,
  generates tracked source during an ordinary build or launch, accepts partial
  output, or imports one consumer's generated artifacts into the other.
