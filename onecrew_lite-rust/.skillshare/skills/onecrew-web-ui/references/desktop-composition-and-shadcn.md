# Web Composition and shadcn Contract

Use this reference for changes that add a shared control, compose a new
desktop surface, touch a raw HTTP call, introduce a query-key family, or
change an overlay, table, or multi-pane layout.

## Critical Rules

**A value used as a component type must have stable identity.** Rebuilding a
table `columns` array or markdown `components` map remounts its subtree and
destroys state. Hoist it or memoize it from stable dependencies.

```tsx
// ❌ FORBIDDEN: a fresh columns array each render is a fresh component type —
// flexRender remounts every cell; row state and scroll position die.
function LeadsPage() {
  const columns: ColumnDef<CreatorRow>[] = buildColumns(onStage);
  return <DataTable columns={columns} data={rows} />;
}

// ✅ CORRECT: stable identity — pages/Leads.tsx memoizes its columns;
// chat-markdown.tsx hoists its react-markdown map to module scope.
const columns = useMemo(() => buildColumns(onStage), [onStage]);
```

**Radix modality is document-wide.** `modal={false}` removes overlay, focus
trap, outside-tree hiding, and pointer blocking; this repository's Dialog also
nests Content in Overlay, so a non-modal Dialog renders nothing. Keep modal
behavior unless simultaneous background interaction is an explicit product
requirement.

```tsx
// ❌ FORBIDDEN: dialog.tsx nests Content inside Overlay; non-modal Radix
// skips the Overlay, so this dialog silently renders nothing.
<Dialog modal={false} open={Boolean(selectedId)}>
  <DialogContent>...</DialogContent>
</Dialog>

// ✅ CORRECT: a modal detail Sheet keeps Radix dismissal and focus behavior;
// the canvas portal limits where its overlay is painted.
<Sheet open={Boolean(selectedId)} onOpenChange={close}>
  <SheetContent container={canvas}>
```

**A virtualized table is a grid and does not size columns from content.** Give
every cell a shrink basis and explicit floor, then set the table floor to the
sum of visible column floors so overflow scrolls instead of crushing columns.
The current implementation lives with the data-table owner.

```tsx
// ❌ FORBIDDEN: width alone — a squeezed flex item ignores it, and with no
// table-level floor the columns collapse instead of side-scrolling.
<TableCell style={{ width: column.getSize() }} />

// ✅ CORRECT: data-table.tsx — basis shrinks to the declared floor, pinned
// columns never shrink (they hold the row's last reachable control), and the
// table floor is the sum of the visible column floors.
const floor = column.columnDef.minSize ?? size;
// per cell: flex: `0 1 ${size}px`, minWidth: floor, overflow: "hidden"
// on <Table>: style={{ display: "grid", minWidth: totalMinWidth }}
```

## Quick Reference

| Concern | Rule |
|---|---|
| New UI infrastructure | Inspect `components/ui` and `components.json`, then the shadcn registry and the headless `@shadcn/react` components, before hand-writing anything |
| Shared primitive admission | Business-agnostic, stable cross-feature API, expected to serve more than one surface |
| Shared primitive contents | No HTTP calls, React Query behavior, business policy, or feature wording inside a `components/ui` primitive |
| Primitive customization | Extend or compose the shared primitive; never copy it into a feature directory |
| Component identity | Module scope or `useMemo` for anything used as a component type |
| Overlay modality | Keep modal behavior for backdrop, outside/Escape close, focus trap, and focus return; use a non-modal Sheet only for an explicit simultaneous-interaction requirement, never a non-modal Dialog |
| Native overlays | Never native `popover` or modal `<dialog>` anywhere on the desktop (WKWebView freeze) |
| Virtual tables | Per-column `minSize` floors plus table `minWidth` equal to their sum |
| HTTP boundary | Typed feature client per touched command; no raw `invoke` in pages or components |
| MCP | The GUI never calls MCP; HTTP and MCP stay peer adapters |
| Query families | One explicit owner per key family and invalidation policy |
| External mutation | Runtime-event invalidation first; otherwise bounded polling plus focus refetch with a documented reason |
| Shell content | Never repeat shell title/subtitle; a primary action reappears only inside an empty state |
| Nav icons | Paired `{rest, selected}` glyphs from `NAV_GLYPHS`; never stroke weight or color alone |

## Common Patterns

### Surface and layout ownership

The shell owns title bar, primary navigation, global feedback, and the page-level
scroll boundary. Read the current route union and navigation
model from `apps/web/src/components/AppSidebar.tsx`; never copy the current
route inventory into guidance. A feature owns only its pane composition and
may request space from the existing shell without redefining it. For a
full-height workspace, identify one overflow owner per axis and verify
resizable panes, sticky chrome, overlays, and narrow windows.

### Brand, theme, and translated layout

