# ADR 0008: Codex plugin and MCP Apps overlay

## Status

Accepted.

## Context

Codex is the first scheduled-agent client for this platform. Its plugin
system can auto-trigger skills on user intent, and its MCP Apps support can
render server-provided UI components inline, so structured parameters can be
collected through a form instead of prose questions. Both capabilities sit
outside this repository's process boundary, which raises the question of
what moves into a plugin and what must not.

ADR 0007 established the guidance split: MCP clients receive bounded registry
playbooks and exact operation schemas from the running server. The Codex plugin
adds one intent router with three conditional stable-policy references, not a
second per-use-case guidance catalog. A plugin-resident operation inventory
would reintroduce drift; plain MCP clients remain complete through catalog
search/get without installing a Codex plugin.

## Decision

The plugin is an overlay, never an owner.

- The Codex plugin ships one `reacher` router Skill. It sends the complete
  request as normalized English atomic capability intents to
  `onecrew_tool_search`, one per independent outcome with explicit toolkit
  filters and no execution values. It consumes a matching returned bounded
  playbook directly and calls `onecrew_tool_get` only for operations it is about
  to execute or presentation data it must prepare. Get remains Agent-only;
  `onecrew_result_present` is called only when the user must see or act on the
  prepared card. It may load at most one intent-specific stable-policy
  reference and duplicates neither field tables nor operation inventories.
- The plugin carries the MCP connection, but only in its machine-private
  staged copy. The bundled sources contain no endpoint, token, or
  `.mcp.json`; when `onecrew serve` starts the local MCP service it stages the
  marketplace into the selected data directory and renders `.mcp.json` there (owner-only
  permissions) with the live endpoint and bearer, stamping the staged
  manifest version with a digest of the actual staged plugin directory so
  Codex's version-checked plugin cache picks up any plugin-byte change — a
  rotated token or endpoint, but equally a changed router skill, hook, or
  manifest. Widgets are compiled into the MCP server binary and are not
  staged plugin bytes, so they do not participate in this digest. A digest
  of the token alone would leave other plugin changes stranded behind an
  unchanged version string. Installing the plugin is therefore the
  whole Codex connection: the setup document's Codex path removes any
  pre-plugin `[mcp_servers.onecrew]` entry instead of writing one,
  and per-client walkthroughs for other MCP clients are reduced to the
  generic connection facts. `local-service` keeps sole ownership of the MCP
  lifecycle, token, and setup capability; secrets and setup URLs never
  enter bundled plugin files, the shared Web, status DTOs, or logs.
- `development/release/version.json` owns the CLI/plugin base version.
  Package, Cargo, lock, and plugin manifests are synchronized mirrors;
  `development/scripts/build.sh` checks them before compiling a release.
- The business-intent action card is served by this loopback server as one
  versioned, embedded `ui://` resource attached only to
  `onecrew_result_present`. Resources never become workflow discovery: search/get
  remain the Agent-readable route, present is an explicit user-display step,
  and `onecrew_tool_execute` stays widget-free.
- The action card selects one closed business-intent view. Toolkit guidance is
  a read-only platform description plus a live registry-derived operation
  directory; business plans preview server-derived steps; simple operation
  forms expose typed fields rather than a JSON editor. The App never persists,
  pre-fills, echoes, or logs secret fields. Registry confirmation, revision,
  idempotency, and readback gates remain authoritative; UI state is not proof
  that an external action completed. The App reads and transitions the shared
  process-local interaction owner through its hidden App-only channel;
  browser storage and transcript prose carry no pending authority.
- Every headless flow stays complete without the plugin or inspector. The
  single router and generic App are conveniences over the same fixed meta
  tools, not alternate contracts.

## Consequences

- A Codex user gets one auto-triggered router and schema-driven parameter
  collection; a plain-MCP client receives the same catalog contract.
- Plugin bytes reach a new Codex session through the staged plugin digest.
  Server-compiled widget resources reach clients with the running MCP server
  binary and do not use the plugin cachebuster. Router bytes and its references
  do use that cachebuster. The setup document is for a
  first-time install, a lost marketplace configuration, or another MCP
  client, not for routine updates. Uninstalling the plugin deletes no OneCrew
  data or tokens.
- CLI plugin status is scoped to the current MCP run. Staging begins a new
  checking generation; only that generation's successful staged read may be
  compared with the exact installed digest-stamped version. A failed or pending
  generation cannot reuse an older ready projection.
- Only `onecrew_result_present` declares the generic UI resource. Registry closure
  tests keep its prepared payload and callable operation projection aligned
  with the selected get result.

## Amendment (2026-08-19)

This amendment records the historical pre-cutover Envoy host, which embedded multiple
compile-time templates through a custom protocol and per-widget typed native
peers. Phase 7 later removed those legacy widget families and mappings in
favor of the one action-card resource described below. The pre-cutover resource
reader accepted a known widget key with a stale hash for transcript replay.

## Amendment (2026-08-27)

