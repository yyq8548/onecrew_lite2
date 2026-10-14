---
name: onecrew-codex-plugin
description: Maintain bundled Codex plugin staging, cache identity, CLI installation, and registration diagnostics. Excludes underlying MCP tool contracts and ordinary Creator work.
---

# OneCrew Codex Plugin

## Goal

Ship the MCP connection and single router Skill through the Codex plugin with
private staging, generation-pinned installation, and verified cache readback.

## Hard Constraints

- **CODEX-01** Change the staged version whenever staged bytes change;
  verify external CLI cache behavior against the installed Codex version
  rather than treating a historical reload observation as a permanent API.
  `development/release/version.json` is the one CLI/web/plugin base-version
  source; `development/scripts/sync-release-version.mjs` updates or checks the
  ecosystem manifests and locks required to repeat that value.
- **CODEX-02** Stamp the staged version as
  `<base>+codex.<stable-content-digest>` (Codex's own cachebuster
  convention): replace any previous `+` suffix, never append or increment
  numeric components, and use a stable digest protocol (SHA-256),
  never a std hasher.
- **CODEX-03** Digest the complete staged executable and plugin assets, using a stable SHA256 protocol. Emit immutable complete generations with marketplace index and bin/onecrew(.exe); never mutate a live installed generation.
- **CODEX-04** Keep embedded and staged plugin sources secret-free. Launch embedded stdio MCP through the CLI with the selected data root; service/control credentials remain in private runtime state, never plugin-cache business storage.
- **CODEX-05** The plugin carries one short `onecrew` router, not one Skill per
  business use case. It normalizes non-English intent, splits independent
  outcomes into atomic capability queries with explicit toolkit filters, and
  keeps execution values out of search. It consumes returned toolkit guides
  and bounded workflow hints directly, batch-reads only operations it will
  execute with `onecrew_tool_get`, and executes independent calls in one
  `onecrew_tool_execute` batch while keeping data-dependent rounds separate.
  `onecrew_result_present` accepts only an execute-minted result reference and is
  called only when the user must see or act on the result; after presenting,
  the router stops without narrating the card again. It never duplicates
  field tables, operation inventories, workflow-hint inventories, or schema
  revisions.
- **CODEX-06** Distinguish the live surface from the cached surface. Fixed
  meta-tool schemas and catalog details come from the running server and never
  wait for a plugin refresh; only the router Skill, its references, the
  manifest, and `.mcp.json` sit in the cache. The ordinary live business
  surface has five model-visible router tools; the plugin's stdio connection
  lists five bootstrap tools ahead of them, so the Router names ten meta tools
  in total. The App-only result-state tool is not a Router verb and must never
  appear in model guidance.
- **CODEX-07** Verify changed cached guidance in a new Codex session; do not
  claim an active session refreshed from a successful install alone. Record
  the tested Codex version when diagnosing reload behavior.
- **CODEX-08** The setup document is for first-time install, a lost
  marketplace configuration, or other MCP clients - never present routine
  updates as requiring a new setup link, and never present a restart
  without a version change as sufficient.
- **CODEX-09** Only explicit `onecrew plugin install` invokes Codex installation and full readback. `plugin status` is cached verification; `--refresh` explicitly queries Codex. `setup` / `setup --copy` never install. Respect explicit --data-dir. Normal serve/status/MCP never installs, probes, or recursively launches Codex.
- **CODEX-10** Stage/status/install share one generation owner. Hold it from
  the initial staged-version read through `plugin add` and a second
  staged/installed/marketplace readback; success requires the staged
  generation to remain pinned and the installed version to equal it. Never
  infer success from the CLI command exit or restart observations. The owned
  generation lease belongs to the actual filesystem/CLI worker, not the
  caller future, so caller cancellation cannot release it while work
  continues.
- **CODEX-11** Guidance travels through the live MCP surface (instant) and
  the plugin cache (new session). Developer messages outrank tool descriptions.
- **CODEX-12** Keep initialize instructions short and self-contained; external
  client truncation budgets require version-specific evidence. Lead
  with direct connection lookup, batch `onecrew_tool_get` for known operation IDs,
  exact `onecrew_tool_execute`, and `onecrew_result_present` only for execute-minted
  user-visible results; use search only for unknown capabilities. It carries no exhaustive
  inventory, and do not depend on a trailing inventory (text owned by
  onecrew-peer-adapters).
- **CODEX-13** Keep stable cross-cutting guidance behind at most one
  intent-selected router reference: toolkit connection lifecycle or OneCrew
  writes. Ordered use-case guidance stays in live search workflow hints;
  operation-specific policy stays in operation details. Do not restore retired
  per-use-case Skill names as aliases.

## Related Skills

- onecrew-peer-adapters — the MCP tools, schemas, or surfaces
  themselves change.
- onecrew-capability — the setup capability, token lifecycle, or
  loopback boundary changes.

## Router

- Cache semantics, update loop, verbs, acceptance checks:
  [Install cache and updates](references/install-cache-and-updates.md)

## Workflow

1. Classify the change: bundled sources, staging logic, setup document, or
   an update-flow diagnosis.
2. For staged-content changes, rely on the digest stamp; never hand-edit a
   staged copy or bump numeric versions as a cachebuster.
3. For diagnosis, inspect `tools/list`. Old fixed meta-tool names/schemas or
   `onecrew_tool_get` operation-contract fingerprints indicate an old running
   server; finish work, stop the old service, and restart. Separate those live-surface symptoms
   from cache-surface symptoms (old router text means the installed
   plugin is old - reinstall).
4. Update the setup document and its pinned tests together whenever the
   Codex section changes.
5. Verify packaging and explicit install/status/setup behavior through the
   `onecrew-codex` crate gate; native Codex readback requires live evidence.

## When NOT to use

- Do not use it for MCP tool or schema contract work (peer adapters own
  that, including `$ref` inlining).
- Do not use it for Claude-side project skills or `.claude` mirrors.
