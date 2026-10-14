# Cross-boundary contract codegen

Use this reference when Rust wire DTOs, JSON Schema manifests, generated
TypeScript declarations, standalone validators, or artifact drift gates change.

## Critical Rules

The schema/canonicalization crate and Node generator own mechanics only. An
adapter owner selects every root explicitly and proves its Serde direction.
The MCP App and Web each own their manifest and checked-in output; neither
frontend imports the other's generated directory.

```text
canonical Rust DTO or exact adapter projection
        ↓ explicit serialize/deserialize root allowlist
canonical schema manifest with final-document hashes
        ↓ shared deterministic generator
consumer-local types and validators
        ↓
handwritten feature client and semantic refinements
```

## Quick Reference

| Concern | Rule |
|---|---|
| Root selection | Explicit, secret-free, consumer-evidenced allowlist |
| Schema identity | Final canonical document after ID, title, and numeric policy |
| Output ownership | Checked-in under the consuming app; never cross-imported |
| Determinism | No timestamps, absolute machine paths, locale ordering, randomness, or environment-dependent banners |
| Generation | Explicit command, complete staging validation, atomic replace |
| Drift | Temporary regeneration plus byte comparison only |
| Runtime parsing | Generated shape validator followed by handwritten semantic refinements |
| Command clients | Remain feature-owned until a separate command-wrapper generator is authorized |

## Common Patterns

```text
❌ Scan every JsonSchema derive and export it.
✅ Export only named secret-free wire roots with an evidenced consumer.

❌ Treat generated TypeScript as runtime validation.
✅ Generate declarations and standalone validators from the same manifest.

❌ Let Web import apps/mcp-ui/generated because the payload looks equal.
✅ Generate independent Web artifacts from its own root manifest.

❌ Replace Host normalization, cross-field consistency, or error diagnosis with
   a broader schema.
✅ Normalize only protocol-optional fields, validate shape, then run handwritten
   semantic invariants.

❌ Rewrite tracked artifacts from build.rs, cargo build/test, Vite build, or app
   startup.
✅ Expose explicit generate and check commands; check regenerates in a temporary
   directory and byte-compares without writing tracked files.

❌ Accept partial output after one root fails.
✅ Generate and validate the complete target in a sibling staging directory,
   then atomically replace it or leave the prior generation intact.

❌ Patch a generated declaration or validator because one frontend fails.
✅ Fix the canonical Rust DTO, exact consumer root allowlist, or shared generator;
   regenerate the complete consumer target and keep handwritten semantics outside
   generated files.

❌ Add a generation timestamp or write directly to the tracked output directory.
✅ Ensure identical source and pinned dependencies produce identical bytes;
   stage, validate, compare or atomically replace, and restore the prior complete
   output if replacement fails.
```

## Checklist

- [ ] Generator dependencies and schema version are pinned.
- [ ] A generated artifact names its source owner and is never edited directly.
- [ ] Two generations from identical source produce byte-identical output with
  no time, machine, locale, random, or environment-dependent content.
- [ ] The final canonical schema is hashed after IDs, titles, and numeric wire policy.
- [ ] Serialize versus deserialize direction, Serde tags/renames,
  null-versus-omission, closed enums, unknown-field rejection, and JS-safe
  integers.
- [ ] Generated declarations pass TypeScript and every standalone
  validator before replacement.
- [ ] Generator tests cover invalid manifests, stale-output detection,
  non-mutating comparison, atomic replacement, and rollback after replacement
  failure; one failed root leaves the prior complete output intact.
- [ ] Command strings, argument envelopes, feature error mapping, query/UI
  state, and semantic consistency outside generated files unless a separately
  authorized command-wrapper generator owns them.
- [ ] Each consumer's contract drift check and package tests/build pass; MCP App
  generated validators must remain inside self-contained bundle budgets.
