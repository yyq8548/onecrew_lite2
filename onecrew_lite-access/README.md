# Reacher Access

Access Worker for the OneCrew deployment. This repository contains the Hono API
and the dedicated React/shadcn account application that Cloudflare Workers
Static Assets serves from `web/dist/access`. The public marketing site is a
separate deployment and is never an Access fallback.

This service owns registration, sign-in, browser sessions, the user-center
session projection and the Access-side Desktop-pairing/device lifecycle. Its public
API is intentionally limited to health, auth capabilities, email session
wrappers, the Google OAuth start/callback pair, and authenticated device
pairing authorization plus device list/status/revoke/delete routes. Other Better Auth account, token and session
endpoints are not mounted. Creator sync, enrollment, snapshots, quarantine and
admin APIs remain in the separate Worker B repository.
There is no MCP, Desktop, Tauri or local-agent connection in this service.

Every HTTP request reaches the Worker before static assets. The Worker requires
the request URL/Host to match the configured `ACCESS_ORIGIN`, redirects only
`/` to `/login`, and permits only the account/auth/pairing pages, existing build
assets, and the exact method/path pairs listed below. Unknown, marketing,
Creator Sync, Admin and sensitive paths return a bounded 404 without SPA asset
fallback. CORS preflight is accepted only when both the API path and requested
method are in the same allowlist.

## Runtime shape

- `src/index.ts` is the thin Worker entry.
- `src/app.ts` composes Hono middleware and routes.
- `src/auth/` owns Better Auth construction and session loading.
- `src/routes/session-auth.ts` owns password registration, password sign-in and
  sign-out wrappers; `src/routes/session-otp.ts` owns the email OTP send/verify
  and password-reset wrappers; `src/routes/session-shared.ts` keeps their JSON
  and safe-user projections shared. `src/routes/account.ts` owns the
  authenticated profile, password and email-change wrappers. The parent
  `src/routes/session.ts` only composes its Hono sub-apps with `route()`.
- `src/pairing/` owns strict browser authorization, PKCE/HMAC proof and the
  narrow named RPC contract reserved for Worker B.
- `src/devices/` owns the safe device DTO, pagination, status, cross-Worker revocation and closed-record deletion.
- `src/users/` owns the internal, read-only Access account identity projection
  used by the Creator Sync Admin. It returns email plus the latest binding IP
  and Cloudflare edge location recorded for that account's Desktop device and
  is not mounted as a public HTTP route.
- `src/rate-limit/` owns the permissive Cloudflare in-Worker coarse gate; its
  keys stay inside the binding and it writes no activation D1 IP counter.
- `src/middleware/` owns typed cross-cutting Hono API middleware built with
  `hono/factory`; `src/app.ts` only orders policy, mounts sub-apps and maps
  terminal errors.
- `src/config/` validates bindings and fails closed.
- `development/` contains deployment, D1 migration, Wrangler type-generation
  and immutable-migration-check helpers; runtime code does not own production
  operations.
- `migrations/0001_better_auth.sql` is the immutable Better Auth 1.7.1 core D1
  schema generated from an empty database.
- `migrations/0002_activation_devices.sql` appends Access-owned device,
  historical activation-verifier and bounded rate-limit storage.
- `migrations/0003_desktop_pairing_grants.sql` appends the new pairing grant and
  quota structures without rewriting deployed migrations.
- `migrations/0004_registration_ip.sql` is retained as an append-only legacy
  account-metadata migration; current Admin identity projection uses binding IP.
- `migrations/0005_device_pairing_ip.sql` adds binding IP and Cloudflare edge
  location fields to linked devices, plus the user-scoped lookup index.
- `migrations/0006_login_risk_session_idx.sql` adds a partial, user-scoped
  latest-session IP index so new-IP OTP checks do not scan the session table.
- `web/` owns the React/shadcn auth, pairing confirmation and device application;
  the deployable Access artifact is `web/dist/access`.
- `web/src/lib/i18n.ts` owns the `i18next` runtime and language detection;
  locale resources live separately in `web/src/locales/zh-CN/` and
  `web/src/locales/en-US/`. Access errors are returned as stable codes and
  translated in the client.

