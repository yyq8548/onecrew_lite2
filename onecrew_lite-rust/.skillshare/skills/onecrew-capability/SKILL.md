---
name: onecrew-capability
description: Implement typed provider operations through transport, client, facade, and runtime toolkit function. Excludes Creator-only policy, operator login, and X Chat cryptography.
---

# OneCrew Capability

## Goal

Contain provider churn below a stable application boundary while delivering one typed vertical slice.

## Hard Constraints

- **CAPABILITY-01** Preserve `transport -> client -> platform facade`; a capability ends at the facade plus a `platform-runtime` toolkit function (`src/toolkits/`), beside the account and credential workflows `platform-runtime` already owns.
- **CAPABILITY-02** Keep wire data, authentication, headers, cookies, signing, retries, and session mechanics in the transport owner.
- **CAPABILITY-03** Keep typed provider models, parsing, pagination, and operation orchestration in the client owner.
- **CAPABILITY-04** Expose only narrow application-facing behavior through the provider facade.
- **CAPABILITY-05** Do not map provider results to canonical Creator DTOs at read time. The platform crate publishes its input checks (counts, queries, identifier formats, criteria, locators) and constraint constants and extracts protocol-native failure evidence as `DmDiagnostic`; a toolkit read applies those checks, plus its own handle trimming, `@` stripping and blank-value checks, before admission, delegates once to the shared read orchestration in `toolkits/read.rs` (YouTube to its credential read, which also commits credential health), and reports `PlatformReadError`. It never reimplements transport, facade or evidence mechanics or names its own failure category (ADR 0017).
- **CAPABILITY-06** Keep provider response types out of Creator and frontend contracts. A peer-facing toolkit read returns `PlatformNativeData`, the platform model serialized under its own field names, which an MCP binding carries as an open object, never as a shared cross-platform model; runtime orchestration such as Creator save keeps the typed model. The runtime re-exports the constraint constants, checks and input types a peer needs.
- **CAPABILITY-07** Route product confirmation, idempotency, audit, durable state, and readback policy through the owning runtime.
- **CAPABILITY-08** Preserve stable external identity and provenance across every mapping boundary.
- **CAPABILITY-09** Fail closed on unsupported target kinds, provider challenges, authentication loss, or ambiguous write outcomes.

## Related Skills

- onecrew-creator — The operation feeds a Creator use case or durable fact.
- onecrew-operator-sessions — Authentication, proxy, browser profile, or session lifecycle changes.
- onecrew-x-chat-crypto-vendor — X Chat cryptography or vendored compatibility changes.
- onecrew-peer-adapters — The operation surfaces through a peer projection; provider reads become typed operation-registry entries rather than a new public tool or an untyped provider dispatcher.

## Workflow

1. Define the stable application result and product states before protocol edits.
2. Implement bounded transport mechanics, then client parsing/orchestration, then the narrow facade with its input checks, constraint constants and evidence extraction.
3. Add the `platform-runtime` toolkit function: validate input with the platform checks, delegate to the shared read orchestration, return `PlatformNativeData` to peers or the typed model to runtime orchestration with stable provider IDs intact, and report `PlatformReadError`.
4. For Telegram target work, read [Telegram Target Semantics](references/telegram-target-semantics.md).
5. Connect an owning runtime only when durable facts or guarded actions are required.
6. Test each changed layer, mappings, unsupported states, and secret-free evidence for every error variant; then hand verification to `onecrew-verify`.

## When NOT to use

- Do not use it for Creator persistence or workflow policy without a provider operation.
- Do not use it for account onboarding, service-key capture, or desktop presentation.
- Do not use it for vendored X Chat cryptography.
