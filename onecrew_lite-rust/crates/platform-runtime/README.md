# Platform runtime

`onecrew-local-runtime` composes the local application services shared
by typed HTTP and MCP adapters. `LocalPlatform` coordinates the data-directory lifecycle lock and is
the canonical entry point for account, credential, and Creator services.

## Public surface

`LocalPlatform` exposes typed application operations for:

- operator-account connection, status, management, and disconnect;
- browser capability discovery and app-owned login flows;
- Telegram phone and QR login;
- proxy validation, live connection self-checks, and connected profile operations;
- YouTube and Resend service credentials;
- access to the shared `CreatorRuntime`;
- composite read-only data preflight and startup-owned upgrades.

Account and credential workflows publish their own process-local
`PlatformChange` invalidations after known committed changes. Subscribers see
only `operator_accounts` and `service_credentials`, without entity data,
secrets, commands, or a durable cursor. Creator publishes its business scopes
independently, including mailbox projection commits. The local service merges
both streams and assigns the wire sequence; it does not coordinate owner
transactions. A later workflow error does not erase an earlier committed
change, while successful rollback preserves the previous notification behavior.

Results are public, secret-free projections. Secrets may enter a typed method
when required for login or a credential lifecycle operation (connect, rotate,
validate), but remain in process and are not returned. This runtime owns
credential and browser workflows through provider facades. Creator save
composes fresh provider reads into canonical Creator inputs here; Creator owns
its business policy and persistence. With the Creator profile refresh operation
removed, that save read and preparation path is the only mapping from provider
profile reads to Creator profile inputs, and no other operation reads a platform
profile and then writes Creator profile facts. Contact target resolution is
separate: it maps a provider target resolution into Creator's canonical target
evidence, and Creator commits the identity association without appending a
profile observation. Outreach preparation freezes the intent
inside the result owner; submitted grants drive provider effects and typed
Creator ledger records. Accepted, unverified, and record-pending effects remain
distinct from completed records. The runtime supplies one shared direct-message
selector for both writes and Creator conversation sync. Creator receives only
the thread-read port; toolkits retain the provider's write and readback methods.
The Email gateway lives in `credentials/email_gateway` and holds the same
credential-services instance. Its narrow Weak callback reads Creator mailbox
bindings through bounded async admission, then releases repository ownership
before provider I/O. Vault and health segments run on the blocking pool.

Toolkit functions under `src/toolkits/` provide application admission for
platform reads and writes. A platform read checks its input before admission,
then delegates once. The platform crate publishes the checks and constraint
constants for counts and page sizes, search queries, the X advanced search
criteria, the Instagram stable user ID, the YouTube channel ID and the Telegram
community locator. Simple presence checks stay in the toolkit function: it
trims handles and identifiers, strips a leading `@` from X and Instagram
handles, and rejects a blank handle or a blank X stable target, user, post or
reply receipt ID.

- X, Instagram and Telegram use the shared operator-account read in
  `toolkits/read.rs`. It checks a bound search authorization, admits the
  connection, opens the account's session through the shared
  `PlatformSessions` on the blocking pool, checks the search again and calls
  the facade. Telegram prepares its session there and connects only after the
  second search check; a failed connect leaves nothing open. Once connected, it
  checks the search a third time and closes the connection if that check fails,
  because the connect is itself a network round trip. Otherwise a
  read that returns, successfully or not, awaits the connection's close. A read
  that panics or is cancelled drops the connection instead, and tg-transport's
  `Connection` drop tells its sender pool to quit and aborts the pool task.
- YouTube uses its credential read, which admits and opens the service
  credential with the same search checks and commits the read's credential
  health before returning.

Peer-facing reads return `PlatformNativeData`: the platform model serialized
as a JSON object under the platform's own field names. Creator save and
outreach target resolution keep the typed platform models. The runtime
re-exports the constraint constants, `accepts_*` checks and input types that
peers need. A peer with a matching product endpoint calls this same entry; a
directory entry alone does not imply that HTTP exposes an executable route.

Every platform read reports `PlatformReadError`. Its local variants name
OneCrew's own outcomes (input, search plan, connection admission, session or
credential, credential health commit, interruption, native data shape). An
invalid input carries the platform-specific explanation constant its toolkit
function wrote, such as "YouTube search count must be between 1 and 50"; every
other local variant has a platform-independent message. A platform failure
carries the platform crate's protocol-native evidence (`DmDiagnostic`)
unchanged, marked when it follows a local commit such as a YouTube credential
health mark; the runtime adds no category of its own. MCP projects these outcomes without inspecting platform,
client or transport errors. An outreach relationship read failure becomes that
evidence, or a local diagnostic naming the local outcome. Provider response
bodies, error source chains, proxy addresses and session diagnostics are not
part of a read failure; session acquisition failures discard their diagnostic
text.

