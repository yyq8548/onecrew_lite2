# OneCrew Lite

OneCrew Lite provides a local creator research workbench, saved evidence, live result tables and an MCP connection for your AI client. Planning and scoring use the current conversation's model. No separate model API key or persistent App Server is required.

## Install

Use Node.js 20 or newer with npm. The application includes its own runtime. This release supports Apple Silicon macOS and native Windows x64; use matching ARM64 or x64 Node.js.

```sh
npm install --global @onecrew/lite@__VERSION__ --registry=https://registry.npmjs.org/ --include=optional --ignore-scripts
onecrew-lite --print-binary-path
```

The exact matching platform package contains the program and third-party licenses. Installation has no lifecycle scripts or extra binary downloads. Keep optional dependencies enabled.

## Preserve your installation

Before updating, identify and stop the service for the original data directory. For an existing ZIP installation, stop its verified installed program first. Preserve the data directory, vault, master key, port, login, browser identity, saved scores and search results. Do not create empty data to bypass an upgrade error. On Windows, `onecrew-lite installation-discover` identifies existing installations. On macOS use the original service or companion plugin metadata.

For an existing scoped account-* root, run data upgrade with its original service stopped. This command upgrades Creator SQLite only, so its `current` result does not migrate a standalone Library:

```sh
onecrew-lite data-upgrade --data-dir "/absolute/original/data-directory"
```

On macOS, after stopping the original standalone Library, migrate its complete verified data into a scoped root with:

```sh
onecrew-lite library-migrate --data-dir "/absolute/original/data-directory" --library-data-dir "/absolute/old-library" --vault "/absolute/original/vault.sqlite" --master-key "/absolute/original/master-key"
```

For an old Mac Rust installation with root crm.db, agent-platform.db and vault.key, use the full migration into a separate private unified destination outside both source directories:

```sh
onecrew-lite library-migrate --data-dir "/absolute/unified-destination" --legacy-data-dir "/absolute/old-service" --creator-db "/absolute/old-service/crm.db" --library-data-dir "/absolute/old-library" --vault "/absolute/old-service/agent-platform.db" --master-key "/absolute/old-service/vault.key" --legacy-port ORIGINAL_WEB_PORT
```

Use verified original paths and port in place of these placeholders. Require migration state `migrated` or `current`, `source_preserved=true` and `search_resumed=false`. Use returned `data_directory`, `vault_path` and `master_key_path` for all subsequent start and client commands. Preserve recovery_directory and the original installation. Migration validates original account/tenant/base ownership, every original Library table and row, and the original online installation identity. Do not copy PGlite manually, create empty tables or fabricate current.json. Already unified data does not need this migration; do not run root-level data-upgrade against an activated current.json generation. Windows stays at 0.9.1; this Mac migration command does not apply there.

Start the verified program using the effective paths and preserve the original port with `--legacy-port`. Require start and status to return `running`. A confirmed first installation skips migration and data upgrade. The application does not support `--help` or `--version`; verify its native version and executable hash using `status`, `candidate.json` beside the program, and live MCP.

## Connect your AI client

For Codex:

```sh
onecrew-lite codex-package --data-dir "/absolute/original/data-directory"
onecrew-lite connect-codex --data-dir "/absolute/original/data-directory" --refresh
onecrew-lite codex-status --data-dir "/absolute/original/data-directory" --refresh
```

Verify the complete generated `<native-version>+codex.<digest>` plugin version, enabled state and fresh verification separately from the service. Main package __VERSION__ selects Mac __VERSION__ and the unchanged Windows 0.9.1 platform package. Complete any required client reload or restart. Other MCP clients use the absolute native program returned by `--print-binary-path`, with `mcp --stdio --data-dir` and the effective data directory. Start the service first.

Complete sign-in and browser authorization yourself. Check a live `onecrew_status` account call, browser connectivity and Library readiness. Use `onecrew_web` and available UI tools to open and read the status card, workbench and existing result table or Library. Installation does not start or resume searches.

## AI installation prompt

Install or update OneCrew Lite __VERSION__ using the npm command above. I authorize the necessary local installation, verification, preservation and upgrade of existing data, service start/stop, companion plugin or MCP configuration, and localhost/browser/Library checks. Perform the terminal operations yourself. Preserve other applications, client settings, login and search results. Use the client's supported approval flow for required sandbox-blocked operations, request only the affected command and paths, and continue from the checkpoint after approval. This prompt itself does not grant system permissions. I complete sign-in, browser authorization, required security consent and client restart. On macOS this is an ad-hoc signed, unnotarized build; verify the program before execution and request specific consent if its download quarantine requires action. Keep system protections unchanged.

When I say “一键修复”, call `onecrew_repair` when available, including while the main service is stopped. Reuse its request ID and selected client. If MCP is unavailable, use the verified native program's `repair --data-dir <original-directory> --client codex` or `--client mcp`. Repair observed faults, verify the running service and live client connection, and keep searches stopped.

Product information: https://onecrewai.io
