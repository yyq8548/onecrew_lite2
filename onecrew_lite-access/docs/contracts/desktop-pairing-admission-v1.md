# Desktop Pairing Admission v1

This contract replaces the never-connected manual activation-key handoff. There
is no compatibility entrypoint, alias, fallback request or legacy field.

The canonical executable golden contract is
`src/pairing/desktop-pairing-admission-contract.ts`. Its strict Zod request/result schemas are the only
TypeScript source of truth. Worker B must import the same pinned module/package
before the coordinated deployment; it must not recreate a permissive mirror.

## Browser authorization

Desktop generates a canonical 32-byte `state` and an RFC 7636 verifier/challenge
pair, keeps the verifier in native process memory, and opens:

```text
/pairing/authorize
  ?state=<43-char base64url>
  &code_challenge=<43-char S256 challenge>
  &code_challenge_method=S256
  &callback_port=<canonical decimal 1024..65535>
  &platform=macos|windows|linux
  &app_version=<safe version token>
  &label=<safe device label>
```

Access accepts exactly one canonical decimal loopback port and constructs the
only callback itself:

```text
http://127.0.0.1:{callback_port}/creator-sync/pairing-callback
```

The port has no leading zero, sign, whitespace, fragment, host, path or URL
syntax. Access accepts no redirect URI, callback URL, host, origin, path, `next`,
`returnTo`, client ID or scope parameter. `state`, S256 challenge,
`callback_port`, user/session and approval intent are bound by the signed token
and persisted request HMAC. `state` is returned byte-for-byte in Location and is
validated by Desktop; Access does not treat it as authentication or CSRF proof.

`GET /api/pairing/authorize` strictly validates the request, requires the Better
Auth session and returns a five-minute session/user/request-bound approval token.
It creates no device, grant or code. Only an explicit user click may call strict
`POST /api/pairing/authorize`; mount, focus, session refresh and React StrictMode
must never approve automatically.

Approval creates one pending device and a 90-second grant, then returns a `303`
with `Cache-Control: no-store`, `Referrer-Policy: no-referrer` and the
server-built loopback `Location`. The browser form submits only the signed
approval token; it cannot override the port or any callback component. Desktop's
loopback listener receives the one-time code and original state.

## Persisted boundary

Access D1 stores only:

- Better-Auth-owned `user_id` through `access_device`;
- opaque `dev_` and `pair_` IDs;
- domain-separated HMAC-SHA-256 values for the browser request and raw code;
- the S256 code challenge;
- safe label/platform/app-version metadata, callback-bound request HMAC and
  lifecycle timestamps;
- bounded attempt and authorization quota state.

It never stores the raw code, code verifier, approval token, state, loopback URL,
session token or IP-derived pairing quota. `access_device.revoked_at` is the only
revocation fact. The grant owns only short-lived PKCE proof, expiry and one-time
consumption.

## Worker B Service Binding

The named target is exactly:

```text
DesktopPairingAdmission.consumePairingGrant({ code, codeVerifier })
```

The strict result is either `admitted` with exactly `userId`, `deviceId` and
`pairingId`, or one of `consumed`, `expired`, `invalid`, `rate_limited`,
`revoked`, `unavailable` with no IDs or extra fields. Unknown request/result
fields fail closed. The executable Zod schemas define all string formats.

Worker B obligations:

1. Accept code and verifier only in one bounded authenticated enrollment request.
   Apply caller/IP edge rate limiting before every Service Binding call; Access
   per-grant attempt counts are not an edge-abuse control and Access cannot
   derive a trustworthy caller IP from named RPC.
2. Keep both values in request memory; never log, trace, analyze, queue, retry or
   persist them.
3. Call the Service Binding once. Transport failure and `unavailable` are
   ambiguous and require a fresh Desktop pairing; never replay automatically.
4. Treat the RPC response as `unknown` and run
   `consumePairingGrantResultSchema.safeParse`. Schema failure maps to local
   `unavailable` and performs no enrollment/snapshot write.
5. Persist only the three opaque IDs plus Worker-B-owned enrollment state, and
   only after `admitted`.
6. Any other result performs no snapshot/provider/local-credential write.
7. `userId`, `deviceId` and `pairingId` are identifiers, never bearers.
8. Access and Sync D1, secrets, migrations and credentials remain separate.
9. Access device revocation calls Worker B's internal
   `CreatorSyncDeviceRevocation.revokeAccessDevice({userId, deviceId, pairingId})`
   first. Worker B revokes the exact credential idempotently; only its `revoked`
   result allows Access to commit `revoked_at`. Unknown, unavailable or mismatched
   results fail closed and remain retryable. Deleting a consumed closed device
   rechecks this revocation before Access removes its local device/grant records.
10. Worker B Admin revocation calls Access's internal
    `DesktopDeviceLifecycle.revokeAccessDevice({userId, deviceId, pairingId})`.
    Access owns the same canonical operation: it invokes Worker B's idempotent
    revoke first, commits `access_device.revoked_at`, and returns `revoked` only
    after both readbacks agree. Invalid, missing or unavailable outcomes leave
    both active and fail closed. Replaying an already revoked identity is
    idempotent and may be used to repair a pre-contract test split.

Desktop validates returned `state` before forwarding the code and verifier,
clears both after the one enrollment submission, and opens sync only after
Worker B has independently persisted an admitted enrollment.

## Coordinated deployment gate

This Access implementation is local-only until Worker B imports the same golden
contract and configures its new binding to `DesktopPairingAdmission`. The unified
gate must apply append-only `0003`, deploy Worker A and Worker B, then prove a
real cross-Worker call, strict transport failure handling and admitted-only
snapshot gating. The same gate must prove the admitted-enrollment revocation
path. Local WorkerEntrypoint tests do not claim that integration.
