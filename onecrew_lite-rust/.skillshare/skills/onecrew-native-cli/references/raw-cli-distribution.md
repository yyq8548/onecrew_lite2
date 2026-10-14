# Raw CLI Distribution

Build `apps/web` and `apps/mcp-ui`, then `cargo build -p onecrew --release`
with an explicit target when cross-building. `crates/onecrew/build.rs` embeds
web and plugin assets. One target directory admits one Cargo process at a time;
use CARGO_INCREMENTAL=0, CARGO_PROFILE_DEV_DEBUG=0, CARGO_PROFILE_TEST_DEBUG=0.

The raw artifact is `onecrew` or `onecrew.exe`. Targets are aarch64-apple-darwin,
x86_64-apple-darwin, and x86_64-pc-windows-msvc. Plugin packaging stages a full
immutable generation including the executable, marketplace index, and plugin
assets. Business data remains under io.onecrewai.onecrew, outside plugin caches.

The distribution setup Skill under integrations/onecrew-distribution/onecrew-setup
requires an explicitly verified manifest URL. Manifest schema_version is 1,
product is onecrew-cli, version is semver, and artifacts maps each target to
url, sha256, size. Only HTTPS onecrewai.io same-origin raw artifacts are allowed:
no redirects, query, or custom ports. Never invent a published endpoint.

Tauri overlays, WebView2, DMG layouts, and NSIS are retired. No signing, notarization, or installer build workflow remains. Observe actual
downloaded-binary execution on native hosts. Preserve OS defenses and report
actual rejection honestly; cross-builds do not prove native runtime acceptance. See root SIGNING.md
and WINDOWS.md. Packaging, signing, publishing, and installation remain explicit
effects within user authorization.
