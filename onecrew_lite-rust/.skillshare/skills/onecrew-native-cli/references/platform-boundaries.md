# Platform Boundaries

Ownership rules for cross-platform changes: which layer owns a native
mechanism, where `cfg` may appear, and what React is allowed to know about
the operating system.

## Critical Rules

Native mechanics live behind their owning Rust crate; the capability contract
above them stays platform-neutral. React branches on the OS name only for
presentation and never infers what the runtime can do (NATIVE-01, NATIVE-02).

```tsx
// ❌ FORBIDDEN: the frontend guesses a runtime capability from the OS name.
const canImportCookies = currentWebPlatform() === "macos";

// ✅ CORRECT: OS branching is presentation-only (apps/web/src/lib/titlebar.ts);
// capability state renders from the authoritative runtime projection instead.
export function titlebarOverlayHeight(os: WebPlatform) {
  return os === "macos" ? "2.25rem" : "0px";
}
```

Keep public DTOs independent of `cfg` unless the OS distinction is itself part
of the canonical contract. Gate only the implementation, and make the
missing-target fallback fail closed rather than silently succeed.

```rust
// ❌ FORBIDDEN: a cfg-shaped public contract forces every consumer to branch.
#[cfg(target_os = "windows")]
pub struct DetectedChrome { /* windows-only fields */ }

// ✅ CORRECT (crates/chrome/src/detect.rs): one platform-neutral DTO on every
// target; only the discovery internals are target-gated.
pub struct DetectedChrome {
    pub executable: PathBuf,
    pub version: Option<String>,
    pub channel: ChromeChannel,
    pub install_scope: ChromeInstallScope,
}
```

## Quick Reference

| Concern | Rule |
|---|---|
| Shared contract | `platform-runtime` owns canonical capability DTOs, state machines, stable errors, and the login workflow |
| Native mechanics | `crates/chrome`, `crates/local-vault`, and the desktop shell own discovery, process, cookie, filesystem-security, and path behavior |
| Peer adapters | HTTP and MCP project the same runtime service; neither calls or imitates the other |
| Presentation | React branches on OS only for the visual shell; capabilities render from runtime state |
| `cfg` placement | Gate modules and functions, not public DTOs, unless the OS distinction is itself canonical |
| `windows-sys` | Enable only the Win32 feature families the crate actually calls |
| Unsafe Win32 | Own the Win32 status, buffer, ownership, cleanup, and failure facts; pair Rust Quality for the generic proof and safe wrapper |
| Fallback targets | Existing `#[cfg(not(any(unix, target_os = "windows")))]` arms return silent `Ok` (e.g. `restrict_permissions` in `crates/local-vault/src/crypto.rs`); treat that as debt, and make any new arm fail closed with a typed error |

## Common Patterns

### One neutral entry point, target-gated internals

`crates/chrome/src/detect.rs` exposes `detect_chrome_install()` on every OS;
`windows_candidates()` and `registered_chrome_paths()` exist only under
`#[cfg(target_os = "windows")]`. Callers never see the branching, and the
`CHROME_BIN` override is honored before any OS-native discovery runs.

### Restricted windows-sys features

Each crate lists exactly the Win32 families it uses: `crates/chrome/Cargo.toml`
enables the foundation, DPAPI cryptography, file-system, registry, threading,
and windowing families, while
`crates/local-vault/Cargo.toml` enables only `Win32_Foundation`,
`Win32_Security`, and `Win32_Security_Authorization`. Adding an API means
adding its feature to that crate, not widening a shared list.

### Native facts feed the generic unsafe proof

`registered_chrome_paths()` wraps each `RegGetValueW` call in its own checked
sequence — size query, buffer allocation, status check, NUL scan — with a
`// SAFETY:` comment on the unsafe block. This Native reference owns those
Win32 call, buffer, status, and cleanup facts. Route the generic proof shape,
block scope, `SAFETY` contract, and safe-abstraction quality through
`onecrew-rust-quality` instead of duplicating that guidance here.

### Record the blast radius before editing

Record whether the change affects process lifecycle, durable paths, secret
permissions, browser capability, UI presentation, or packaging. A new OS
implementation must preserve unaffected macOS behavior and every
platform-neutral workflow (Telegram, YouTube, Email, Creator, MCP).

## Checklist

- [ ] The capability DTO and error surface compile identically on all targets.
- [ ] All `cfg` gating sits inside the owning crate, not in adapters or React.
- [ ] React OS branching affects only presentation (titlebar, shell chrome).
- [ ] `windows-sys` features are limited to APIs the crate calls.
- [ ] Every unsafe Win32 path records the API-specific pointer, buffer, status,
      ownership, and cleanup facts needed by the Rust Quality proof.
- [ ] Any new `#[cfg(not(...))]` fallback arm fails closed with a typed
      error; the existing silent-`Ok` arms are known debt, not precedent.
- [ ] Unaffected macOS behavior and platform-neutral workflows still hold.
