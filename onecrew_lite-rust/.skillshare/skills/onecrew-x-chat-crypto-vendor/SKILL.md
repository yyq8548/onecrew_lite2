---
name: onecrew-x-chat-crypto-vendor
description: Maintain X Chat cryptography, key recovery, vendored compatibility, and binding parity. Excludes ordinary X APIs, Creator messaging policy, and unrelated vendor updates.
---

# OneCrew X Chat Crypto Vendor

## Goal

Make the smallest compatible X Chat change while preserving wire/storage compatibility, cryptographic invariants, and upstream traceability.

## Hard Constraints

- **XCHAT-01** Read `libs/chat-xdk/AGENTS.md` and `libs/UPSTREAM.md` before changing vendored or protocol code.
- **XCHAT-02** Prefer integration fixes in `crates/x-chat`; edit vendored source only with evidence that the owner lies there.
- **XCHAT-03** Keep Chat XDK crypto-only and keep networking, retries, persistence, ordering, and application policy in callers.
- **XCHAT-04** Preserve existing wire formats and stored ciphertext/key contracts; route any persisted-format change through its explicit evolution owner.
- **XCHAT-05** Never weaken signature verification, downgrade protection, identity binding, bounded parsing, zeroization, or redaction.
- **XCHAT-06** Verify against caller-supplied signing keys and never trust event-carried signing material.
- **XCHAT-07** Parse and bound untrusted wire data before cryptographic processing.
- **XCHAT-08** Implement shared behavior once in Chat XDK core and keep every supported binding at API/behavior parity.
- **XCHAT-09** Regenerate protocol code from schema and never hand-edit generated files.
- **XCHAT-10** Keep Juicebox limited to key storage/recovery and preserve its protocol compatibility.
- **XCHAT-11** Isolate authorized upstream imports, preserve licenses/notices/baselines, and review the complete vendor diff.
- **XCHAT-12** Keep private keys, passcodes, raw key blobs, sessions, tokens, and environment files out of logs, outputs, fixtures, and commits.

## Related Skills

- onecrew-capability — X provider acquisition or facade behavior changes.
- onecrew-local-data-evolution — Stored ciphertext or key formats change.
- onecrew-rust-quality — Pair for generic unsafe, error, lint, public
  API, or rustdoc quality while cryptographic and vendor behavior stays here.
- onecrew-verify — Run parent, vendor, and affected binding matrices.

## Workflow

1. Capture the failing event/fixture or authorized upstream revision and expected compatible behavior.
2. Classify the owner as caller integration, shared core, generated wire, Juicebox, binding, or upstream import.
3. Fix the narrowest owner and add a historical/cross-client compatibility fixture.
4. Propagate core API behavior through bindings and regenerate only through documented commands.
5. Run parent integration plus affected vendor/binding checks and audit the diff for secrets and provenance.

## When NOT to use

- Do not use it for ordinary X REST/GraphQL capabilities outside encrypted Chat.
- Do not use it for Creator messaging policy without cryptographic integration changes.
- Do not use it for a broad vendor or dependency refresh unrelated to a concrete compatibility/security need.
