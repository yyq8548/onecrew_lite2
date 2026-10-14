---
name: onecrew-local-dev-release
description: Build, package, replace, and verify a local OneCrew CLI development release. Use when local source changes must become a dist artifact, replace the local onecrew binary, or refresh the bundled Codex plugin.
---

# OneCrew Local Development Release

## Goal

Turn the current checkout into a verified local OneCrew CLI and plugin installation while preserving the repository's distribution and restart boundaries.

## Hard Constraints

- Always build the final `onecrew` binary with `development/scripts/build.sh`; never treat a crate library or `target/release/onecrew` alone as the deliverable.
- Always use the script-generated `dist/onecrew/<version>/<target>/manifest.json` for version, size, and SHA-256 verification.
- Always replace `/Users/$USER/.local/bin/onecrew` through `__bootstrap-update`; never copy over the installed binary directly.
- Always run `onecrew plugin install` explicitly after replacing the CLI.
- Always report plugin installation as requiring a full Codex quit and restart; installation success does not prove the current task loaded the new plugin.
- Never echo pairing secrets, one-use URLs, credentials, or private authorization values.
- Never run this workflow for production publication, signing, notarization, or multi-target release upload unless separately authorized.

## Related Skills

- onecrew-native-cli — Target-specific native packaging or bootstrap mechanics change.
- onecrew-codex-plugin — Bundled plugin source or reload contract changes.
- onecrew-verify — An artifact, replacement, or post-restart status is claimed.

## Workflow

1. Confirm the checkout, target triple, and current worktree status. Preserve unrelated dirty changes.
2. Run from the repository root:

   ```bash
   bash development/scripts/build.sh --release --skip-ui
   ```

   Use `--install-deps` only when the user explicitly requests dependency installation. Use the normal UI build when web assets are not already present.

3. Replace the CLI with the checked-in script. It reads `dist/onecrew/<version>/<target>/manifest.json` and checks that the product, version and target match and that the artifact's SHA-256 and size equal the manifest's. It refuses a target other than the host. It then invokes the fresh binary's `__bootstrap-update` with the manifest's exact `version`, `sha256` and `size`, and afterwards checks the installed binary's `--version`, hash and size against the manifest:

   ```bash
   bash development/scripts/install-local-build.sh
   ```

   `--dry-run` prints the exact `__bootstrap-update` command after all checks pass, without running it. `--install-path` overrides `/Users/<user>/.local/bin/onecrew`. `--debug` selects the debug artifact.
4. Run `/Users/<user>/.local/bin/onecrew plugin install` and retain its typed status. Passing `--plugin-install` to the script runs it right after a verified replacement. Stop installation work when it returns a restart requirement.
5. After the user fully quits and restarts Codex in a new task, run `onecrew plugin status --refresh` and then a read-only `onecrew_status` through the newly loaded plugin. Do not infer live tool availability from the installer receipt.

## Output Template

Report the dist artifact path, target, CLI version, installed binary hash, plugin status, and the exact remaining restart or verification action. Distinguish build success, local replacement, plugin installation, and post-restart live verification.

## When NOT to use this skill

- Do not use it for ordinary Rust compilation without local installation.
- Do not use it for production release publication or remote deployment.
- Do not use it to install a plugin supplied by an unverified source.
