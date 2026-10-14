# MCP App Structure and Host Boundaries

Use this reference for `apps/mcp-ui` source hierarchy, Host SDK bootstrap,
result clients, state controllers, views, app-owned primitives, or tests.

## Critical Rules

```text
❌ Web page/component -> iframe result parser -> Host tool call -> React view
✅ widget entry -> app bootstrap -> feature controller -> result client/contract -> pure view
```

The iframe package owns its source, manifest, tests, theme, and build. The
Web may host the sandboxed iframe but is never a source or dependency root.

## Quick Reference

| Concern | Owner |
|---|---|
| SDK bootstrap and Host theme | `app/` |
| Closed wire types and parsing | `lib/result-contract/` |
| App-only calls and continuation | `lib/result-client/` |
| Effects and transitions | feature controller |
| Rendering and local form state | feature view/components |
| Test fixtures and Host mocks | `test-support/` |
| Entry and root mount | `widgets/<name>/` |

## Common Patterns

### Canonical package shape

```text
src/
  app/                 official SDK bootstrap and Host theme/context projection
  components/ui/       package-owned shadcn/Radix primitives
  features/<feature>/  controller, presentation, view, leaf components
  lib/result-contract/ closed wire types, parsers, and input invariants
  lib/result-client/   typed App-only calls, failures, continuation coordination
  styles/              package-local tokens and iframe shell behavior
  test-support/        typed fixtures and Host mocks
  widgets/<name>/      index.html and minimal root mount only
```

### Dependency direction

```text
widget entry -> app bootstrap -> feature controller -> result client
                              -> feature view ------> result contract
result client -------------------------------------> result contract
leaf components -----------------------------------> presentation/contract
```

`app/` may use the official MCP Apps SDK. `lib/result-client` may accept the
SDK `App` as a typed port. Contracts and presentation helpers stay SDK-free.
Views never call Host tools, claim continuation leases, or own retry timers.

### Host and protocol boundary

- The initial tool result is parsed once from the registered metadata contract.
- App-only calls use the exact descriptor name from the originating server;
  model-facing namespaces do not enter iframe routing.
- Host response shape is strict. A mismatch is classified without echoing
  response values and is fixed at the registered schema/Host boundary.
- Continuation context contains only the server-owned safe projection. The App
  sends the server-owned message and acknowledges delivered, paused, or
  delivery-unknown exactly once under the claimed lease.
- Remounts hydrate current state before enabling actions. Missing, expired,
  terminal, mismatched, or unknown authority remains actionless.

### Component admission

Use the upstream shadcn/Radix implementation in this package. A package-local
primitive is business-agnostic and has no Host call, result state, or feature
copy. Feature composition stays under `features/`; one-off business markup
does not become a shared primitive.

### Localization boundary

Web's `onecrew-i18n` instance, catalogs, aliases, and locale storage
are not dependencies of this package. When MCP App localization is requested,
keep its messages, translator, tests, and any new dependencies under
`apps/mcp-ui`. Derive locale only from a supported Host contract or an explicit
package-local choice; do not guess Host fields or import Web preferences.
Changes to Host locale projection or public schema belong with
`onecrew-peer-adapters`. Translate App-owned labels and accessible copy
while preserving provider/user data and server-owned continuation text.

### Test ownership

Keep four evidence families local:

- contract parser and cross-field consistency;
- result client, descriptor call, continuation lease, and failure categories;
- accessible business views and actionless terminal states;
- source-boundary negatives for Web imports, legacy aliases, raw bridges,
  storage authority, native dialog/popover, and retired protocol names.

Fixtures and SDK mocks belong in `test-support`. Tests import through
`@mcp-ui/*`; they do not traverse to another application or require another
application's Vitest config.

## Checklist

- [ ] No Web source, theme, alias, HTTP client, or undeclared dependency.
- [ ] Entry mounts only; Host SDK state is in `app/`.
- [ ] Contracts, clients, controllers, and views follow the dependency direction.
- [ ] Leaf components contain no Host calls, retry timers, or continuation leases.
- [ ] Tests use `@mcp-ui/*` and local fixtures/mocks.
- [ ] Parser or Host mismatches fail closed and route contract changes to Peer Adapters.

## Stop condition

Stop when every source path has one role, Host effects are outside views,
package tests own the behavior, and no Web dependency survives.
