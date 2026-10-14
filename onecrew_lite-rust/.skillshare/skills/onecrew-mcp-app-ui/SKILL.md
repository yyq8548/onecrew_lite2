---
name: onecrew-mcp-app-ui
description: Build isolated MCP App iframe UI, Host SDK state, and self-contained bundles. Excludes Web React and server-owned tool or continuation contracts.
---

# OneCrew MCP App UI

## Goal

Deliver self-contained MCP Apps whose contracts, Host calls, state controllers,
views, tests, and generated assets have one package-local owner.

## Hard Constraints

- **MCP-UI-01** `apps/mcp-ui` is an independent application package. It never imports `apps/web`, Web `node_modules`, HTTP clients, or loopback MCP endpoints; every runtime dependency is declared in its own manifest.
- **MCP-UI-02** Use only the `@mcp-ui/* -> src/*` alias inside source and tests. Keep widget entries to stylesheet import plus root mount; place SDK bootstrap and Host theme projection under `app/`.
- **MCP-UI-03** Keep wire types and strict parsing in `lib/result-contract`, Host tool calls and continuation coordination in `lib/result-client`, stateful orchestration in a feature controller, and React rendering in pure views/components.
- **MCP-UI-04** Do not broaden a parser, infer a namespace, or add response fallbacks to repair one Host. If a public schema, resource, App-only tool, or Host projection changes, pair with `onecrew-peer-adapters` and fix the protocol boundary.
- **MCP-UI-05** Use the official MCP Apps SDK as the single iframe transport. Never add a second JSON-RPC bridge, raw `postMessage`, operation-derived tool routing, local storage authority, or App-authored continuation text.
- **MCP-UI-06** A component renders already-derived state. Hydration attempts, continuation leases, refresh timers, mutation transitions, failure classification, and action eligibility stay outside leaf components.
- **MCP-UI-07** Keep shadcn/Radix primitives app-owned under `components/ui`, install their real dependencies here, preserve accessibility, and let Host variables theme the package-local token subset. Never reach into another app for a primitive or theme file.
- **MCP-UI-08** Tests live beside the MCP UI owner and use `@mcp-ui/*`, package-local fixtures, and Host mocks. Contract, client/continuation, view, and source-boundary behavior must not depend on a Web test runner or `../../` cross-app paths.
- **MCP-UI-09** Delegate vertical sizing to Host auto-resize, and treat the inline frame as a fixed budget rather than a starting size. The Host caps inline height, and a Host may ignore the height the App reports and keep its own frame whatever the App grows to, so design the card to fit that budget: disclosure that would add height — a fuller text, one row's details, an editor — replaces the card's body with a bounded view carrying a way back, and content that can run long is paged. Growing in place is the one thing the cap punishes, because the added rows and the action bar under them fall outside the frame rather than merely being pushed down. Fitting the budget is a design obligation, never a licence to clip: whatever a frame does not show must stay reachable by scrolling the document, so the root must not suppress the viewport's vertical overflow — `overflow` on the root propagates to the viewport, so suppressing it there turns clipped content into lost content. It is the page-level scrollbar that is kept invisible, not the scrolling. An inline card still adds no scrollport of its own, because an inner scroll region hides the very thing the card exists to show. Swapping and paging bound reading only: they must never stand between the reader and the decision, so the card's action bar is reachable without visiting every page or opening any panel, and it acts on the whole selection rather than on what is displayed. A page is sized from the room the Host stated measured against the rendered row and the action bar, never from a hand-picked count, which is a guess about someone else's screen. A card with genuinely more to show declares `fullscreen` in `availableDisplayModes`, asks for it through `requestDisplayMode`, and renders the mode the Host returned rather than the one it requested; a scrollport of its own exists only in that mode. Keep at most two decisions plus a way out, at the bottom of the content and never pinned, because the Host's composer can overlay an inline card's bottom edge. Show the small amount of structured data the action needs — never owner output flattened into rows, internal identifiers, audit payloads or storage timestamps. No visible page-level scrollbar, viewport- or percentage-height root outside the granted `fullscreen` mode, native `popover`, or modal `<dialog>`. Read the frame the Host states (`displayMode`, `availableDisplayModes`, `containerDimensions`, `platform`, `safeAreaInsets`) rather than inferring it, and choose layout from the element's own observed width, never from a media query, which reports the outer view inside an iframe.
- **MCP-UI-10** `build.mjs` resolves paths from `import.meta.url`, rejects Web/legacy aliases, emits one self-contained HTML file per widget, enforces bundle budgets, and regenerates committed adapter assets. Never hand-edit generated HTML.
- **MCP-UI-11** Do not change interaction semantics while doing a structure-only refactor. Preserve closed states, exact descriptor tool names, once-only mutation, continuation lease outcomes, and typed error copy through focused tests.
- **MCP-UI-12** Do not package, sign, install, launch native applications, or perform live effects unless the user explicitly authorizes them.
- **MCP-UI-13** Keep requested MCP App localization package-local; never import Web i18n or assume its saved locale applies to the Host. Preserve server-owned data and continuation messages, and pair Host locale schema/projection changes with `onecrew-peer-adapters`.

## Router

| Intent | Read |
|---|---|
| Change MCP App source hierarchy, Host SDK ownership, contract/client/controller/view placement, or package isolation | [Structure and Host boundaries](references/structure-and-host-boundaries.md) |
| Change widget entry discovery, source audit, self-contained output, generated assets, or verification | [Build and verification](references/build-and-verification.md) |

## Related Skills

- onecrew-peer-adapters — pair only when MCP tools, resources, schemas, App-only routing, or Host projection contracts change.
- onecrew-web-ui — pair only when the Web transcript or iframe Host changes, not for iframe source composition.
- onecrew-verify — run MCP UI test/check/build, generated-asset, Web consumer, and adapter evidence selected by the final diff.

## Workflow

1. Identify the widget entry, app bootstrap, strict contract, result client, controller, view, test owner, and generated asset.
2. Read the exact routed reference and confirm `apps/mcp-ui` has no Web import or undeclared dependency before editing.
3. Change pure contract/client/presentation helpers first with focused tests; keep effectful Host orchestration in the controller.
4. Compose accessible views from app-owned primitives and preserve Host auto-resize without nested vertical scrolling.
5. Run package tests and typecheck, build the self-contained assets twice, and require the second build to produce no diff.
6. Add Web or adapter lanes only when their consumed asset or public contract changed; report native/live observations separately.

## Output Contract

- Package-local owner and dependency boundary
- Contract/client/controller/view/test placement delta
- Host and interaction semantics preserved or explicitly handed to Peer Adapters
- Generated asset and bundle-budget result
- Exact verification results and unrun live/native lanes

## When NOT to use

- Do not use for Web pages, iframe sandbox hosting, or HTTP commands.
- Do not use for MCP tool/resource/schema changes, runtime interaction ownership, or provider/Creator policy.
- Do not use merely because a generated widget asset is embedded by the MCP adapter when its source did not change.
