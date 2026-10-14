# Frozen navigation contract

These JSON fixtures were extracted from the actual production registry export
at P11 commit `dab48fad`, before the P12 catalog migration. The export was
produced by `registry_contract_snapshot_is_exportable` with
`MCP_REGISTRY_SNAPSHOT_OUT`.

- `directory-full.json` is the former HTTP directory (MCP `full` surface).
- `directory-surfaces.json` retains all seven former MCP surface directories.

Object content and array ordering are both part of the frozen contract. Runtime
catalog construction never loads these fixtures. Adapter tests compare their
real projections against the frozen data; they must not regenerate expected
results from the new declarations.
