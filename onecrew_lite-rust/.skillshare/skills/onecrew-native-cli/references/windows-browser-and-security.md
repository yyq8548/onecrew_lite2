# Windows Browser and Security

Windows-specific Chrome discovery, version probing, lifecycle, cookie
decryption, and secret-file protection. Owning code: `crates/chrome`, plus
the DACL helpers in `crates/local-vault/src/crypto.rs` and the MCP
access-token writer in `crates/local-service/src/mcp_access_token.rs`.

## Critical Rules

Capability probes must be side-effect free (NATIVE-04). Windows `chrome.exe`
is a GUI executable: invoking `--version` can open a browser window and block
the caller until Chrome exits. Read the VERSIONINFO resource instead.

```rust
// ❌ FORBIDDEN: launches the GUI browser just to learn its version.
let output = Command::new(chrome_exe).arg("--version").output()?;

// ✅ CORRECT (crates/chrome/src/detect.rs chrome_version, windows build):
// read the executable's VERSIONINFO resource; no process is started.
let size = unsafe { GetFileVersionInfoSizeW(binary.as_ptr(), &mut ignored) };
// ... GetFileVersionInfoW + VerQueryValueW, then VS_FIXEDFILEINFO major.
```

The ordinary `--version` probe stays `#[cfg(not(target_os = "windows"))]`,
requires a successful exit status, and parses only Chrome or Chromium
version text.

App-Bound (`v20`) cookie values fail closed with a typed error (NATIVE-05).
Never skip them silently, elevate, inject, impersonate Chrome paths, or
request a Windows password to reach them.

```rust
// ❌ FORBIDDEN: silently skipping rows hides that the capability is absent.
if raw.encrypted_value.starts_with(b"v20") { continue; }

// ✅ CORRECT (crates/chrome/src/cookies.rs decrypt_value, windows build):
if encrypted.starts_with(V20) {
    return Err(ChromeError::AppBoundCookieUnsupported);
}
```

A secret file whose security setup failed must not survive. Create it with
`create_new`, apply the protected DACL, and delete it on failure (NATIVE-06).

```rust
// ❌ FORBIDDEN: on DACL failure the key stays readable through inherited ACEs.
file.write_all(key)?;
let _ = restrict_permissions(path);

// ✅ CORRECT (crates/local-vault/src/crypto.rs write_private_key):
if let Err(error) = restrict_permissions(path) {
    let _ = fs::remove_file(path);
    return Err(error);
}
```

## Quick Reference

| Concern | Rule |
|---|---|
| Discovery order | `CHROME_BIN` override → registered App Paths (HKCU/HKLM, both registry views) → `%LOCALAPPDATA%` per-user install → both Program Files roots; no drive letter assumed |
| Discovery output | `DetectedChrome` carries channel and install scope as mechanics metadata; `platform-runtime` decides the capability state |
| Version probe | Windows: VERSIONINFO APIs only; macOS/Linux: `--version` with a successful exit status |
| Launch | Only an app-owned isolated user-data directory, only after explicit user action |
| Close | `WM_CLOSE` to every window of the launched pid, bounded 15 s wait, forced kill-and-reap on any failure; an already-exited child counts as closed |
| Cookie formats | Only the DPAPI-wrapped `v10` AES-256-GCM layout from the app-owned profile's `Local State`; `v20` → `ChromeError::AppBoundCookieUnsupported` |
| Cookie integrity | Cookie-schema v24+ prepends SHA256(host_key); verify it before accepting a value, as Chromium does |
| State placement | Databases, profiles, keys, tokens, and bounded logs under App Local Data — never Roaming App Data or installation resources |
| Secret DACL | `D:P(A;;FA;;;OW)(A;;FA;;;SY)(A;;FA;;;BA)` — protected; owner, System, and Administrators only |
| User's Chrome | Never inspect, copy, mutate, or impersonate the user's ordinary profile (NATIVE-03) |

## Common Patterns

### Detection is passive; capability decisions live upstream

`detect_chrome_install()` only checks `path.is_file()` and reads resources.
It reports install channel and scope; whether that means "browser login
available" is `platform-runtime`'s decision, not the chrome crate's.

### Graceful close protects the cookie flush

`request_close` (crates/chrome/src/launch.rs) posts `WM_CLOSE` via
`EnumWindows` and returns a typed error when no top-level window matched the
launched pid. `wait_for_close` bounds the wait with `WaitForSingleObject`
(15 s); on timeout the cookie flush is unproven, so extraction must not
proceed. `force_cleanup` always kills and reaps, so no child process or
profile lock survives an error path.

### DPAPI unwrap with cleaned-up native allocations

`windows_profile_key` reads `os_crypt.encrypted_key` from the app-owned
profile's `Local State`, requires the `DPAPI` prefix, and rejects any
unwrapped key that is not exactly 32 bytes. `dpapi_unprotect` wraps the
DPAPI-allocated output in a `LocalBuffer` drop guard so the native buffer is
freed on every path.

### The same DACL shape in every secret writer

`crates/local-vault/src/crypto.rs` and the MCP access-token writer in
`crates/local-service/src/mcp_access_token.rs` share the same SDDL and the same
create-new / protect / delete-on-failure sequence; `validate_key_file`
additionally rejects non-regular files and wrong key lengths on open. New
secret files copy this shape.

### macOS counterpart: passwords only through the native Keychain dialog

On macOS the Chrome Safe Storage password is read with
`security_framework::passwords::get_generic_password`, which makes macOS show
its native Keychain authorization dialog. Per the repository constitution,
a macOS login password is never requested or passed through chat, tool
arguments, or app fields — the OS dialog is the only acceptable prompt.

## Checklist

- [ ] No probe launches a GUI executable or touches the user's default profile.
- [ ] Discovery covers override, registry App Paths, per-user, and both Program Files roots.
- [ ] Every close path ends in kill-and-reap; no child or profile lock leaks.
- [ ] `v20` and unknown formats return typed errors on Windows; nothing is silently skipped.
- [ ] Mutable Windows state lands under App Local Data.
- [ ] Every secret file gets create-new, a protected DACL, and delete-on-failure.
- [ ] No elevation, injection, or password prompt outside the OS-native dialog.
