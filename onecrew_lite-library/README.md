# OneCrew

OneCrew is a local application built on the Teable v2 packages. It exposes the
same table operations through HTTP and the official MCP TypeScript SDK v2, and a
small React interface uses the public ReUI Data Grid registry component.

The engine is real Teable command/query execution backed by a directory
PGlite database. It creates one internal base on first start and persists tables,
columns, and records under `.data/pglite`. Dependencies are pinned as local
source packages in `vendor/teable`; runtime does not require an upstream checkout. The default address is
`http://127.0.0.1:8091`. Occupied ports are skipped across the next 19 ports.
`make status` reports the actual address from `.data/run/server.json`; stop/restart
manage only that recorded OneCrew process. The local MCP stdio entry
`dist/cli.js mcp` reads the same descriptor, so clients do not pin the HTTP port.
The Chrome extension automatically discovers the default 8091–8110 range.
Custom `PORT` ranges are available for HTTP/MCP and isolated tests, but are not
automatically discovered by the extension; use the default range for browser search.

Table responses include a detail link under `web_url`. Set `PUBLIC_WEB_URL` to
the public origin (and optional path prefix) used for those links; it must be an
`http` or `https` URL without credentials, query parameters, or a fragment.
For example: `PUBLIC_WEB_URL=https://tables.example.com/team-a pnpm start`.
It defaults to `http://127.0.0.1:${PORT}` (or port `8091` when `PORT` is unset).

```sh
# All normal dependencies install from the registry or project-local workspace packages.
pnpm install
pnpm build
pnpm start
```

Local process commands (development profile):

```sh
pnpm start
pnpm status
pnpm restart
pnpm stop
```

For UI development, run `pnpm dev` and `pnpm --dir web dev` in separate terminals.
The backend watches source files; Vite provides frontend hot updates and proxies
API and login callback requests to the verified running backend for the same
profile. It reads the service's actual port, including after port fallback.
The development page URL is printed by Vite. Both commands normally use `.data`.
To use another profile, run `pnpm exec tsx watch --conditions=onecrew:source src/cli.ts serve --data-dir <profile>`
and set `ONECREW_DATA_DIR=<profile>` for `pnpm --dir web dev`.
Stop the running binary for that profile before starting the development backend.

Direct CLI commands use the OS application data directory by default:

```sh
node dist/cli.js start
node dist/cli.js status
node dist/cli.js stop
node dist/cli.js serve --data-dir /absolute/path/to/profile
node dist/cli.js mcp --data-dir /absolute/path/to/profile
```

macOS: `~/Library/Application Support/OneCrewConnect`; Windows: `%LOCALAPPDATA%\OneCrewConnect`;
Linux: `$XDG_DATA_HOME/onecrew-connect` (defaults to `~/.local/share/onecrew-connect`).
`--data-dir` / `ONECREW_DATA_DIR` overrides the default. The npm development
commands and Makefile explicitly choose project `.data` as the development profile.
A new installation uses this separate data space and does not read or migrate
data from older OneCrew or `io.onecrewai.onecrew` directories. Sign in again on
first use.
A profile contains `pglite/`, `run/server.json`, `server.log`, and
`server.lock`. The lock file is persistent; ownership is an OS file lock, not
the file's existence or a recorded PID. The OS releases ownership after a crash;
the next start can replace a stale descriptor. MCP and CLI must select the same profile. Runtime files contain a
private control token and must not be shared. Stop requests authenticate the exact
instance over loopback and close the database; they never kill an arbitrary PID.
`serve` runs in the foreground. Background lifecycle does not depend on Bash.
Stop/start preserves data; task pause/resume is a separate future mail feature.

Use `node dist/cli.js --help`, `node dist/cli.js help mcp`, or
`node dist/cli.js mcp --help` for command help. `mcp` is a foreground stdio
bridge launched by an MCP client, not a plugin installer or service starter.
See [MCP client setup](MCP.md) for direct stdio/HTTP configuration and login
verification without a plugin. Plugin skills provide optional examples; the
server instructions, tool descriptions and schemas describe the required workflow.

