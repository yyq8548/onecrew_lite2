# ADR 0003: Separate business and secret storage

## Status

Accepted.

## Context

CRM records need durable querying and attribution. Browser sessions, social
account credentials, and provider API keys have a different confidentiality
and lifecycle model. Storing them in one schema would spread secret handling
through the business layer and complicate recovery.

## Decision

`crm-runtime` owns business records in its SQLite store. `local-vault` owns
encrypted operator profiles and service credentials in a separate SQLite store
and key file.

CRM records may retain public projections and opaque references to vault-owned
accounts, credentials, and mailboxes. They do not store raw sessions, cookies,
passcodes, provider keys, or the MCP bearer.

## Consequences

- Business queries do not require decrypting credentials.
- Vault lifecycle and access-control policy can fail closed independently.
- Cross-store references are reconciled by `platform-runtime`, not foreign keys
  across database files.
- Backup, preflight, and upgrade logic must classify every owned store before
  opening the runtime for mutation.
