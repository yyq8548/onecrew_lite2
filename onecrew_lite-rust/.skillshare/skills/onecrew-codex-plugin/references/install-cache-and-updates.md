# Install Cache and Updates

The thin CLI owns explicit integration commands; the minimal codex crate isolates
Codex discovery, packaging, installation, cached verification, refresh, and setup.
No service startup, business/MCP call, or ordinary status may launch/query Codex.
No app-server supervisor or turn-control interface belongs in this integration.

`onecrew plugin install` stages an embedded executable and secret-free assets,
invokes the explicit Codex installer, and reads back installed version/path and
marketplace registration. Exit status alone is insufficient. Pin the complete
immutable generation through installation and readback; cancellation must not
release ownership while a filesystem/CLI worker continues.

`plugin status` returns only cached last verification. `plugin status --refresh`
explicitly queries Codex. `setup` and `setup --copy` produce configuration without
installation. Install/setup honor explicit --data-dir; normal MCP uses the same
service/data root and never recursively launches Codex.

`plugin package --target <triple> --output <dir> [--binary <path>]` emits a complete
immutable generation with `.agents/plugins/marketplace.json` and
`plugins/onecrew/bin/onecrew` (or .exe). The generated and the repository
marketplace name are both `onecrew-cli`; the plugin is `onecrew`. Version is
<base>+codex.<SHA256>, derived from all packaged bytes with a documented stable
hash protocol. Do not append suffixes or change numeric versions as cachebusters.
Business data stays in io.onecrewai.onecrew, outside all plugin cache generations.
Codex keeps the marketplace source path it registered, so generated marketplaces,
receipts, and staged service binaries live in the fixed per-user install root
that `onecrew-local-service` owns (`~/.local/share/onecrew` on macOS/Linux,
`%LOCALAPPDATA%\OneCrew` on Windows), never an OS cache directory the host may
purge. Cached status reads that registration from `CODEX_HOME/config.toml`, not
from Codex, and reports a missing or relocated source; `plugin install` repairs it.

The plugin's stdio connection lists the five bootstrap tools (status, login,
login cancel, logout, web) ahead of the business surface's five model-visible
router tools (connection list, search, get, execute, present), so the Router
validator allowlist names ten meta tools; the App-only result-state tool is
listed to the host with App visibility only and is never a Router verb.

A cached router and a live MCP surface are distinct. Validate cached guidance in
a new Codex session, and record the tested Codex version. A successful installation
does not refresh an already running session. For a running binary mismatch,
finish current work and explicitly stop before restarting; do not claim automatic
hot replacement. Keep router guidance short and retrieve live contracts.
