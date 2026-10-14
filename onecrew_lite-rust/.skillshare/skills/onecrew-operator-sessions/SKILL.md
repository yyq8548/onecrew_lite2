---
name: onecrew-operator-sessions
description: Implement human account login, proxy identity, and transient/provider session lifecycles. Excludes service API credentials, Creator policy, and provider operations.
---

# OneCrew Operator Sessions

## Goal

Complete an interactive provider session while preserving user control, proxy identity, secret boundaries, and bounded cleanup.

## Hard Constraints

- **SESSION-01** Resolve the operator-account and proxy references before authentication.
- **SESSION-02** Validate proxy reachability and observable egress before opening login, using independent checks when available.
- **SESSION-03** Stop on failed or contradictory proxy evidence instead of silently using direct connectivity.
- **SESSION-04** Keep browser login visible, app-profile-owned, and under human control.
- **SESSION-05** Never click, dismiss, accept, or script macOS Keychain or provider security prompts.
- **SESSION-06** Follow runtime-reported Telegram QR, OTP, and 2FA states exactly and keep polling bounded by expiry/cancellation.
- **SESSION-07** Require explicit X Chat enablement and account/profile selection before accepting a transient passcode or X Chat operation.
- **SESSION-08** Keep cookies, sessions, OTPs, passcodes, and tokens in `local-vault` or bounded transient handoff only.
- **SESSION-09** Expose only opaque references, safe connection/capability state, timestamps, and sanitized errors.
- **SESSION-10** Verify completion with a read-only current identity/profile operation before marking connected.
- **SESSION-11** Clean up pending flows on cancellation, expiry, failure, disconnect, and shutdown; never retry without a bound.

## Related Skills

- onecrew-native-cli — Browser, Keychain, profile, path, or permission mechanics change.
- onecrew-capability — Provider session or identity transport behavior changes.
- onecrew-web-ui — Interactive onboarding UI changes.
- onecrew-service-credentials — The requested connection is a service/API credential instead of a human session.

## Workflow

1. Inspect safe account status, auth method, proxy reference, and pending flow.
2. Run the proxy gate and record only sanitized reachability/egress evidence.
3. Execute exactly one runtime-reported branch: visible browser, Telegram QR, Telegram phone/OTP, or 2FA continuation.
4. Verify the resulting identity and capabilities read-only.
5. Persist only safe references and status; clean up pending state on every terminal path.

## When NOT to use

- Do not use it for YouTube, Resend, or other service/API key capture.
- Do not use it for posting, messaging, discovery, email sending, or other provider actions.
- Do not use it for unattended automation that bypasses user-held authentication or security prompts.
