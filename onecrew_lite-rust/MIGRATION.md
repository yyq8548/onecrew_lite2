# Historical migration baseline

This file records the original migration scope. It is not the current runtime
surface contract; see [README.md](./README.md) and the canonical DTOs/use cases
in `crates/creator-runtime`. In particular, the current application includes the
Rust loopback MCP adapter and exposes the same canonical Creator workflows
through MCP and Tauri.

The Keychain items below are obsolete historical planning statements, not the
current storage model. The current vault uses a random `vault.key` beside
`agent-platform.db`; creation makes a best-effort attempt to set the expected
`0600`, while existing-file reads do not revalidate permissions. macOS Keychain
access is limited to reading Chrome Safe Storage during Chromium cookie import.
This historical document defines no compatibility, fallback, or data-migration
behavior.

## Difficulty

| Area | Difficulty | Reason |
|---|---:|---|
| Desktop shell and local Profiles UI | Low | Existing Tauri/shadcn patterns apply |
| Browser Profile import | Low | macOS Chromium importer is already validated |
| Encrypted local SQLite vault | Medium | Historical plan: schema, migrations, and Keychain lifecycle (obsolete; current implementation uses sibling `vault.key`) |
| X read/write operations | Medium | Existing client code can be adapted directly |
| Instagram read/write operations | Medium | Existing backend classes can be adapted |
| X encrypted chat | High | Passcode, key material, and Chat SDK lifecycle |
| Self-contained macOS packaging | High | Native dependencies must be signed and notarized |
| Full Rust protocol rewrite | Very high | Reimplements two unstable private APIs |

## Delivery slices

### 1. Local foundation

- Tauri shell.
- Chromium import.
- Keychain master key (obsolete historical plan; the current vault uses sibling
  `vault.key`, attempts `0600` on creation, and does not revalidate existing
  permissions).
- SQLite Profile vault.
- Profile list/delete/select.

### 2. Read operations

- X whoami, search, user, tweet, replies, inbox, thread.
- Instagram profile, search, posts, inbox, thread.

### 3. Write operations

- X post, reply, follow, unfollow, DM.
- Instagram follow, unfollow, comments, likes, DM.

### 4. Distribution

- Link native platform crates into the desktop application.
- Sign, notarize, and test on a clean macOS account.

## Explicit removals

- Python FastMCP and remote MCP deployment (replaced by the local Rust MCP
  adapter).
- OAuth/JWT access keys.
- Enrollment RSA keys and envelopes.
- Cloudflare Tunnel configuration.
- Remote Profile database.
- Remote profile IDs.
