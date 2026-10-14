# Workspace download package

The complete Workspace download contains the embedded Rust workbench and the
self-contained Library executable. It adds no Node.js prerequisite for users.
It is separate from the raw CLI artifact; a raw CLI by itself does not include
Library and cannot open a working Library service on a new computer.

Build the normal Rust artifact with `development/scripts/build.sh`. Build a
matching Library release with `onecrew_lite-library/scripts/build-release.mjs`.
Then assemble the verified artifacts:

```sh
node development/scripts/package-workspace.mjs \
  --cli-dir dist/onecrew/<version>/<target> \
  --library-dir <matching-unpacked-library-release> \
  --output <new-workspace-directory>
```

The packager checks both input manifests, exact target pairing, size and SHA-256
of every input artifact. It rejects unsafe paths, symlinks and an existing output
directory. The output manifest covers both programs and the launcher. This is
local assembly; it does not publish or change Access download metadata.

After extracting, the user double-clicks `安装并打开 OneCrew.command` on macOS or
`打开 OneCrew.cmd` on Windows. The launcher invokes `onecrew workspace`, starts
or reuses the main service, and asks the companion's `web` command to start or
reuse Library and return its verified URL. Once both are ready, it opens the
workbench and `/library` in the system browser. Addresses follow actual service
ports. First use requires the existing sign-in flows. No table, Campaign,
provider read or approval is created by launching.

Each service owns its existing locks, authentication and data directory. The
launcher removes inherited Library profile/port overrides so the customer entry
uses the documented system profile. The main `--data-dir` flag still applies to
the main service only. `workspace --no-browser` returns the two URLs without
opening pages; MCP startup and ordinary `web` retain their existing behavior.
Keep the entire package together when moving it.

On macOS the entry first installs both executables into the current user's
fixed layout: `~/.local/bin/onecrew` and `~/.local/bin/library/onecrew`. The
workbench uses its existing verified bootstrap updater, including drain and
replacement. The companion's source and staged copy must match the manifest
SHA-256; an existing different companion is stopped through its own CLI before
atomic replacement. Identical binaries are retained. This fixed layout is also
where MCP resolves Library, so a portable launch alone is insufficient for a
new customer's MCP session. The optional `--no-browser` entry argument is for
native acceptance; double-clicking opens the two pages normally.

The recommended Mac first-use entry `首次安装 OneCrew.command` runs the
existing program installer, opens both pages, installs and verifies the pinned
OpenCLI runtime, then invokes the explicit Codex plugin installer. Any failure
returns a nonzero exit and retains its error;
it never announces that a chat card has appeared. Its `--no-browser` option is
for isolated native acceptance only. Customers must fully quit and reopen
Codex, then send “使用 OneCrew” in a chat. The bundled OneCrew skill requires
`onecrew_web` and both returned page URLs on every activation; the readonly
card is created by the Codex host, not by the shell installer. A missing Codex
installation or plugin-load failure cannot be reported as three-view readiness.
Explicit plugin installation removes only the retired identities
`onecrew-local@onecrew-cli` and `onecrew-local@reacher-local` through native
Codex CLI removal, verifies they are absent, then installs the current generation.
It never edits Codex configuration directly or removes unrelated plugins.

The separate repair entry `连接 Codex.command` installs the embedded plugin through the
installed workbench, then asks the customer to fully quit and restart Codex.
It does not log in, start searches or install OpenCLI. Failure leaves the error
visible instead of printing a success message. The package includes a UTF-8
`使用说明.txt` for customers who do not use Markdown. Windows installation
behavior is unchanged and is not covered by this Mac acceptance.

The workbench's installation repair button downloads its own pinned Node runtime
and compatible OpenCLI into the per-user OneCrew install directory. A valid
managed installation takes precedence over old commands on PATH. It never
uninstalls unrelated global packages or edits Chrome profiles. Bundled OneCrew
add-on conflicts are backed up outside the active command registry before
replacement; unrelated add-ons stay untouched. Installation readiness and
extension/profile connectivity are separate readbacks.

Run the native Mac customer lane after packaging:

```sh
python3 development/scripts/test-mac-workspace-install.py \
  /absolute/package-directory /absolute/development/node
```

It uses an empty child profile, excludes Node/Codex CLI from PATH, installs and
relaunches both programs, installs the plugin with native Codex discovery, and
tests the cached plugin binary's real MCP workspace/card protocol. The test
client needs Node; the customer package does not. The logged-out Library's
same-origin `/login?returnTo=%2Flibrary` redirect is expected. Other redirects
are rejected. Services are stopped by their owners before temporary profiles
are removed. This does not prove Codex host card rendering or a real search.

First use without results opens the Library list. Search result synchronization
and the workbench's result-table link remain the existing Campaign-owned flow.
Opening the list does not claim qualification, cross-platform deduplication or
a finished search. Startup timeout preserves unknown outcome and requires status
inspection; it does not blindly replay a start operation.

Mac ARM runtime evidence belongs in the dated acceptance log. Windows launcher
generation is not Windows native execution evidence. Customer publication still
needs the proper signed/downloaded artifact and platform acceptance; these are
separate from creating this local package.
