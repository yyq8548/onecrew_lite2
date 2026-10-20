# OneCrew Lite 0.9.0 — Mac installation prompt

Install or update OneCrew Lite 0.9.0 on my Apple Silicon Mac. Use this terminal command to read the complete Lite installation guide:

```sh
curl --fail --silent --show-error --proto '=https' --max-time 30 --write-out '\nHTTP_STATUS:%{http_code}\n' 'https://raw.githubusercontent.com/yyq8548/onecrew_lite2/refs/tags/mac-installer-0.9.0-r3/mac_installer/INSTALL-CODEX.md'
```

Require HTTP 200. Follow the guide's Mac installation and client-setup steps in order. Read the matching GitHub release manifest; verify the ZIP's exact byte size and SHA-256 before extraction, then verify all extracted files with SHA256SUMS. Use the prebuilt program without installing Node.js or compilation tools.

An absent Lite application means install it; a stopped Lite service means start it. Identify the existing Lite data directory from its running service or companion client configuration. Preserve its data, login, master key, port and authorized Chrome identity. If multiple data directories are found, ask me to choose. Never reset an existing account or use a new empty directory to bypass an error. This guide covers Apple Silicon; stop on an Intel Mac.

For Codex, use this binary's codex-package and connect-codex commands and verify the generated 0.9.0+codex companion plugin separately from the binary. The full plugin suffix depends on the target paths and data directory. Other supported clients use the same service through MCP. I complete sign-in, CAPTCHA and browser authorization.

Stop when a required guide step fails or tool permission is denied; report that step and its error. Do not remove quarantine, weaken security settings or bypass a macOS warning. This is an ad-hoc signed, unnotarized development candidate. Verify the running binary hash, service version and one live MCP account-status call. Use onecrew_web to verify the workbench and existing Library table if browser tools are available. Report any required client restart, login or checks still pending. Do not start creator searches or send messages or email.
