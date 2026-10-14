# onecrew_lite

onecrew_lite is one Rust CLI executable containing its React webpage and Codex plugin
assets. The local service owns the application runtime; browser HTTP and raw MCP
are authenticated peer adapters over the same Creator, provider, and vault owners.

```sh
onecrew_lite login --no-browser
onecrew_lite serve --no-browser
onecrew_lite status
onecrew_lite web
onecrew_lite mcp --stdio
onecrew_lite data status
onecrew_lite stop
```

`serve` starts or reuses the background service and displays status/next actions;
`serve --foreground` is a diagnostic host. `web` opens its browser surface.
`login --no-browser` displays the browser pairing action without opening it.
`logout` removes the local account authorization. `stop` drains work and preserves
data. A running binary mismatch requires finishing current work and stopping the
old service before restart; automatic hot replacement is not implemented.

Local-service startup checks ownership and older processes, inspects persisted
data, and automatically runs any supported required upgrade using preflight,
backup, adjacent upgrades, validation, and recovery. Compatible data starts
directly; changing the executable alone does not justify an upgrade. Business
operations require Ready. Inspecting, Upgrading, and Failed allow limited status
and MCP diagnostics; a failed upgrade blocks business even after partial commits.
`data status` is read-only. There is no manual upgrade command or HTTP/MCP route.

Business data remains under the stable `io.onecrewai.onecrew_lite` application data
directory, separate from plugin caches. Explicit `--data-dir` selects another
root. Historical directories are never implicitly copied or migrated at startup.

```sh
onecrew_lite plugin install
onecrew_lite plugin status
onecrew_lite plugin status --refresh
onecrew_lite setup --copy
onecrew_lite plugin package --target aarch64-apple-darwin --output ./plugin-output
```

Only explicit `plugin install` invokes the Codex installer and verifies readback.
`plugin status` returns cached verification; `--refresh` explicitly queries Codex.
`setup` prints configuration, while `setup --copy` copies it; neither installs.
Normal service, status, and MCP calls never probe or recursively launch Codex.
External creator/content searches require a user-confirmed temporary search plan by default.
Prepare `onecrew.search.plan.prepare` through the live MCP router, review its editable
description and keyword chips, then use the returned `confirmed_plan_ref` for
the approved searches and pagination. Approval is process-local and expires;
changing search scope requires confirmation again. A confirmed plan sets
criteria and parameters only: approved searches page freely with `next_cursor`
until the plan expires, and nothing is recorded under a plan.
When the user's current request explicitly asks to skip the plan or try searching
directly, set `search_plan_policy: "user_requested_direct"` on that search request.
Otherwise omit it (the default is `require_confirmation`); the agent must never
choose direct mode on its own. The runtime enforces the declared policy but cannot
verify natural-language intent. Direct mode only skips search-plan approval; input
validation, account/provider authorization, and all write approvals still apply.
Plugin installation/setup respect the selected data directory. Packaging emits
an immutable complete generation containing `.agents/plugins/marketplace.json`
and `plugins/onecrew_lite/bin/onecrew_lite` (or `.exe`), with version
`<base>+codex.<SHA256>`. Generated marketplace identity is `onecrew_lite-cli`; the source
repository marketplace remains `onecrew_lite-cli`.

For development, install Rust from the pinned toolchain and Node/npm, then run:

```sh
bash development/scripts/build.sh --all
node development/scripts/agent-check.mjs --profile commit
```

Build accepts `--target <Rust triple>`. Web and MCP UI are built before the CLI.
Keep Cargo commands strictly serial per target directory and use
`CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0`.

Distribution targets are `aarch64-apple-darwin`, `x86_64-apple-darwin`, and
`x86_64-pc-windows-msvc`. End users obtain the installation command from Access
and follow the installer and CLI's returned `next_action` / `next_actions`.
Installation and connection setup do not require a separate Skill. CLI updates
use `onecrew_lite update`; plugin installation remains an explicit, separate action.
See
[development/README.md](development/README.md) for builds and R2 uploads,
and [ARCHITECTURE.md](ARCHITECTURE.md) for runtime ownership.
