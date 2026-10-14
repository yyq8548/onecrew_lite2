# Teable source dependency

Pinned upstream: https://github.com/teableio/teable/commit/5ef2238883cad7c3980084de9a9031135fb9734f

These are checked-in source packages, not generated archives. Root pnpm-workspace.yaml
lists the 15 packages explicitly. They contain the command/query core, PostgreSQL
repositories and transaction support, and CSV/Excel import adapters with their
dependencies. The v1 core is required by upstream type declarations;
v2-tsdown-config supplies the upstream build configuration.
No Teable application or UI package is included.

Install with `pnpm install` at the OneCrew root; build with `pnpm build:teable`.
Build outputs are ignored by Git. No prepare or bootstrap generator is used.

Local integration changes:
- Package type exports point to built declarations; i18n runtime exports point to dist.
- Retain build tooling and type dependencies; omit upstream lint/test-only dependencies
  and scripts from package manifests. Test sources are preserved.
- Exclude Vitest globals from build-facing tsconfigs.
- Add explicit `.js` extensions to Day.js plugin imports for native Node ESM.
- Include the base TypeScript config required by package extends paths.

The upstream v2 packages are private monorepo packages, not published npm packages.
OneCrew imports their package entry points and supplies its own PGlite lifecycle
and Kysely connection driver. It uses native Teable schema installers and service
registration functions. The upstream test container and its exclusive dependencies
are not included. Binary packaging embeds PGlite WASM/data assets.

See LICENSE for upstream licensing. The retained packages declare MIT.