`apps/web/src/components/brand.tsx` owns `OneCrewMark`, backed by the
canonical OneCrew SVG asset. Reuse it rather than drawing another logo or
assembling a text substitute. `apps/web/src/styles/theme.css` owns the
white light-theme background/card/sidebar surfaces and yellow accents; keep
its dark-theme tokens intact when adjusting a light surface.

Use content-driven card and control dimensions with the existing pane/overflow
ownership. Longer English copy needs wrapping and adequate control space, not
a smaller font or a giant fixed card. Pair React copy and locale formatting
with `onecrew-i18n`; translated labels, aria names, and memoized display
values must all respond to switching language.

### Contextual header uniqueness

Treat the shell title and subtitle as the page's single contextual heading:
no repeated title, subtitle, breadcrumb meaning, or explanatory sentence in
the body. Route-level primary and refresh actions go in the shell action slot
when one exists; otherwise one compact body action row without another title
block. Body content starts with information unique to the feature state. A
primary action may reappear inside an empty state (contextual recovery), not
in an ordinary first content card. A detail view may carry its own entity
title but must not restate the route title above it. Review the shell and the
first viewport as one composition even when they live in different files.

### HTTP client boundary

Page/component code does not import raw `invoke`. Use a feature-named typed
client helper under `apps/web/src/lib/`; command spelling and serialization
stay there. Migrate only touched calls and keep business policy in the runtime
or pure UI policy helpers.

### React Query freshness matrix

Name one explicit owner for every query family's key builders and
invalidation mapping — a feature module, or a shared domain/client module
when the cache crosses surfaces.

| Data origin | Required freshness policy |
|---|---|
| Local GUI mutation | Invalidate every authoritative affected key after success; optimistic state has rollback and eventual invalidation |
| Runtime data also mutated by MCP | Map an existing runtime change scope to the same authoritative keys; prefer the root runtime-event bridge (`lib/runtime-change-bridge.tsx`) |
| No runtime event available | Bounded polling plus refetch-on-window-focus, with a documented reason why an event cannot cover it |
| Immutable for the app lifetime | `staleTime: Infinity` only with evidence that neither GUI nor MCP can mutate it |

Do not use local component state or `setQueryData` as the only convergence
mechanism for shared persisted data, and do not duplicate literal query keys
in components when a key owner exists. Runtime events are an end-to-end
contract: when no existing scope covers a new externally mutable family,
either add the runtime scope through the architecture/adapters/runtime skills
(publish from every successful mutation path, forward through HTTP, map to
invalidation, test the chain) or document why that is out of scope and poll.
Never add only a TypeScript scope mapping for an event the runtime cannot
publish.

### Navigation icon states

Primary navigation communicates the active state with paired glyphs, never
stroke weight or color alone. `components/nav-icons.tsx` exports `NAV_GLYPHS`
— one `{rest, selected}` SVG component pair per view sharing a single accent
hue — and `AppSidebar.tsx` consumes only those. A new view adds its pair in
the same visual language; do not mix in a Lucide state-variant pair beside
the custom set, and keep both glyphs reading as the same object.

### Accessibility and interaction acceptance

Every icon-only control has an accessible name. Dialogs and sheets have a
meaningful title and description, visually hidden when the design omits them.
Preserve initial focus, focus trapping, escape dismissal, return focus,
disabled semantics, and destructive-action confirmation supplied by the
shared primitive. Keep keyboard and pointer behavior equivalent; announce
async failure through existing feedback primitives rather than color alone;
avoid layout shift between idle and loading; respect reduced motion.

### Verification layers

Normalization, policy, status derivation, transitions, query keys, and
invalidation mapping live in pure TypeScript with focused tests. Use the
existing interaction harness where needed; do not add a new browser or
accessibility stack incidentally. `onecrew-verify` owns standard command
selection and evidence reuse. Static checks do not prove focus, pane ownership,
native integration, or cross-MCP freshness, so record the relevant authorized
runtime observation or an explicit blocker.

## Checklist

- [ ] Every component-type value (columns arrays, components maps) is module
  scope or memoized.
- [ ] Overlays chose modality deliberately; modal detail surfaces preserve
  backdrop/dismissal/focus behavior, and no `Dialog` carries `modal={false}`.
- [ ] Virtual tables declare per-column floors and a table `minWidth` equal
  to their sum.
- [ ] Upstream (registry, `@shadcn/react`) was checked before hand-writing
  behavior, and any hand-rolled piece says so in a comment.
- [ ] No raw `invoke` in pages or components; touched commands go through a
  typed client.
- [ ] Each query family has one owner and complete post-mutation
  invalidation; externally mutable data has an explicit freshness policy.
- [ ] Shell heading is not repeated; the body starts with feature-unique
  content.
- [ ] Icon-only controls are named; dialogs and sheets are titled and
  described.
- [ ] Narrow windows, overflow owners, resize, and focus were verified in the
  running app or listed as not run.