`pnpm test:cli` validates lifecycle and persistence from another working directory.

Regression checks live in `tests/integration/`; shared test accounts and server
fixtures live in `tests/helpers/`. `scripts/` contains build, packaging and plugin
configuration tools. `pnpm test:database` checks initialization, persistence and
transaction isolation; `pnpm test:table-files` checks imports, exports and capacity.

Each authenticated user has a private tenant with its own Teable space/base.
HTTP and MCP resolve the verified principal once per business call and pass an
explicit `TenantContext` into services; table, field, row, export and search IDs
cannot select another tenant. Browser connections and search jobs retain their
tenant ownership. `pnpm test:tenancy` covers concurrent principals and cross-tenant
access, while `pnpm test:stdio` checks that an existing local MCP bridge rejects
account switches until it reconnects. OneCrew-owned tables have no foreign keys.
The local Access adapter remains a desktop login adapter; a SaaS deployment must
supply request-authenticated sessions and a production database/transport setup.
The `001_tenant_initial` baseline replaces the earlier disposable test schema;
old data is not automatically assigned to whichever user signs in first.

Build a standalone executable for the current OS and architecture with Node 25.5+
and `pnpm build:binary`. To package another target, use
`node scripts/build-binary.mjs --target darwin-arm64 --node /absolute/path/to/node`
(available targets are listed by `--help`; a cross-target Node executable must
match the build generator's version). The artifact is written to `release/`; it embeds
the Node runtime, server, web assets, database runtime and native dependencies.
Packaging uses Node's official `--build-sea` command after esbuild bundles the
application. Intel macOS uses the documented SEA blob + postject flow with Node
26.7.0 because the built-in Mach-O injector fails startup on that target
([upstream issue](https://github.com/nodejs/node/issues/65479)).
`scripts/sea-launcher.cjs` is a CommonJS JavaScript entry that extracts
and loads the embedded assets; it is not a C program.
Users do not need Node or the source checkout. On first launch, bundled runtime
assets are extracted to the OS user cache; databases remain in the data profile.
The package stages only the native dependency prebuild for the selected target.
For example, the macOS ARM artifact is `release/onecrew-macos-arm64`.

To build the four macOS and Windows distributions together on macOS, run
`pnpm build:release`. It runs the project build once, automatically downloads
and verifies official Node 26.7.0 runtimes for the host SEA generator and target
executables, then writes one folder and ZIP per target
under `~/Downloads/OneCrew-<version>-<git-short>/` by default. Pass `--target`
to package one target, `--output` to choose another output directory, or
`--skip-build` when the application server and web assets are already built.
The packaging host can keep its existing Node 25.8 installation; it does not
need a global Node upgrade.

The application release version is maintained in the root `package.json`.
CLI `onecrew --version`, MCP server metadata, and Access pairing all read that
version; the binary build embeds the same package metadata. Distribution plugin
versions are generated as `<app-version>+<git-short>`. The source plugin's
`+codex.*` version is only a development cachebuster and is replaced in release
bundles. Chrome extension versions are maintained separately in
`browser-extension/manifest.json`; do not lower an already shipped extension
version to match the application. The private contracts package has its own
workspace version in `packages/contracts/package.json`.

Each package contains a standalone executable at its root, Codex plugin files
and marketplace, an unpacked Chrome extension, one installer, and license
notices. Double-click `install.command` or `install.cmd`. It copies the package
to `~/Applications/OneCrewConnect` on macOS or
`%LOCALAPPDATA%\Programs\OneCrewConnect` on Windows, configures the installed
plugin path, then starts the service and prints its URL. This does not install
Codex or Chrome extensions, change PATH, or read, move, or delete data from the
older OneCrew or `io.onecrewai.onecrew` directories. The new data space is
`~/Library/Application Support/OneCrewConnect` on macOS and
`%LOCALAPPDATA%\OneCrewConnect` on Windows; sign in again on first use. The
package directory can be moved after installation by rerunning its installer.
Register the installed marketplace using an explicit path from any directory.
macOS terminal:

```sh
codex plugin marketplace add "$HOME/Applications/OneCrewConnect"
codex plugin add onecrew-tables@onecrew-tables
```

Windows PowerShell:

```powershell
codex plugin marketplace add "${env:LOCALAPPDATA}\Programs\OneCrewConnect"
codex plugin add onecrew-tables@onecrew-tables
```

The marketplace lives at `OneCrewConnect/.agents/plugins/marketplace.json`.
Do not register the Downloads copy with `marketplace add .`. The installer prints
the command with the installed path. Load the Chrome extension from the installed
`browser-extension` directory too; the downloaded copy can then be removed.

To move the installed program, run the matching command from its new location,
then reinstall the plugin cache:

```sh
./onecrew configure-plugin --plugin-dir <plugin-directory> --data-dir <data-directory>
.\onecrew.exe configure-plugin --plugin-dir <plugin-directory> --data-dir <data-directory>
```

Installation requires no Node and does not change PATH. An archive SHA256SUMS
file sits alongside the ZIPs. The
macOS Intel binary passed runtime acceptance under Rosetta on Apple Silicon; it
has not been tested on physical Intel hardware. Windows targets passed static
architecture checks; runtime acceptance is pending.

```sh
./release/onecrew-macos-arm64 start
./release/onecrew-macos-arm64 status
./release/onecrew-macos-arm64 stop
./release/onecrew-macos-arm64 help mcp
pnpm test:binary ./release/onecrew-macos-arm64
```

The binary integration test uses an isolated profile and a PATH without Node.
It covers startup, port conflicts, embedded web assets, MCP, crash recovery,
graceful stop and persisted records.

`pnpm build:teable` builds the checked-in Teable packages using their own build
scripts. Run it after installation or upstream source changes; `pnpm build`
also runs it before building OneCrew. `pnpm dev` runs OneCrew's TypeScript entry.
There is no prepare, download, archive, or source-copy step. The exact upstream
commit and local integration changes are documented in `vendor/teable/README.md`.

The page redirects from `/` to `/library`; the Library search is backed by the
engine's table list query (`q`), and each result opens `/library/:tableId`.
The detail view uses the public ReUI `DataGridTableVirtual` registry component.
The UI fetches 50-row server batches as the bounded grid nears its end; the
HTTP and MCP APIs retain explicit pagination for clients. Sorting clears the
loaded batch, and stale requests cannot overwrite a newer sort. Cells are
read-only until a double click, then Enter or blur saves through HTTP and
Escape cancels. New columns and rows are persisted by the same Teable engine.
Each table can hold up to 10,000 records and 100 columns. The limits are passed
to Teable's native capacity configuration when the database is initialized;
HTTP, MCP and file imports use the same write path. Zod also bounds each request.
Column menus can rename a field without changing its ID or values. The pinned
Actions column contains row deletion with confirmation and a disabled email
action reserved for the email module.

The Library imports `.csv` and `.xlsx` files into independent tables using
Teable's native import commands and adapters. CSV uses the first row as column
names; XLSX defaults to the first worksheet and detects its header. Uploads are limited to 10 MiB,
10,000 data rows and 100 columns. Invalid or oversized imports fail without
leaving an empty table.
The pinned Teable CSV parser rejects some one-column files when it cannot
detect a delimiter. OneCrew returns that parse error without modifying the
uploaded data; single-column XLSX files can be used instead.

Table detail exports XLSX with all records, including records not yet loaded
by the virtual grid, up to the XLSX worksheet limit of 1,048,575 data rows plus
the header. This is a readable export: numbers and booleans retain their
types, people/company cells contain names and profile links, and cell comments
retain available avatar URLs, identities and score reasons. It is not a backup
of the semantic schema; reimporting an export uses normal file type inference.

| HTTP operation | Request |
| --- | --- |
| Import a table | `POST /api/v1/dynamic-tables/import`, multipart `file`, optional `name` and `sheet_name` |
| Export a table | `GET /api/v1/dynamic-tables/:tableId/export?locale=zh-CN` (`en` is also supported) |
| Rename a column | `PATCH /api/v1/dynamic-tables/:tableId/columns/:columnId`, JSON `{ "name": "Name" }` |
| Delete a row | `DELETE /api/v1/dynamic-tables/:tableId/rows/:rowId` |

These routes require login and use the shared table service. `pnpm test:table-files`
checks file handling and table edits against an isolated PGlite database.

`GET /healthz` reports the engine and base ID. `Makefile`
has `start`, `stop`, `restart`, `status`, `build-web`, and `test` targets.

The `/connections` page reports a locally loaded Chrome extension. Load the
`browser-extension` directory as an unpacked extension in Chrome; it
automatically connects to the local service and retries after service restarts.
The page only reports status. Asynchronous YouTube searches are started through
MCP and the search API polls
the search status and paginated results endpoints. Results are stored in the
auxiliary `onecrew_search_jobs` table. The search service does not call table
operations. By default the agent evaluates the returned video/channel evidence
and saves each new search to an independent dynamic table using the existing
table tools: `people` (name, avatar and hidden identity), `website` (channel URL),
and `score` (match level and reason). An explicit preview-only request skips this
step. See [MCP workflow](MCP.md).
The extension bridge session is recreated automatically after a server restart.
`pnpm test:browser` checks automatic connect, authenticated command polling,
replay rejection, and timeout cleanup.

HTTP exposes table and row routes; there is no presentation
metadata API. Columns expose `{id, name, data_type}` and old `type` or
`presentation` payloads are rejected.
The MCP endpoint is `/mcp`, served by the SDK v2 `createMcpHandler`. Its native
stateless compatibility mode accepts both modern `2026-07-28` requests and
legacy `2025-06-18` initialize/list/call clients. Every tool is registered
directly, and both HTTP validators and MCP input schemas reuse the contracts in
`packages/contracts/src/tables.ts`. Clients should call `table_get` first and use
its generated column IDs for row values.
The MCP endpoint also exposes `platform_search_create`,
`platform_search_status`, and `platform_search_fetch` for asynchronous YouTube
searches. Cross-origin MCP and search requests are rejected.

Run the integration check to start a server, exercise HTTP CRUD, stop and
restart it, verify PGlite persistence, then use official v2 clients in pinned
modern and legacy modes to list all ten tools and call a query tool:

```sh
pnpm test
```

The integration server uses port `18092` by default so it can run while the
application stays available on `127.0.0.1:8091`; set `TEST_PORT` to use another free
loopback port.

`pnpm check` runs ESLint, formatting checks, backend TypeScript checking,
server and frontend builds, and HTTP/MCP/PGlite integration tests.
`pnpm test:distribution` separately installs production artifacts offline in a
temporary directory outside the checkout and launches compiled JavaScript from
an unrelated working directory. It requires a completed `pnpm build` and a populated pnpm store.

The HTTP server follows the official Hono Node.js starter layout: `src/index.ts`
calls `serve` from `@hono/node-server`, `src/app.ts` assembles middleware and
mounted routes, and `src/bootstrap.ts` composes the Teable/PGlite dependency
graph. Dynamic table code is kept in the coarse modules under
`src/modules/tables/`: `service.ts`
owns shared use cases, `data.ts` owns Teable commands/queries, and `http.ts` and
`mcp.ts` provide protocol mappings. `src/mcp/server.ts` registers the SDK
instructions and modules; `src/mcp/transport.ts` owns the SDK v2 handler
lifecycle. The reference was created with `create-hono 0.19.5` using its
Node.js template; the temporary reference is kept at
`tmp/onecrew-hono-nodejs-template-20260922` and produced Hono `4.13.8` with
`@hono/node-server 2.1.1`.

Web requests use [Hono RPC](https://hono.dev/docs/guides/rpc).
Each HTTP module returns its chained routes; `src/app.ts` exports `AppType`
from the mounted application. `web/src/lib/api.ts` creates `hc<AppType>`
and handles failed requests and login-state notifications. Calls use the typed
client for paths, query parameters, JSON bodies and file uploads. Response
models are inferred from routes, and cell input types come from `@onecrew/contracts/tables`.
The frontend imports `AppType` through the server's type-only `onecrew/rpc` export,
which is erased from the browser bundle.
The Node-only Vite configuration uses `onecrew/runtime` for local service discovery;
that module is not part of the shared contracts or browser client.
Changing an HTTP contract is checked against its Web callers by `pnpm typecheck`;
`pnpm test:rpc` exercises the client against the actual Hono application.

`packages/contracts` is the shared workspace package for table, search, and browser
bridge schemas. It only depends on Zod; database migrations, credentials, tenant
authorization, and Teable adapters stay in the server. Both the server and `web`
use `workspace:*` dependencies and one root lockfile. TypeScript project references
build contracts before server declarations and Web type checking; no handwritten
copy of the API types is maintained.

Production package exports resolve to `dist/*.js` and `dist/*.d.ts`. In development,
the `onecrew:source` condition used by `pnpm dev` and Vite resolves contracts directly
to their TypeScript source, so schema edits trigger backend restart and frontend HMR.
Normal builds and binaries use the compiled package. Run `pnpm build:server` after a
fresh checkout to build contracts and RPC declarations, or `pnpm build` to include
the Teable and Web builds as well.

Teable owns the table, field, record, native type, query, sort, and PGlite
persistence behavior. OneCrew stores semantic field types in the
`onecrew_field_types` table keyed by real Teable IDs. The public API has no
presentation concept. Structured people/company/score values are encoded in the
underlying text fields and decoded by the data adapter. Complex JSON columns
cannot be sorted. `src/database.ts` opens and closes PGlite and composes the
upstream Teable service registrations. `src/database-schema.ts` contains the
initial schema version, executed by Kysely's Migrator before HTTP/MCP start.
Teable's metadata schema and undo functions use their upstream installers;
OneCrew's three fixed business tables share the database and have no foreign keys.
The pinned PGlite contains PostgreSQL 17, so no legacy type-validation SQL is needed.
Pinned package manifests expose built declarations.
No Teable `apps/` code is copied or needed at runtime.

See [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md) for Teable and registry
source notes.

## YouTube browser search

Load the Chrome extension and let it connect to the local service as described in
[browser-extension/README.md](browser-extension/README.md). Search results are
persisted separately from dynamic tables; saving them to a table is explicit.

Additional checks use isolated test databases and simulated extension responses:

```sh
pnpm test:browser
pnpm test:search
node --test browser-extension/tests/*.test.js
```

These checks do not establish compatibility with a live YouTube session; run a
real channel search after loading the extension.

### 代码质量检查

- `pnpm lint`：后端、脚本、浏览器扩展和前端 ESLint 检查；前端包含 React Hooks 与 Fast Refresh 规则。
- `pnpm --dir web lint`：只检查前端。
- `pnpm typecheck`：检查后端、前端及 RPC 类型契约；`pnpm --dir web typecheck` 只检查前端。
- `pnpm format` / `pnpm format:check`：Prettier 自动格式化 / 只检查格式。
- `pnpm check`：依次执行 lint、格式检查、前后端类型检查、构建和 HTTP/MCP、RPC 集成测试。

配置统一在 `eslint.config.js`、`.prettierrc.json`。按官方渐进接入建议，`@shadcn/lint` 先启用 `no-restyle`：业务页面只通过 variant 控制组件外观，className 用于布局。现有违规已整理成组件 variants，零警告为检查门槛。基础组件内部关闭这条规则，让组件自己定义样式；其它 ESLint 规则继续执行。组件发现使用 `web/components.json`，也覆盖 ReUI 组件。参考 https://github.com/shadcn-ui/lint/blob/main/docs/adoption.md 。

ReUI/shadcn 基础组件允许混合导出 variants/hooks，因此仅在这些组件目录关闭 Fast Refresh 的单组件导出限制，业务页面仍检查。React Hooks 使用运行时正确性规则，没有启用项目尚未采用的 React Compiler 专属规则。

Codex 插件与 marketplace 已放在本项目；四平台分发包可用
`pnpm build:release` 生成。运行安装入口和注册 Codex marketplace 的步骤见
[plugins/README.md](plugins/README.md)。安装目录和独立数据目录均为 OneCrewConnect；首次使用需重新登录，旧版数据不会迁移。

## Access account and device diagnostics

The bottom-left account menu starts Access browser authorization with PKCE. The
local callback exchanges the one-time grant with Sync, verifies the installation,
and reads authenticated `GET /me` for `{ id, email }`. No avatar is returned; the
UI uses the email initial. The authorization device label uses the macOS ComputerName
(or hostname when unavailable) and the Windows hostname. Sync and its public API Router expose `/me`; Access is
unchanged. Credentials are stored in PGlite's `onecrew_access_accounts` table,
keyed by the verified Access user ID, with one active account per local profile.
Credentials are stored in plaintext; no system keychain or application encryption
key is used. The database directory is owner-only on macOS/Linux. Windows uses the
current user's LocalAppData directory and its inherited ACL. A custom data directory
must also be private to that operating-system user. Anyone who can read the database
or a full database backup can read its credentials. Credentials are never included
in Web/MCP responses, table exports, logs, or diagnostic reports.
Existing keychain credentials are not migrated; sign in again after installing
this version. Logging out clears only the active account's stored credential; it
does not revoke that credential remotely. Account credential separation does not
yet isolate the profile's tables or search history between users.

Local endpoints:

- `GET /api/v1/access/status`: current local account state.
- `POST /api/v1/access/login`: start browser authorization.
- `POST /api/v1/access/logout`: cancel pending login and remove local credentials.
- `GET /creator-sync/pairing-callback`: Access's fixed loopback callback.
- `GET /api/v1/system/info`: OS version/build/kernel, native and process architecture,
  Node version and translation status; available from the account diagnostics menu.

Account mutations require JSON and `X-OneCrew-Request: 1`; browser requests must
use the same local origin. Logout does not sign out the browser's Access session
or revoke other devices. After a validated login, diagnostics are uploaded through authenticated Sync
`PUT /device-info` and displayed in its installation admin view. Each local process
reports once per credential; a failed report retries on a later status check after
five minutes and does not fail login. Payloads exclude hostname, serial number
and home directory. Windows diagnostics are implemented but still require
validation on a real Windows host.

The local UI redirects to `/login` until Access validates the account. All table,
search and browser bridge HTTP endpoints require login; signed-out requests return
401, and unavailable account verification returns 503. MCP stays connectable for
tool discovery and the `account` tool (`status`, `login`, `logout`); each business
tool verifies login before execution. `account login` returns the local login URL
without exposing pairing codes or tokens. Logout immediately locks
business endpoints and closes the browser bridge session. Health, account, system
diagnostics and authenticated CLI lifecycle control remain available.

This is a login gate for one local data profile, **not tenant isolation**: existing
tables are not assigned to a user, and signing into another account in the same
data directory still accesses that profile's data. Use distinct data directories
until account-specific data isolation is implemented. Validate with
`pnpm test:access`; business tests inject a test identity in their test harness,
with no production environment flag to bypass login.
