# ADR 0005: Vendored X Chat cryptography

## Status

Accepted.

## Context

X Chat encryption and passcode recovery depend on the Chat XDK and Juicebox
implementation. Pulling changing source implicitly would make cryptographic
behavior vary between checkouts and make audits difficult.

## Decision

The Chat XDK and Juicebox SDK source trees used by `x-chat` are vendored under
`libs/` as standalone workspaces excluded from the root workspace; `x-chat`
consumes them through explicit path dependencies. `x-chat` is the application integration
boundary: it owns event normalization, key application order, encryption, and
decryption while delegating cryptographic primitives to the vendored source.

Upstream changes are imported deliberately and reviewed as source diffs.
Required licenses and notices ship with the application.

## Consequences

- Every checkout builds the same reviewed cryptographic implementation.
- Vendored-source changes receive security-sensitive review and targeted tests.
- CLI, shared Web, CRM, and MCP layers do not depend directly on SDK internals.
- Replacing the SDK or moving it out of tree requires an explicit supply-chain
  and compatibility decision.
