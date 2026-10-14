---
name: onecrew-web-ui
description: Build or fix Web React surfaces, shared controls, rendering, and freshness. Excludes MCP iframe internals, native shell mechanics, and catalog-only translations.
---

# OneCrew Web UI

## Goal

Deliver coherent Web interactions without duplicating shell content, runtime policy, or feature state inside components.

## Hard Constraints

- **UI-01** Reuse existing shadcn/Radix primitives before creating any shared primitive, and check whether upstream ships one before writing behaviour by hand — the registry and the headless components in `@shadcn/react`. This covers behaviour, not just markup: scroll anchoring, focus management, virtualisation and drag are where a hand-rolled version looks finished and is not. Hand-write only after upstream has been checked and does not cover it, and say so in a comment.
- **UI-02** Keep business composition in its feature owner and add a shared primitive only when it is business-agnostic and reused.
- **UI-03** Preserve accessible names, focus behavior, keyboard dismissal, and dialog/sheet labelling.
- **UI-04** Keep status derivation, state machines, query keys, and contract normalization in pure TypeScript helpers.
- **UI-05** Give each query family and invalidation policy one explicit owner.
- **UI-06** Invalidate every affected query after mutation and define freshness for data MCP can change externally. A query an invalidation can reach must be a **local read**: anything that calls a provider is an explicit mutation that invalidates the local read once it settles. Invalidating a query whose `queryFn` syncs makes the sync re-trigger itself — that loop has already rate-limited a live account, and code comments were the only place the invariant existed.
- **UI-07** Prefer runtime-event invalidation; otherwise use bounded polling plus focus refetch with an explicit reason.
- **UI-08** Wrap every touched HTTP command in a typed feature client; never call raw transport from a page or component.
- **UI-09** Never call MCP from the webpage.
- **UI-10** Preserve shell, pane, overflow, resize, narrow-window, and overlay ownership.
- **UI-11** Never repeat shell-owned title/subtitle content or duplicate a primary action except as empty-state recovery.
- **UI-12** Never package, sign, notarize, or build installers unless explicitly requested.
- **UI-13** Use accessible shared overlay primitives in the browser. MCP App iframe source composition belongs to `onecrew-mcp-app-ui`.
- **UI-14** Do not fix a desktop defect you cannot observe. When the symptom lives in the running app — scroll position, focus, layout, anything that only appears on a real window — get an observation before editing. A change made to test a hypothesis is a change you then have to justify keeping, and a fix that resembles the cause is worse than no fix, because it retires the search. Follow the observing reference routed in the Workflow; it also covers getting a console at all (release bundles have none).
- **UI-15** Reuse `OneCrewMark` for the brand and preserve the white light-theme app/card/sidebar surfaces, yellow accents, and existing dark tokens. Let cards and controls fit their content; do not impose giant fixed card dimensions or shrink fonts to hide longer translations.
- **UI-16** Pair with `onecrew-i18n` for React copy or locale-aware presentation changes; translate accessible and transient UI copy as well as visible labels, while preserving runtime values and user-authored data.

## Related Skills

- onecrew-i18n — React copy, language switching, or locale-aware formatting changes; catalog-only wording belongs to i18n without this UI owner.
- onecrew-peer-adapters — HTTP commands or public DTOs change.
- onecrew-mcp-app-ui — MCP App iframe source, Host SDK state, or self-contained widget bundles change.
- onecrew-native-cli — Native presentation or OS capability changes.
- onecrew-verify — Run helper, typecheck, frontend, and smoke evidence.
- onecrew-local-service — A spinner or stalled command is caused by host-side blocking rather than React.

## Workflow

1. Identify the owning feature, shell-provided title/subtitle/actions, typed client, query owner, and nearest pure helper.
2. Read the matching reference before editing:
   - [Web Composition and shadcn Contract](references/desktop-composition-and-shadcn.md) for primitives, overlays, tables, multi-pane layouts, HTTP clients, or external freshness.
   - [Observing a defect you cannot see](references/observing-a-defect-you-cannot-see.md) before touching code for a defect that only reproduces in the running app (UI-14).
3. Model state and contracts in pure helpers with focused tests, then compose the React surface from existing primitives.
4. Wire complete invalidation/freshness and cover loading, empty, ready, action-required, error, and disabled states.
5. Check keyboard/focus, accessible naming, narrow windows, overflow, and action uniqueness; inspect both supported languages when copy or locale formatting changes.
6. Run focused tests, typecheck, and non-packaging frontend build as proportional evidence.

## When NOT to use

- Do not use it for runtime domain behavior, persistence, MCP schemas, or provider login mechanics.
- Do not use it for React, Host SDK, contract/client/controller/view, tests, or build code under `apps/mcp-ui`.
- Do not use it for release packaging or native installers.
- Do not use it merely because a backend change has a desktop consumer when no UI code changes.