Better Auth is the only authentication implementation. Email/password uses its
native hashing and HttpOnly cookie session. Production auth cookies use the
`__Host-onecrew` prefix with `Secure`, `Path=/`, `SameSite=Lax` and no `Domain`
attribute, so they remain scoped to the Access hostname. Google is enabled only when both
Google bindings are present. OAuth access and refresh tokens use Better Auth's
at-rest encryption; Google ID tokens are discarded before account persistence.
When both Resend bindings are present, Better Auth's official `emailOTP` plugin
generates expiring, attempt-limited, single-use codes and the official Resend
SDK sends them. New-IP password sign-ins are challenged with the same OTP flow;
the client can also choose passwordless email-code sign-in; an unregistered
address is provisioned by Better Auth only after its sign-in OTP is verified.
Passwordless accounts can set a credential password from Account settings, and
existing credential accounts use Better Auth's current-password change flow.
Forgot-password reset and email changes use Better Auth's email-OTP endpoints,
including current/new email verification. Without Resend, the
legacy password response remains unchanged and the OTP controls stay disabled.
Every public send-code wrapper is guarded by a 60-second client cooldown, while
Better Auth's database-backed rate limiter enforces the authoritative 60-second
send window (`max: 1`) on its email-OTP send endpoint.
Authenticated session projections include the current account email; binding
IP is exposed on each linked device and to Creator Sync only through the private
`AccessUserDirectory` Worker RPC.
Database-backed Better Auth rate limiting trusts only Cloudflare's connecting-IP
header. The product does not persist a second bearer or session model.

Public API:

- `GET /api/health`
- `GET /api/auth/capabilities`
- `POST /api/session/register`
- `POST /api/session/login`
- `POST /api/session/login-otp/send`
- `POST /api/session/login-otp`
- `POST /api/session/register/verify/send`
- `POST /api/session/register/verify`
- `POST /api/session/forgot-password/send`
- `POST /api/session/forgot-password`
- `POST /api/session/logout`
- `GET /api/session`
- `GET /api/account/settings`
- `POST /api/account/profile`
- `POST /api/account/password`
- `POST /api/account/email/current-otp/send`
- `POST /api/account/email/current-otp/verify`
- `POST /api/account/email/change/request`
- `POST /api/account/email/change/verify`
- `POST /api/auth/sign-in/social`
- `GET /api/auth/callback/google`
- `GET /api/pairing/authorize?<strict Desktop request>`
- `POST /api/pairing/authorize`
- `GET /api/devices?cursor=<opaque-device-cursor>`
- `GET /api/devices/:deviceId/status`
- `POST /api/devices/:deviceId/revoke`
- `DELETE /api/devices/:deviceId`

Internal Worker RPC (not mounted on HTTP):

- `AccessUserDirectory.lookupUsers({ userIds })` returns each matching
  account's `userId`, `email`, latest `bindingIp`, and `bindingLocation` for
  Creator Sync Admin.

The Google routes are the only raw Better Auth HTTP forwards. Email session
wrappers call private in-process Better Auth HTTP endpoints so its validation,
origin checks and database-backed rate limiter run, then replace the response
body with a fixed token-free projection.

Desktop binds a native loopback listener, generates `state` and an RFC 7636 S256 verifier/challenge, then opens the
external browser. The website only previews the strict request until the signed-
in user explicitly approves it. The request includes one canonical
`callback_port`; Access alone builds a 303 loopback Location at
`http://127.0.0.1:{port}/creator-sync/pairing-callback`. D1 stores a
domain-separated request/code HMAC and the S256 challenge, never the raw
code/verifier/approval token.

The Account page no longer issues manual keys. It explains Desktop-initiated
pairing and retains safe device list/status/revoke/delete. Active or pending
devices expose only “解除绑定”; revoked or expired records expose only “删除设备”.
For consumed devices, Access first revokes the exact Worker B installation over
the internal `CreatorSyncDeviceRevocation` binding, then commits its own revoke.
Deletion rechecks the same B revocation before removing Access history.
Worker B Admin delegates the reverse entry point to Access through
`DesktopDeviceLifecycle`; Access runs this same B-first/readback/device-commit
state machine, so Admin revocation cannot leave the Account page active.

Rate limiting is deliberately layered. `EDGE_RATE_LIMITER` applies a coarse
per-location gate inside the Worker for auth/session/device routes and separately
keyed pairing reads and writes, after Origin/CORS policy and before Better Auth
or D1 work. Its executable cap lives in `wrangler.toml`. Pairing confirmation
writes also use Access D1's exact per-U authorization window; their separate
edge key means page refreshes cannot consume confirmation capacity. The edge binding is
permissive, eventually consistent and not a global exact counter; this service
allows the request if the binding call throws. Better Auth keeps its own D1
limiter for auth endpoints. Access D1 separately enforces exact pairing
authorization and per-grant proof-attempt limits, plus current-device capacity
and atomic one-time consumption. The pairing authorization quota stores no IP
or IP-derived bucket; Better Auth continues to own auth/session IP persistence.