`toolkits/link_hub.rs` is the public entry to the private
`capabilities/public_web/` owner. That capability owns bounded, allowlisted
fetching and page parsing, with no account, Creator, registry or selection-cache
dependency. Every redirect is validated; relative links resolve against the
final page URL, while the originally requested URL remains the source. Creator
contact classification belongs to the Creator save use case.

YouTube channel reads retain full provider descriptions and raw About destinations;
save mapping attaches their actual public source URLs without classifying routes.
Failed bounded profile sections remain visible in the native provider result and
produce save-preview warnings. Search still performs only its page read and one
channel metadata batch, without per-result About, upload, or comment requests.

## Runtime state

The immutable `CapabilityCatalog` declares canonical toolkit/operation identities,
descriptions and explicit MCP-binding or Web-navigation support. It has no
handlers, schemas, application instances or execution state. Local service
builds one catalog and shares it with MCP registration and direct HTTP directory
projection. MCP validates its complete binding set against that independent
inventory and keeps protocol schemas, execution and search. A navigation entry
does not create a Web command; Web calls typed owners through its own command
declaration. See [ADR 0016](../../docs/adr/0016-neutral-capability-catalog-and-typed-peer-adapters.md).

`LocalPlatform::open` acquires the data-directory lifecycle lock, runs the
read-only composite preflight under that lock, and then constructs the vault,
shared sessions, credential services, retained Creator ports, and Creator runtime;
unsafe storage fails closed.

Pending browser, Telegram, QR, and high-level account-connection flows are
private in-process state keyed by opaque flow identifiers. Durable account and
credential state is written only after the corresponding typed workflow
transition. Startup reconciles vault-owned account references into public Creator
projections and resumes supported pending lifecycle cleanup.

MCP App result state is also private in-process state. External Apps reach it
only through the App-only MCP projection; no adapter owns a second card state
machine. The bounded, TTL-managed map
owns result/view binding, action and terminal CAS, and continuation leases, but
is deliberately not restored after process restart. A card whose result record
is absent fails closed and can only be shown read-only.

## Storage

The runtime coordinates, but does not merge, the owned stores:

- `local-vault` owns encrypted profiles and service credentials;
- `creator-runtime` owns business records;
- app-owned browser profile directories contain isolated browser state.

Cross-store references are opaque. Composite preflight verifies that required
stores, keys, and registered browser roots can be read safely before any store
is opened read-write.

`LocalPlatform::open` and the startup-owned upgrade serialize on the same
data-directory lifecycle lock; the lock is held only for the duration of open
or upgrade, not for the platform's lifetime. Upgrade delegates the database
migration to its storage owner after composite safety checks.
Only local-service startup invokes this upgrade after process-owner checks;
HTTP and MCP expose status, with business work gated until startup is ready.

## Failure model

Ordinary open fails closed when any owned component is unsupported, unsafe,
unreadable, or concurrently owned. It does not repair or migrate implicitly.

Account workflows return typed phases and capability codes for expected states
such as handoff, user input, reconnect, unsupported browser capability, and
cancellation. Unexpected provider, vault, or repository failures remain
secret-free and do not expose stored credentials.

Local-data preflight and upgrade return typed states, the storage owner's
impact model, block reasons,
support codes, and recovery metadata. If storage committed successfully but
runtime reconstruction fails, the result preserves that distinction and the
backup information.

## Dependency rules

- Compose application services here; do not implement provider protocols here.
- Keep HTTP and MCP protocol DTOs in their adapters; local-service owns the
  shared application lifecycle.
- Keep Creator business transitions in `creator-runtime`.
- Keep secret persistence in `local-vault`.

See the root [architecture map](../../ARCHITECTURE.md), the
[peer-adapter ADR](../../docs/adr/0002-peer-adapters-over-shared-runtime.md), and
the [storage ADR](../../docs/adr/0003-separate-business-and-secret-storage.md).
The process-local result lifecycle is specified by
[ADR 0013](../../docs/adr/0013-result-first-mcp-tool-router.md).
