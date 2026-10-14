---
name: onecrew-service-credentials
description: Handle service API keys and mailbox credentials through vault-backed, secret-free references. Excludes human login sessions and provider operation mechanics.
---

# OneCrew Service Credentials

## Goal

Manage non-human service credentials through one local vault-backed lifecycle without exposing secret material.

## Hard Constraints

- **CREDENTIAL-01** Accept raw credentials only through an explicit secure local input boundary.
- **CREDENTIAL-02** Allow raw values only in bounded controlled-input state, the dedicated write-only connect/rotate HTTP commands, and deserialize-only MCP connect/rotate tool inputs over the authenticated loopback boundary (ADR 0006); clear them after invocation, redact every error path, and never cache, log, audit, persist, or project them.
- **CREDENTIAL-03** Store durable keys and tokens only in `local-vault` and expose opaque credential or mailbox references.
- **CREDENTIAL-04** Validate a credential with the narrowest read-only service operation before marking it usable.
- **CREDENTIAL-05** Project only provider kind, safe reference, status, capabilities, timestamps, and sanitized errors.
- **CREDENTIAL-06** Keep service validation and rotation semantics in `platform-runtime`, not UI or adapters.
- **CREDENTIAL-07** Make replacement and disconnect explicit and clean up superseded vault material after authoritative state changes.
- **CREDENTIAL-08** Never serialize a credential into MCP, HTTP outputs or read DTOs, database rows outside the vault, clipboard guidance, or reusable frontend state.
- **CREDENTIAL-09** Fail closed on ambiguous credential kind, validation, ownership, or deletion outcome.

## Related Skills

- onecrew-web-ui — Secure capture or status UI changes.
- onecrew-peer-adapters — Safe reference or status projections change.
- onecrew-creator — The Creator owner consumes a mailbox or credential reference.
- onecrew-operator-sessions — Authentication belongs to a human provider account or session.

## Workflow

1. Classify the provider credential, owner, validation operation, and current safe reference.
2. Capture through the matching boundary — bounded controlled-input state feeding the dedicated write-only connect or rotate HTTP command, or the deserialize-only MCP connect/rotate tool input — and clear the transient value immediately after invocation.
3. Validate read-only, persist only the vault reference and safe capability state, then clear transient input.
4. For replacement/disconnect, update authoritative references and clean superseded vault material with explicit outcomes.
5. Test redaction, validation failure, replacement, disconnect, and safe projections.

## When NOT to use

- Do not use it for browser, QR, OTP, 2FA, or other human account sessions.
- Do not use it for sending email, querying YouTube, or other business/provider operations after connection.
- Do not use it to reveal, export, or copy raw credentials.
