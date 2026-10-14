---
name: onecrew-native-cli
description: Handle OS-specific process, path, permission, browser, and distribution behavior. Excludes React-only layout and platform-independent runtime policy.
---

# OneCrew Native CLI

## Goal

Preserve one shared application contract while isolating fail-closed native mechanics and proving each target at the correct evidence level.

## Hard Constraints

- **NATIVE-01** Keep application capabilities platform-neutral and isolate native mechanics behind their owning Rust crate.
- **NATIVE-02** Limit OS-name branching in React to presentation; never infer runtime capability from the OS name.
- **NATIVE-03** Keep browser profiles app-owned and never inspect, copy, mutate, or impersonate the user's ordinary Chrome profile.
- **NATIVE-04** Make capability probes side-effect free; never launch a GUI executable to discover its version.
- **NATIVE-05** Fail closed for unsupported credentials, permissions, or native prerequisites.
- **NATIVE-06** Keep mutable Windows state in App Local Data and protect secret files with a verified current-user boundary.
- **NATIVE-07** Ship a target-specific raw CLI executable with embedded web/plugin assets; no WebView2, HTTP-shell overlay, DMG, or installer is a runtime prerequisite.
- **NATIVE-08** Treat cross-built artifacts as candidates only; require native-host runtime evidence for release acceptance.
- **NATIVE-09** Classify linker diagnostics before any target-scoped suppression and never disable warnings globally.
- **NATIVE-10** Keep binary packaging, publication, installation, overwrite, and application launch within explicit authorization. No signing/notarization/installer pipeline exists.

## Related Skills

- onecrew-architecture-placement — Native work changes ownership or public contracts.
- onecrew-web-ui — Native presentation or prerequisites appear in React.
- onecrew-local-data-evolution — Native paths, permissions, or lifecycle affect an upgrade.
- onecrew-rust-quality — Pair for generic unsafe proof and safe-wrapper
  quality while native status, buffer, permission, and cleanup mechanics stay here.
- onecrew-verify — Report host/target acceptance and blocked native coverage.

## Router

- Ownership: [Platform Boundaries](references/platform-boundaries.md)
- Raw binary artifacts and manifest: [Raw CLI Distribution](references/raw-cli-distribution.md)
- Windows Chrome or secret paths: [Windows Browser and Security](references/windows-browser-and-security.md)
- cargo-xwin/MSVC diagnostics: [Windows Cross-Build Diagnostics](references/windows-cross-build-diagnostics.md)
- Acceptance levels: [Verification Matrix](references/verification-matrix.md)

## Workflow

1. Record target OS/architecture, shared contract, native owner, excluded paths, and current proof level.
2. Implement the shared contract first and the smallest target-gated mechanics second.
3. Add host-neutral tests plus target-gated compile or unit evidence.
4. Run the applicable verification rows and label host, target, architecture, and evidence level.
5. Report native-host, SDK, or authorization gaps as blocked acceptance.

## When NOT to use

- Do not use it for platform-neutral Creator logic, persisted-data semantics, or provider protocol work.
- Do not use it for ordinary React composition with no native boundary.
- Do not use it as authority to produce or install a release artifact.
