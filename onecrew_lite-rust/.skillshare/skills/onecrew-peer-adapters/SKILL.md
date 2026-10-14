---
name: onecrew-peer-adapters
description: Project shared runtime contracts through HTTP/MCP and selected Rust-to-TypeScript codegen. Excludes domain policy, provider mechanics, and layout-only UI.
---

# OneCrew Peer Adapters

## Goal

Expose one canonical runtime use case through thin HTTP and MCP peers without
creating an adapter-owned business model.

Readiness checks anchors and assets, not task authorization or semantic parity.
Treat reported dirty paths as scope evidence: preserve unrelated work and
coordinate overlapping owners; authorized adapter edits do not block readiness.

## Hard Constraints

- **ADAPTER-01** Route both peers through the same owning runtime use case. Stop and hand off when that typed owner is absent.
- **ADAPTER-02** Keep domain rules, persistence, normalization, confirmation, idempotency, audit, and provider readback in the runtime owner.
- **ADAPTER-03** Keep the desktop on typed HTTP calls; never route GUI behavior through MCP or loopback HTTP.
- **ADAPTER-04** Map typed surface inputs to one canonical request and map its typed outcome to secret-free surface responses.
- **ADAPTER-05** Permit an erased argument object only at the registered generic executor boundary: resolve the exact operation and surface, validate its exact schema, decode its registered Rust input, then call the typed owner.
- **ADAPTER-06** Derive shared IDs and navigation descriptions from the runtime capability catalog; MCP owns protocol surfaces, bindings, schemas and search. HTTP projects navigation directly from that catalog. Never pin inventories in this Skill or infer execution from a directory entry.
- **ADAPTER-07** Give every registry operation one closed owner, stable ID, exact input/output schema, effect/secret/approval classes, connection requirement, surface allowlist, and typed binding.
- **ADAPTER-08** Keep catalog discovery, explicit presentation, execution, connection lookup, and App-only interaction authority separate according to the executable surface contract. Presentation never grants execution authority.
- **ADAPTER-09** Keep MCP Apps resources as content-addressed UI templates. Retired or malformed resources fail closed and never reconstruct expired interaction authority.
- **ADAPTER-11** Preserve runtime confirmation, idempotency, unknown-effect, provider-readback, recovery, warning, and audit semantics in both projections.
- **ADAPTER-12** Keep credentials and secret material out of schemas, DTOs, errors, logs, fixtures, Apps state, and results.
- **ADAPTER-13** Classify domain failures beside the typed binding and keep framework/extractor/serialization handling as a separate thin boundary. Platform read bindings share one protocol-semantic projection of the runtime's `PlatformReadError` that never branches on the platform and carries the platform's evidence; no binding classifies platform failures per platform or inspects platform errors. Use the routed MCP Failure Guidance for this workflow.
- **ADAPTER-14** Delegate native RMCP route lookup, disabled-tool handling, extraction, and invocation to its router. Do not copy dispatch or invent a universal error envelope.
- **ADAPTER-15** Keep the registry split by responsibility behind a thin construction facade; registrations call current typed owners directly. No legacy handler wrapper, alias, fallback, or dual surface is allowed.
- **ADAPTER-16** Prove schema/serialization, surface closure, safe errors, missing write guards, replay behavior, and semantic peer parity. Standard commands and evidence reuse belong to `onecrew-verify`.
- **ADAPTER-17** When an MCP App succeeds through a direct client or one Host but fails after `tools/call` or a user transition elsewhere, use the routed MCP Apps Host Contract Cases. Isolate the Host boundary before editing production, and normalize only fields the registered schema marks optional.
- **ADAPTER-18** Cross-language contract generation shares only deterministic schema and generator mechanics. MCP and Web keep separate explicit root allowlists, manifests, checked-in artifacts, `generate`/`check` commands, and handwritten semantic refinements. Generated artifacts are never hand edited; timestamps, machine paths, locale order, random values, and partial replacement are forbidden. Never generate from every derive, import one frontend's artifacts into the other, or write source during an ordinary build, test, or application launch. Use the routed Cross-boundary contract codegen pattern.

## Scenario Router

| Request | ❌ Wrong first action | ✅ Correct first action |
|---|---|---|
| Change only MCP App card spacing/focus | Edit the MCP registry because the card came from MCP | Route to `onecrew-mcp-app-ui`; peer-adapters stays inactive while resource/interaction contracts are unchanged |
| Change widget interaction kind or `ui://` resource contract | Treat it as layout-only UI work | Own the protocol/resource delta here; pair MCP App UI for the iframe consumer and Web UI only if its Host changes |
| Add a Creator business rule exposed by both peers | Implement the rule twice in command/tool handlers | Stop and add the typed rule to `creator-runtime`, then project that one owner |
| Add one public write projection | Start from a generic JSON envelope | Start from the owner's typed request/result and preserve confirmation, idempotency, readback, unknown-effect, and audit semantics |
| Generate Rust wire types for both frontends | Put DTO roots and generated files in one shared app package | Share canonicalization/generator mechanics, but keep owner-local root manifests and consumer-local artifacts |
| A generated contract is wrong or stale | Patch generated TypeScript or validators until the consumer passes | Change the Rust root, explicit allowlist, or shared generator, regenerate the complete target, then prove byte-stable drift checks |

## Router

| Intent | Read |
|---|---|
| Rust schema manifest, TS declarations, runtime validators, generated HTTP/MCP App contracts, or drift checks | [Cross-boundary contract codegen](references/cross-boundary-contract-codegen.md) |
| MCP failure classification, platform read failure projection, and safe error projection | [MCP Failure Guidance](references/mcp-failure-guidance.md) |
| MCP Apps Host result envelopes, App-only calls, remounts, or invalid response diagnosis | [MCP Apps Host Contract Cases](references/mcp-app-host-contract-cases.md) |

## Related Skills

- onecrew-creator — pair when the projections expose a Creator-owned use case.
- onecrew-web-ui — pair when a typed Web client or React consumer changes.
- onecrew-mcp-app-ui — pair when an iframe client consumes or presents the public MCP Apps contract.
- onecrew-verify — run or reuse schema, serialization, write-safety, and parity evidence.
- onecrew-local-service — handoff when command execution threads, blocking dispatch, or host lifecycle change.

## Workflow

1. Identify the typed runtime owner, request/result, persistence boundary, effect class, and recovery semantics.
2. Define the HTTP and MCP projections, surfaces, authority, schemas, and secret-free failure contract.
3. Implement each peer as typed input mapping, one owner call, explicit outcome classification, and typed output mapping.
4. Reference canonical catalog IDs in MCP bindings; keep protocol metadata in MCP and validate bindings against the independent catalog declarations. Both peers derive navigation from that catalog and execute typed owners.
5. When contract codegen changes, prove explicit roots, deterministic output,
   complete staging validation, atomic replacement or rollback, consumer-local
   artifacts, and non-mutating drift checks.
6. Add branch-adjacent schema, closure, replay, failure, parity, and generator
   failure-path tests.
7. Run focused affected checks, then run or reuse one final owner gate through `onecrew-verify` without packaging.

## When NOT to use

- Do not use it to create runtime/domain/persistence policy.
- Do not use it for Web or MCP App React composition when adapter contracts stay unchanged.
- Do not use it for provider protocols, account authentication, vault mechanics, or native OS work.
- Do not use historical migration plans as current adapter instructions.