ADR 0012 replaced flat tool/manual discovery with the stateless catalog at the
Phase 7 atomic cutover. The single router consumes search-returned bounded
playbooks and fetches exact operation schemas through get; it duplicates
neither field tables nor operation inventories. `onecrew_tool_get` is the only
App-bearing meta tool, while `onecrew_tool_execute` remains widget-free.
External Apps submit a selected registered operation through that executor;
the historical Envoy host instead mapped a closed interaction kind to explicit
typed native peers and never gained a generic JSON executor. Plugin staging, cache, secret, and
reload boundaries are unchanged. The current action card resolves from its
exact content-addressed URI. A valid retired operation-inspector hash resolves
to one inert version-incompatible App so historical conversations fail closed
without exposing a raw resource error; retired widget keys, foreign schemes,
and malformed paths remain unknown resources.

Envoy records each inspector card's action and continuation state against the
transcript tool-call identity. A remount or repeated click reads that recorded
state and cannot repeat a terminal owner action. The App may request a host
continuation, but App-authored message text is not authority: the host derives
the continuation from the recorded card identity and completion.

## Amendment (2026-08-28)

Toolkit-guide detail now derives the requested surface's complete operation
directory from the compiled registry and exposes only ID, title, and canonical
summary. The shared Web consumes that same secret-free projection through
typed HTTP; it owns no second toolkit inventory. The directory is navigation,
not schema or execution authority.

The historical Envoy ledger v4 bound Campaign create/update confirmation to the exact final
typed payload before one one-shot owner call. Cancel seals the current card
without an owner call or continuation. Agent edit seals it, emits a recorded
edit-request identity, and permits only one explicitly linked replacement
card; a Choice result never authorizes a Campaign write. External Hosts retain
weaker presentation-only dismissal/edit state and cannot claim historical Envoy
ledger truth.

## Amendment (2026-08-29)

Phase 10 supersedes the dated get-bearing and split Host-state behavior above.
Ordinary `onecrew_tool_get` calls are Agent-only and create no View;
`onecrew_result_present` is the fifth model-visible tool and the sole App-bearing
descriptor. A presentable get returns an opaque reference, present exposes the
full payload only to the component, and the Agent does not repeat that payload
in chat. The model-visible present result remains a compact receipt. The hidden
App-only state route returns the bounded secret-free component projection in
`structuredContent`, following the MCP Apps private-tool pattern rather than
depending on Host preservation of custom result metadata.

External Apps and the historical Envoy host now read and transition one bounded process-local
interaction owner in `platform-runtime`. External Apps use the hidden App-only
state route; the former Envoy used a typed native peer. Both share the same action
revision, cancellation, replacement, submit/unknown-effect barrier, terminal
state, and continuation policy without routing the shared Web through MCP. The state
is intentionally lost on process restart: missing, expired, evicted, and every
pre-0.4.1 card render read-only and cannot be revived from localStorage or the
transcript. The process-local owner creates no data-directory artifact or
legacy interactive compatibility path.

## Amendment (2026-08-30)

ADR 0013 replaces the catalog-first public mechanics in the decision and prior
amendments. The router now consumes toolkit-complete `onecrew_tool_search`,
batch-reads exact operation contracts with `onecrew_tool_get`, executes up to eight
independent calls through `onecrew_tool_execute`, and calls `onecrew_result_present`
only with an execute-minted `result_ref`. Public Plan/get-card/form branches no
longer exist. Choice is the registered no-effect `task.choice.request`
operation; operations requiring attended payload confirmation produce a
result-bound card before their owner runs.

The generic App renders only the bounded `ResultSnapshot` projection and uses
the hidden `onecrew_result_state` capability for confirmation, multi-question
Choice, cancellation, Agent edit, and continuation claim/ack. The App supplies
only exact bindings, answers, or bounded edit text; it never authors operation
arguments, owner payloads, result references, or continuation messages.

## Amendment (2026-09-10)

Choice is now the registered no-effect `onecrew.ask_user_question` operation
in the `onecrew` toolkit; `task.choice.request` was renamed on 2026-09-09 and
has no alias. The Campaign create/update operations were retired with Creator
schema v36 (release 0.6.0, 2026-09-10). External creator and content searches
now require the no-effect `onecrew.search.plan.prepare` operation, whose
input-required plan card is confirmed through the same result-bound App path
before a bound search runs. Earlier sections are dated records and are not
rewritten.

## Amendment (2026-09-15)

Codex keeps the absolute source path of the generated `onecrew-cli` marketplace
in its config. The marketplace, its receipts, and the local service's staged
executables therefore live in the fixed per-user install root owned by
`onecrew-local-service` (`~/.local/share/onecrew` on macOS and Linux,
`%LOCALAPPDATA%\OneCrew` on Windows), not in an OS cache directory the host can
purge while that registration remains. The retired cache directories are
neither read nor deleted; explicit plugin install replaces a registration at a
noncanonical source. Cached plugin status reads the registration without
querying Codex and reports a missing or relocated source. Business data stays in
the application data directory.
