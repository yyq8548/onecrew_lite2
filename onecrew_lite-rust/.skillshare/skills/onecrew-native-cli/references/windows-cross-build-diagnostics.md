# Windows Cross-Build Diagnostics

Target x86_64-pc-windows-msvc produces raw onecrew.exe with embedded assets.
A cross-build is compile/link evidence only; no installer or signing path exists.
Do not install targets, toolchains, SDKs, or cross-build helpers without scope.

Classify linker errors before changing flags: missing target libraries, SDK or
import libraries, incorrect architecture, undefined symbols, and duplicate symbols
have distinct owners. Inspect the exact command and target directory. Never disable
warnings globally or broaden target-scoped suppression without evidence.

Use one Cargo process per target directory and the low-disk environment:
CARGO_INCREMENTAL=0, CARGO_PROFILE_DEV_DEBUG=0, CARGO_PROFILE_TEST_DEBUG=0.
Preserve existing build outputs; cleanup requires explicit scope.

Record target triple, toolchain/SDK, exact argv, binary SHA256/size/version manifest,
and unresolved diagnostics. Native Windows acceptance separately covers startup,
authenticated HTTP/MCP, private data paths and ACLs, browser launch, explicit plugin
installation/readback, shutdown, and data continuity. If Windows rejects the actual
binary, report that rejection without disabling OS defenses. Compilation alone
never establishes successful native launch or distribution acceptance.
