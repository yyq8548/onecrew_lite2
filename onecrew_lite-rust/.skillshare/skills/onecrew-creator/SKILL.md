---
name: onecrew-creator
description: Implement creator-runtime use cases for the canonical Creator profile, contacts, relationships, conversations, email, persistence, audit, and guarded external actions. Excludes provider-only mechanics, login, and presentation-only UI.
---

# OneCrew Creator

## Goal

Deliver one canonical Creator workflow while preserving global identity, provenance, transaction safety, and peer-adapter parity.

## Hard Constraints

- **CREATOR-01** Keep Creator use cases, DTOs, repository ports, confirmation, idempotency, and audit policy in `creator-runtime`; Creator, contact, relationship, conversation, message, email, and touchpoint facts stay globally canonical.
- **CREATOR-02** Format freshly read provider data into canonical Creator profiles and contact observations at save preparation. Save preparation accepts the selected locator, connection and real provenance directly; it holds immutable typed review data without writing Creator rows, snapshots or save reservations. Persist only the exact prepared rows and routes the user confirms; do not introduce a separate candidate cache or cross-platform selection framework.
- **CREATOR-03** Persist discovered people only through the candidate-save workflow; never permit adapters or UI to write Creator/contact rows directly.
- **CREATOR-04** Preserve stable provider identity, source URL, discovery method, provenance, and collision semantics.
- **CREATOR-05** Use runtime toolkit and composition functions for provider reads, save preparation, and outreach effects; do not restore their retired Creator gateways or discovery snapshots. Creator retains the Email gateway and the conversation thread-read port, whose implementations use runtime-owned credentials and sessions.
- **CREATOR-06** Store only public Creator data and opaque account, credential, or mailbox references; keep secrets in `local-vault`.
- **CREATOR-07** Require explicit confirmation and idempotency for external writes and record supported readback atomically with state and audit.
- **CREATOR-08** Keep repository work inside the transaction boundary selected by placement; never let HTTP, MCP, UI, or provider adapters open Creator transactions.
- **CREATOR-09** Route every persisted-shape or durable-version change through `onecrew-local-data-evolution` before implementation.

## Related Skills

- onecrew-capability — The Creator use case needs provider behavior.
- onecrew-peer-adapters — HTTP or MCP exposes the use case.
- onecrew-local-data-evolution — Schema, marker, fingerprint, or persisted encoding changes.
- onecrew-service-credentials — The workflow consumes a service credential or mailbox reference.

## Workflow

1. Define the user-visible use case, canonical inputs/results, product states, and transaction outcome.
2. Compose migrated provider behavior through the owning runtime's typed toolkit function; inspect current callers before changing a remaining capability port.
3. Implement repository behavior in the current schema or activate local-data evolution first.
4. Keep direct save input validation and fresh provider reads in `platform-runtime`; pass typed prepared data to Creator identity, contact and transaction policy. The result owner holds the immutable review payload. Confirmation executes it without refetching the provider or consulting a candidate cache.
5. Project the same runtime operation through requested peer adapters.
6. Test normalization, identity/collision behavior, transactions, write guards, repository round trips, and adapter consumers.

## When NOT to use

- Do not use it for provider protocol changes with no Creator workflow impact.
- Do not use it for operator login, proxy, browser, vault, or native distribution work.
- Do not use it for presentation-only UI changes with unchanged Creator contracts.
