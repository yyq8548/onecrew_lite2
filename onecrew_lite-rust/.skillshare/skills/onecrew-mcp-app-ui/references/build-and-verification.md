# MCP App Build and Verification

Use this reference when entry discovery, aliases, theme sources, source audits,
bundle budgets, generated HTML, or verification changes.

## Preconditions

- Confirm the source change belongs to `apps/mcp-ui`, not the Web iframe Host or an MCP public contract.
- Record the current generated asset diff and bundle budgets before changing the build.
- Confirm package-local test/check/build commands and `@mcp-ui/*` alias resolution.

## Workflow

### Build boundary

`apps/mcp-ui/build.mjs` resolves the repository and application roots from
`import.meta.url`; caller working directory is not an input. Each directory
under `src/widgets/` is one entry and emits one inlined HTML document.

The build must:

- resolve `@mcp-ui` to this package's `src`;
- use one React/React DOM instance;
- reject Web imports, `@/*`, `~/*`, and relative paths escaping `src`;
- keep scripts and CSS inline with no external documents or media;
- enforce raw, gzip, and Brotli budgets for HTML, JS, and CSS;
- delegate vertical sizing to Host auto-resize;
- write only the generated adapter asset and remove temporary `dist` output.

Generated `crates/mcp-adapter/assets/apps/*.html` files are committed build
outputs. Rebuild them from source; never repair a generated file directly.

### Verification sequence

From the repository root:

```bash
npm --prefix apps/mcp-ui test
npm --prefix apps/mcp-ui run check
npm --prefix apps/mcp-ui run build
git diff -- crates/mcp-adapter/assets/apps
asset_hashes="$(mktemp)"
shasum crates/mcp-adapter/assets/apps/*.html > "$asset_hashes"
npm --prefix apps/mcp-ui run build
shasum -c "$asset_hashes"
```

Then select only affected consumers:

- Web test/typecheck/build when the embedded asset or iframe Host changes;
- `cargo test -p onecrew-mcp-adapter apps::tests` when committed assets,
  resource hashes, retired resources, or embedding checks change;
- Peer Adapter verification when a public MCP/Host contract changed.

Matching generated-asset hashes after the second build prove deterministic regeneration,
not runtime behavior. A real Host render, user action, remount, and continuation
observation remains separate native/live acceptance.

## Failure recovery

- Fix source-boundary failures at the import or manifest that crossed the package; never allowlist a Web path.
- Fix generated-asset drift by rebuilding from source; never patch committed HTML.
- Fix budget failures by removing accidental dependencies or code before changing a limit; change a limit only with measured intentional scope.
- Treat a Host-only mismatch as a Peer Adapter/Host contract investigation, not permission to weaken the App parser.

## Stop condition

Stop after package-local tests/check/build pass, the second build is clean,
budgets pass, generated assets match source, and every affected consumer lane
is either evidenced or explicitly unrun.
