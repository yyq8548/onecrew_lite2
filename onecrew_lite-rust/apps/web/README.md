# Web application

`apps/web` is the React webpage embedded into the `onecrew` CLI binary. It owns
view composition and ephemeral interaction state; application use cases stay in
the Rust runtime crates behind `local-service`.

## Embedding and transport

`npm run build` writes `apps/web/dist`. `crates/onecrew/build.rs` embeds every
file under that directory as `WEB_ASSETS` and fails the Rust build when
`dist/index.html` is missing. The `local-service` web router serves those bytes
and falls back to `index.html` for extension-less paths, so the page is one
native binary away from the user; `onecrew web` and `onecrew serve` open it
through a one-use `/launch` ticket, and `onecrew web --no-browser` prints the
URL for a side browser panel.

The browser reaches `local-service` over same-origin HTTP only:

- `GET /api/session` is the handshake (CSRF token, service version,
  `transport: "local_http"`); nothing business-related runs before it succeeds.
- `POST /api/commands/{name}` carries one typed human command with the
  `X-OneCrew-CSRF` header; the envelope is `{ ok: true, result }` or
  `{ ok: false, error }`.
- `GET /api/events` is one SSE stream of runtime change events.
- `POST /api/creator-import` uploads a Creator workbook.

`src/lib/local-runtime.ts` owns this transport. A `401`/`403` invalidates the
session, and readiness means a real handshake, never a synthetic native marker.
The webpage never calls MCP: HTTP and MCP are peer inbound adapters over the same
runtime owners (see the root architecture map).

## Boot and gates

`src/main.tsx` awaits `initializeI18n()`, attempts the runtime handshake (a
failure renders the connection gate instead of pretending to be a native
application), syncs the presentation locale, and mounts `App` inside the error
boundary, `QueryClientProvider`, and `I18nextProvider`. `App` layers
`LocalDataGate` and `PairingGateController` in front of the workspace.
`LocalDataGate` polls the read-only `local_data_status` command until the
service reports `ready`; a failure shows the typed failure copy, the support
code, and whether a recovery point exists. Local-service startup owns any data
upgrade; no webpage action executes one. The workspace views are `leads`,
`inbox`, `mail`, `accounts`, and `connect` (`components/AppSidebar.tsx`).

## Code layout

- `src/lib/` holds the typed feature clients (`creator.ts` wraps the Creator
  owner's commands behind `creatorInvoke`; `service-credentials.ts`,
  `operator-account.ts`, and siblings wrap their command families), pure
  helpers with focused tests, the query-key owners (`runtime-query.ts`,
  `creator-groups.ts`, `components/mail/queries.ts`), and the runtime-change
  scope-to-key mapping (`runtime-change.ts`, bridged by
  `runtime-change-bridge.tsx`). Wire mapping and non-visual derivation live
  here, not in components.
- `src/lib/generated/creator-sync-contracts/` holds TypeScript declarations and
  standalone validators generated from the `local-service` roots named in
  `creator-sync-contracts.config.json` through `development/schema-codegen`.
  Never hand-edit them: `npm run contracts:generate` regenerates the complete
  target and `npm run contracts:check` byte-compares the committed output (both
  spawn `cargo test`).
- `src/pages/` and `src/components/` compose views from the shared shadcn
  primitives in `components/ui`. The typed clients in `src/lib/` are the
  command boundary for pages and components; today `pages/Accounts.tsx` and
  the account-management components under `pages/accounts/` (`columns.tsx`,
  `proxy.tsx`, `connect-wizard.tsx`, `tg-login.tsx`,
  `resend-credential-dialog.tsx`, `x-chat-passcode-dialog.tsx`) still import
  `invoke` from `local-runtime.ts` directly, and a command touched from now on
  moves behind a typed client (`onecrew-web-ui` UI-08). Pages and components
  do not query SQLite, read vault files, call provider protocols, or recreate
  workflow transitions.
- `src/i18n/` holds the `zh-CN` catalogs (which define the key shape), the
  `en-US` mirrors, locale detection, and the shared `appI18n` instance.
  `src/i18n/resources.test.ts` enforces key and interpolation parity.

## Freshness

React Query caches are process-local. A local mutation invalidates every
authoritative key it affects. Changes made through MCP arrive as
`runtime://changed` events with typed scopes; `runtime-change.ts` maps each
scope to the local read keys it invalidates and never to a query whose
`queryFn` reaches a provider. Reconnecting the event stream invalidates every
scope, so lag becomes a full refresh rather than a partial-state assumption.

## Storage ownership

The webpage owns no persistent store. `local-service` startup selects the data
directory, runs read-only inspection and any supported upgrade, and gates
business commands on readiness. `creator-runtime` owns the Creator store
(`crm.db`) and `local-vault` owns encrypted credentials (`agent-platform.db`
with its `vault.key`). Browser storage carries per-browser presentation
conveniences rather than business data, credentials, or tokens: the saved
locale preference (`onecrew-locale`, `src/i18n/locale.ts`), persisted
data-table column filters (`data-table-filters:*`,
`components/data-table/filter-state.ts`), and the shadcn sidebar open-state
cookie (`sidebar_state`, `components/ui/sidebar.tsx`).

## Failure model

Startup and upgrade failures arrive as the typed `DataStartupStatus` with stable
support codes; the UI renders them and offers only re-inspection, never a
bypass. Provider and runtime errors are displayed secret-free:
`src/lib/provider-error.ts` separates the localizable title and hint from the
provider detail. Request acceptance is not completion: an external write is
shown as done only when the owning runtime has confirmed or safely classified
its outcome.

## Checks

From `apps/web`: `npm run typecheck`, `npm test` (vitest, including raw-source
contract tests that read `crates/local-service` and `crates/platform-runtime`),
`npm run build`, and `npm run contracts:check`. From the repository root,
`node development/scripts/agent-check.mjs` selects the web lane whenever
`apps/web/**` changes.

See the root [architecture map](../../ARCHITECTURE.md), the
[native-process ADR](../../docs/adr/0001-native-process-and-loopback-mcp.md), and
the [local-data ADR](../../docs/adr/0004-explicit-forward-local-data-evolution.md).