`DesktopPairingAdmission`, `DesktopPairingRateLimitAdmin`, and
`DesktopDeviceLifecycle` are named Worker RPC entrypoints, not HTTP exchanges.
The rate-limit admin entrypoint only clears the exact per-U pairing authorization
bucket; it does not reset Better Auth, Cloudflare edge limits, or reopen a
proof-locked grant.
Worker B owns the reciprocal `CreatorSyncDeviceRevocation` entrypoint used only
for Access-initiated revocation. The exact handoff is in
[`docs/contracts/desktop-pairing-admission-v1.md`](docs/contracts/desktop-pairing-admission-v1.md).

## Bindings

Required D1 binding:

- `DB`

Required Cloudflare Rate Limiting binding:

- `EDGE_RATE_LIMITER` (per-location, permissive in-Worker coarse gate only)

Required variables:

- `ACCESS_ORIGIN`
- `BETTER_AUTH_URL`
- `WEB_ORIGIN`

Required secret:

- `BETTER_AUTH_SECRET`
- `PAIRING_HMAC_SECRET` (independent domain key; do not reuse the auth secret)

The auth secret must be a non-placeholder base64url value that decodes to at
least 32 bytes and contains sufficiently diverse characters. Plain HTTP origins
are accepted only for loopback development; non-loopback origins must use HTTPS.
`ACCESS_ORIGIN`, `BETTER_AUTH_URL` and `WEB_ORIGIN` must resolve to the exact
same root origin or every page/API request fails closed.

Optional secrets, which must always be configured together:

- `GOOGLE_CLIENT_ID`
- `GOOGLE_CLIENT_SECRET`

Optional Resend email-OTP configuration, which must always be configured
together:

- `RESEND_API_KEY` (secret; use a Resend `sending_access` key scoped to the
  verified sending domain)
- `RESEND_FROM_EMAIL` (non-secret Worker variable, for example
  `OneCrew <auth@example.com>`)

Configure the secret without putting it in Git or the browser bundle:

```sh
wrangler secret put RESEND_API_KEY
```

Set `RESEND_FROM_EMAIL` in the Worker environment (or in the ignored
`.dev.vars` file for local development). The sender domain must be verified in
Resend before delivery is attempted. The public capabilities endpoint exposes
`emailOtpEnabled` so the UI can keep the code-login option visibly disabled
until both bindings are configured.

Do not commit local or production secret values. For local development, put
the independent auth and pairing HMAC secrets in the ignored `.dev.vars` file. `pnpm run
dev:worker` explicitly overrides all three origins to one localhost origin; the default
`wrangler.toml` route, non-secret D1 ID and origins describe the custom-domain
Access target. `--local` remains required for local D1 migration. Migration,
type-generation and deploy helpers always pass an explicit validated Wrangler
config and optionally accept an auth profile. The validator requires the pinned
Access D1 binding/ID and rejects target-changing deploy flags, so these helpers
cannot fall back to the retired database by name. `db:migrate:remote` and
`deploy` remain explicit remote actions.

Examples (replace the profile name with the locally configured Wrangler auth
profile):

```sh
pnpm run types -- --config wrangler.toml --profile onecrew
pnpm run db:migrate:local -- --config wrangler.toml --profile onecrew
pnpm run db:migrate:remote -- --config wrangler.toml --profile onecrew
pnpm run deploy -- --config wrangler.toml --profile onecrew
```

The deploy helper freezes migrations `0001` through `0006`, builds only the
Access web artifact, verifies that
`web/dist/access/index.html` exists, and only then invokes Wrangler. It never
puts secret values on the command line.

## Local checks

```sh
pnpm run db:migrate:local
pnpm run check:migrations
pnpm run check:domains
pnpm run test:operations
pnpm run typecheck:worker
pnpm run test:worker
pnpm run build:access
pnpm run build:site
pnpm run build:worker
pnpm run build:site-worker
pnpm run check
```

The Site build uses React Router Framework Mode to pre-render the 22 manifest
paths in Chinese and English (44 HTML documents) while the Access build remains
isolated from marketing modules. These commands use only the local D1 and
Wrangler dry-run paths. Creating a
remote D1 database, applying remote migrations, uploading secrets and deploying
are separate explicit operations.
