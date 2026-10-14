# OneCrew Access installer templates

## Windows desktop setup (OpenCLI onboarding)

`onecrew-windows.nsi` packages the existing Windows x64 release executable as
a per-user desktop setup. It creates Start menu and desktop shortcuts that open
the local OneCrew webpage, where **Connect OpenCLI** links to the official
OpenCLIApp download and Chrome Web Store, checks the local connection, and lets
the user choose a connected browser identity. OpenCLI is an independent
dependency and is not bundled. Uninstall removes the OneCrew executable and
shortcuts, not business data.

On the build machine, build the signed `dist/onecrew/<version>/x86_64-pc-windows-msvc/onecrew.exe`
and its matching manifest first, then run `build-windows-installer.ps1` with a
code-signing certificate thumbprint and an explicit Windows SDK `signtool.exe`
path. The script checks the binary's manifest, version and Authenticode
signature, builds with NSIS, signs the setup, and verifies its signature. NSIS
uses zlib compression. `-UnsignedDevelopment` permits a local test candidate
only; it is not a customer release.

OneCrew Access accepts a Windows x64 `installer` asset. Publish the signed
`OneCrew-Setup-<version>-windows-x64.exe` to the existing private R2 release
object path and then publish matching immutable Access release metadata. The
download page already renders installer assets; this setup does not change the
separate CLI update contract. A customer release still requires a real signed
binary and setup, a published/downloaded artifact, and clean-machine
install/connect/uninstall acceptance. A successful NSIS syntax build or web
unit test is not that acceptance.

## Legacy CLI script templates

`install.sh.txt` installs the native macOS arm64 or x86_64 executable, including
arm64 selection when invoked under Rosetta. `install.ps1.txt` installs Windows
x64 using native PowerShell; ARM and 32-bit operating systems are rejected.
These are **server-rendered templates**, not directly executable downloads.

Access copies these sources to `src/cli-distribution/install.sh.txt` and
`src/cli-distribution/install.ps1.txt`, validates published release metadata,
then substitutes the following literal placeholders:

| Placeholder | Contract |
| --- | --- |
| `__ONECREW_VERSION__` | Exact release version, taken from the published release rather than this table; strict numeric semver |
| `__ONECREW_MAC_ARM_URL__` | Access HTTPS download gateway with short-lived scoped ticket, arm64 |
| `__ONECREW_MAC_INTEL_URL__` | Same gateway, macOS x86_64 |
| `__ONECREW_WINDOWS_URL__` | Same gateway, Windows x64 |
| `__ONECREW_MAC_ARM_SHA256__`, `__ONECREW_MAC_INTEL_SHA256__`, `__ONECREW_WINDOWS_SHA256__` | Exactly 64 lowercase hexadecimal characters, from the corresponding published binary |
| `__ONECREW_MAC_ARM_SIZE__`, `__ONECREW_MAC_INTEL_SIZE__`, `__ONECREW_WINDOWS_SIZE__` | Positive decimal byte count of the raw executable |

All values occur inside single-quoted string literals. The renderer must validate
the gateway origin/path and every metadata field; URLs must be ASCII HTTPS,
percent-encoded as needed, with no credentials, fragment, literal apostrophe,
backslash, whitespace, control characters, or line breaks. Do not use raw user
input or leave unresolved placeholders. The gateway must return the raw binary
directly with HTTP 200; redirects are unsupported. Never log rendered scripts or
ticket URLs. Each template needs only its platform's placeholders plus VERSION.

Render the selected template, save privately, and run macOS with `/bin/sh` or
Windows with PowerShell. Installation verifies byte count, SHA-256, native
executable header, and exact `onecrew <version>` output before an atomic replace.
Staging lives beside the installed executable and is removed on completion or
failure. Existing binaries survive failed verification/download/replacement.

Locations are `$HOME/.local/bin/onecrew` and
`$env:LOCALAPPDATA\OneCrew\bin\onecrew.exe`. The CLI later keeps regenerable
integration state in the matching per-user root, which these templates never
create: the Codex marketplace from `onecrew plugin install` in `codex-plugin`
and the local service's running executable copies in `service-binaries`, under
`$HOME/.local/share/onecrew` on macOS and `$env:LOCALAPPDATA\OneCrew` on
Windows. Releases before this layout used `io.onecrewai.onecrew/cli-plugin` and
`io.onecrewai.onecrew/service-binaries` in the OS cache directory, which macOS
can purge while Codex still references the marketplace. Those directories are no
longer used and are not removed; run `onecrew plugin install` once to register
the new marketplace. No shell profile or PATH is modified;
the short output supplies an absolute, quoted login command. No login, plugin
installation, service start, or process termination runs automatically. A locked
Windows executable fails safely; subsequent CLI version-conflict handling remains
responsible for service lifecycle decisions.

## Unsigned Windows customer trial

Explicitly authorized unsigned trials use `build-windows-test-installer.mjs`; this
does not bypass or change the signed production builder. Build the current raw
CLI with `development/scripts/build.sh`, build the Library SEA on Windows, then:

```powershell
node development/scripts/package-workspace.mjs --cli-dir dist/onecrew/0.6.30/x86_64-pc-windows-gnu --library-dir ../onecrew_lite-library/release/win32-x64 --output C:/build/OneCrew-workspace
node development/installers/build-windows-test-installer.mjs C:/build/OneCrew-workspace C:/build/OneCrew-installer
```

The builder verifies every workspace manifest entry, includes the companion and
license notices, and emits an explicitly unsigned NSIS installer and SHA-256.
It executes both binaries with only Windows system directories in PATH before
packaging. GNU builds statically link the C++ runtime and explicitly select GCC
for BoringSSL through `development/scripts/windows-gnu.cmake`.
It installs at the existing standard `%LOCALAPPDATA%/OneCrew/bin` path, creates
test-labelled shortcuts and an uninstall entry, and refuses to overwrite an
existing CLI. Uninstall removes only the manifest-listed installation files;
business profiles and unrelated files are retained. A running Library must be
closed before uninstall (reboot, then uninstall before reopening OneCrew).
Codex plugin connection is a separate explicit Start Menu action, followed by a
full Codex restart. Login, Chrome/OpenCLI setup and customer search acceptance
are still required. This is not a signed production release.

Native acceptance (creates and removes trial-labelled shortcuts/registration;
refuses an existing trial registration, uses isolated data profiles):

```powershell
node development/installers/test-windows-test-installer.mjs C:/build/OneCrew-installer/OneCrew-Test-Setup-0.6.30-windows-x64-unsigned.exe
```

Offline validation:

```sh
/bin/sh -n development/installers/install.sh.txt
uv run --no-project development/installers/test_installers.py
```

Tests use temporary HOME, byte fixtures, and mocked host detection, download,
file-size and Mach-O-header commands. Hashing, executable version checking,
installation, command quoting (including spaces/apostrophes), old-file preservation,
and staging cleanup run against isolated files. They do not validate execution of
a real Mach-O binary. PowerShell contract checks are static assertions, not parsing
or execution. `pwsh` is unavailable on the implementation host: Windows native
PowerShell parsing, successful installation, locked-file preservation, and ARM
rejection still require real Windows acceptance. No real network or user
installation is exercised by this test suite.
