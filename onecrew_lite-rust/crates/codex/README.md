# onecrew_lite CLI plugin owner

## Windows MCP launch (2026-10-09)

The current plugin/server ID is `onecrew_lite`, in marketplace
`onecrew_lite-cli`. Do not add a second standalone MCP registration to repair
the bundled plugin. Historical owner/crate paths below do not rename the product.

Windows packages launch `cmd.exe /d /v:off /c .\\bin\\onecrew_lite-mcp.cmd`
with plugin-relative `cwd: "."`. The bundled launcher resolves its own native
`onecrew_lite.exe` and preserves stdio. A relative executable passed directly to
Rust `Command` on Windows does not resolve against `current_dir`; this failed
from an unrelated workspace even though an absolute-path SDK probe succeeded.
An explicit data root travels in a quoted environment expansion with delayed
expansion disabled, not interpolated shell source. The launcher is included in
the immutable package digest. macOS keeps its direct executable contract.

Repair through the built `onecrew_lite plugin install` owner, then fully restart
Codex. Verify fresh plugin readback **and** actual `onecrew_status`; an enabled
plugin, a running service, or a host `ready` event alone is insufficient. Never
delete the customer's data, copy credentials, or modify an installed generation.
The Windows native regression covers Rust process launch, stdin/stdout, and
Unicode, spaces, shell metacharacters, and explicit data-root preservation.

This crate packages the native OneCrew binary with the embedded router Skill and
stdio MCP configuration. It owns no business data, authentication secrets, task
execution, application bootstrap, or data migration.

The synchronous APIs are `package_plugin(executable, target, destination)`,
`install_plugin(plugin_root, executable, data_dir, codex_cli)`,
`read_cached_plugin_status(plugin_root)`,
`refresh_plugin_status(plugin_root, codex_cli)`, `codex_cli_candidates()`, and
`setup_text(executable, data_dir)`. An optional
absolute data directory scopes installer MCP arguments and setup instructions;
portable distribution packages omit it. Async callers must use a
blocking worker. Only explicit install and refresh call Codex. Default status
reads a local receipt and labels it `cached`; it does not establish current
installation state. `unavailable` and its diagnostic code represent failed
verification, not successful installation.

Default status also reads the `onecrew-cli` registration in
`CODEX_HOME/config.toml`, without starting or querying Codex. When that
registered directory or its `.agents/plugins/marketplace.json` is gone, status
reports `unavailable` with `codex_marketplace_source_missing`; when the
registration is not this plugin root's marketplace (for example a retired cache
location), it reports `codex_marketplace_source_relocated`. Both keep the
receipt's verification and last check, and
`PluginStatus::requires_marketplace_repair()` identifies them for consumers,
whose repair is `onecrew plugin install`. No registration, a healthy
registration, or a config status cannot read leaves the receipt unchanged;
explicit install still reports config errors.

`codex_cli` names the Codex CLI to drive and replaces discovery outright, so a
caller that has resolved the CLI itself never falls through to another one; the
CLI validates the path before calling. Without it, discovery searches this
process's own PATH, plus `ChatGPT.app` on macOS only. `codex_cli_candidates()`
returns that search in order so a caller can report what was tried when nothing
matched.

Packages support Apple arm64, Apple x86_64, and Windows x86_64 MSVC. Each immutable
generation contains exactly one target binary. A SHA256 digest covers the binary
and plugin files, including the manifest normalized to its base version to avoid
a circular hash. The installed manifest, binary target, and complete file digest
must match before an installation receipt reports success.

Generated marketplaces use the distinct `onecrew-cli` name. Codex keeps the
absolute source path it registered, so that path must survive cache cleanup:
the marketplace for a Codex home lives at
`<plugin_root>/<sha256(canonical CODEX_HOME)>`, with receipts and immutable
generations under its `onecrew` namespace. This crate has no internal
dependencies and derives no install paths. `onecrew-local-service`'s
`UserInstallLayout` owns the fixed per-user install root, and the CLI and
service pass its `codex_plugin()` directory as `plugin_root`:
`~/.local/share/onecrew/codex-plugin` on macOS and Linux,
`%LOCALAPPDATA%\OneCrew\codex-plugin` on Windows. The earlier OS cache location,
`io.onecrewai.onecrew/cli-plugin`, is neither read nor deleted; install replaces
a registration that still points there. A filesystem generation
lease spans staging, publishing, Codex commands, readback, and receipt writing.
Complete previous generations remain available; interrupted unpublished trees
can be safely replaced on the next explicit packaging run. Existing business data
and the older `reacher-local` marketplace are outside this owner's write scope.

Packaging unit tests use synthetic headers only. They do not establish native
startup, Codex `${PLUGIN_ROOT}` interpolation, signing, or notarization evidence;
those require real target artifacts and explicit integration acceptance.
