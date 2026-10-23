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

After installing a different build, run data upgrade only for existing data and with its original service stopped:

```sh
onecrew-lite data-upgrade --data-dir "/absolute/original/data-directory"
onecrew-lite start --data-dir "/absolute/original/data-directory" --vault "/absolute/original/vault.sqlite" --master-key "/absolute/original/master-key"
onecrew-lite status --data-dir "/absolute/original/data-directory"
```

These are path placeholders: use the verified absolute original paths. Require upgrade state `upgraded` or `current`; preserve its database backup. Keep the original port with `--legacy-port <port>` when applicable. A confirmed first installation skips data upgrade and lets the program initialize its own data, vault and key. The application does not support `--help` or `--version`; verify its version and executable hash using `status`, `candidate.json` beside the native program, and live MCP.

## Connect your AI client

For Codex:

```sh
onecrew-lite codex-package --data-dir "/absolute/original/data-directory"
onecrew-lite connect-codex --data-dir "/absolute/original/data-directory" --refresh
onecrew-lite codex-status --data-dir "/absolute/original/data-directory" --refresh
```

Verify the complete generated `__VERSION__+codex.<digest>` plugin version, enabled state and fresh verification separately from the service. Complete any required client reload or restart. Other MCP clients use the absolute native program returned by `--print-binary-path`, with `mcp --stdio --data-dir` and the original directory. Start the service first.

Complete sign-in and browser authorization yourself. Check a live `onecrew_status` account call, browser connectivity and Library readiness. Use `onecrew_web` and available UI tools to open and read the status card, workbench and existing result table or Library. Installation does not start or resume searches.

## AI installation prompt

Install or update OneCrew Lite __VERSION__ using the npm command above. I authorize the necessary local installation, verification, preservation and upgrade of existing data, service start/stop, companion plugin or MCP configuration, and localhost/browser/Library checks. Perform the terminal operations yourself. Preserve other applications, client settings, login and search results. Use the client's supported approval flow for required sandbox-blocked operations, request only the affected command and paths, and continue from the checkpoint after approval. This prompt itself does not grant system permissions. I complete sign-in, browser authorization, required security consent and client restart. On macOS this is an ad-hoc signed, unnotarized build; verify the program before execution and request specific consent if its download quarantine requires action. Keep system protections unchanged.

When I say “一键修复”, call `onecrew_repair` when available, including while the main service is stopped. Reuse its request ID and selected client. If MCP is unavailable, use the verified native program's `repair --data-dir <original-directory> --client codex` or `--client mcp`. Repair observed faults, verify the running service and live client connection, and keep searches stopped.

Product information: https://onecrewai.io
