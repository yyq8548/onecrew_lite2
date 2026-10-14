# Telegram Target Semantics

Read this reference when a provider capability discovers, resolves, reads,
messages, or persists Telegram targets. The types live in
`crates/tg-client/src/models.rs`, locator classification in
`crates/tg-client/src/parser.rs`, resolution and writes in
`crates/tg-client/src/client.rs`, canonical mapping in
`crates/platform-runtime/src/outreach/targets.rs`, and Creator-side action policy in
`crates/creator-runtime/src/target_resolution.rs`. Verify current signatures in
source; this reference owns the semantics, not a frozen inventory.

## Critical Rules

### Classify the locator before any network call

`parse_locator` is a pure, total classifier: every input becomes a `TgLocator`
with one `TgLocatorKind`, and each kind owns exactly one resolution path in
`resolve_target`. A nested `t.me` message link, a `tg:` special link, or an
unclassifiable string must never degrade into a username lookup — and a bare
numeric peer ID is classified `Unknown`, never a username.

```rust
// ❌ FORBIDDEN: stripping a t.me path to "something username-shaped" resolves
// the wrong peer — t.me/creator/123 is a message link, not @123.
let name = request.locator_value.rsplit('/').next().unwrap();
let peer = client.resolve_username(name).await?;

// ✅ CORRECT: classify first; only username and invite kinds reach MTProto,
// and the static kinds return BrowserOnly before an account is even loaded.
let locator = parse_locator(&request.locator_value);
if let Some(result) = static_browser_only(&locator) {
    return Ok(result); // MessageLink, PrivateMessageLink, SpecialLink, Unknown
}
let resolution = account.resolve_target(&locator).await?;
```

### Never fabricate peer authorization

The stable identity of a target is its bare ID, carried as the `external_id`
string across every layer. An access hash is a separate, observed
authorization: it may only come from a non-min provider result (see
`TgChat::access_hash`), travels as a string so desktop JSON never loses i64
precision, and is never invented, defaulted, or zeroed. A numeric community
target without an observed hash fails closed
(`numeric_community_peer_ref` in `crates/tg-client/src/client.rs`).

```rust
// ❌ FORBIDDEN: a made-up hash forges authorization Telegram never granted
// this account; a zero hash is still a fabricated credential.
let peer = PeerRef {
    id: PeerId::channel(1_688_680_661).unwrap(),
    auth: PeerAuth::from_hash(0),
};

// ✅ CORRECT: numeric targets are reachable only through the access hash the
// account actually observed; absence is a typed error, not a guess.
let peer_ref = numeric_community_peer_ref(expected_id, target.access_hash.as_deref())?;
```

### Re-verify kind and stable ID at the write boundary

Usernames are mutable aliases. `send_dm` accepts a `TgUserTarget` (stable
`external_id` plus `username`), re-resolves the username at send time, and
rejects the write when the peer is a bot, is not a user, or now carries a
different bare ID. `conversation` in the platform adapter applies the same
check for reads that imply a relationship.

```rust
// ❌ FORBIDDEN: trusting a stale resolution; @name may have been released
// and re-registered by someone else since the target was saved.
client.send_message(cached_peer_ref, text).await?;

// ✅ CORRECT: resolve again, gate on kind, then compare the stable ID
// before the network write (send_dm in crates/tg-client/src/client.rs).
let user = match client.resolve_username(&name).await? {
    Some(Peer::User(user)) if !user.is_bot() => user,
    _ => return Err(TgError::Other(format!("@{name} is not a human user"))),
};
if user.id().bare_id_unchecked().to_string() != target.external_id {
    return Err(TgError::Other(format!("@{name} now resolves to another Telegram user")));
}
```

## Quick Reference

| Concern | Rule |
|---|---|
| Locator input | `parse_locator` first, always; each `TgLocatorKind` has one path |
| Raw numeric IDs | Never resolvable locators; `parse_locator("1688680661")` is `Unknown` |
| Access hashes | Observed non-min values only, carried as strings; never fabricated |
| Stable identity | `external_id` (bare-ID string) survives every mapping; usernames are aliases |
| Invite links | `CheckChatInvite`: joined/peek resolve; `Invite` is a Preview with no stable ID |
| Message/special links | `BrowserOnly` without loading an account (`static_browser_only`) |
| DM writes | Human `User` targets only; re-resolve and verify kind plus ID before send |
| Conversation reads | Resolved human `User` only; peer ID must equal the resolved external ID |
| `Bot` targets | Visible and read-only (`OpenProfile`); candidate save rejects them |
| Communities | `Channel`/`Supergroup`/`Group` open; admins only for `Channel`/`Supergroup` |
| Admin permission | `CHAT_ADMIN_REQUIRED` stays Telegram's `TgError::Rpc`; its read evidence keeps that RPC code and name |
| Read failures | `TgCommunityReadError` local conclusions carry Telegram-owned codes; RPC errors keep code and name (`community_read_evidence`) |
| Resolution trigger | Explicit operator account and explicit resolve action; never auto-resolve |
| Provider types | `Tg*` models never cross into Creator or frontend (CREATOR-05); MCP carries platform-native data and decodes only the runtime-re-exported locator |

## Common Patterns

### Target kinds survive every mapping

`TgTargetKind` (`User`, `Bot`, `Channel`, `Supergroup`, `Group`) is preserved
one-to-one into the canonical `TargetKind` in
`crates/platform-runtime/src/outreach/targets.rs`, and `valid_target_kind` in
`crates/creator-runtime/src/target_resolution.rs` accepts exactly those five kinds
for Telegram. Collapsing kinds (for example treating a `Supergroup` as a
`Group`, or a `Bot` as a `User`) silently changes which actions the Creator owner offers.

### Kind decides capability, and the table lives in the Creator owner as canonical policy

`actions` in `crates/creator-runtime/src/target_resolution.rs` grants: `User` gets
`OpenProfile` and `DirectMessage`; `Bot` gets `OpenProfile` only; `Channel`
gets `OpenCommunity`, `ReadPublic`, and `ExtractAdministrators`; `Supergroup`
gets `OpenCommunity` and `ExtractAdministrators`; `Group` gets `OpenCommunity`
only. Only a Telegram `User` attaches as a Creator identity
(`attach_as_creator_identity`). That table speaks canonical `TargetKind` —
provider policy and `Tg*` types stay behind the capability port per CREATOR-05,
and provider responses stay out of Creator contracts per CAPABILITY-06.

### Four resolution outcomes, all fail-closed

`TgTargetResolution` is `Resolved`, `Preview`, `BrowserOnly`, or `NotFound`.
An invite that the account has not joined returns `Preview { kind, title,
canonical_url }`; the adapter maps it to `BrowserOnly` because Telegram
exposes no stable target ID before access — do not promote a preview title
into a resolved target. `NotFound` and `BrowserOnly` are answers, not errors:
they map to typed `PlatformTargetResolution` variants, never a retry loop.

### Resolution is an explicit operator action

`resolve_contact_target` in `crates/creator-runtime/src/target_resolution.rs`
requires an explicit `operator_account_ref_id`, and a missing account maps to
`RequiresOperatorAuth` instead of a silent login. Discovery finding a `t.me`
route persists the route only; never run target-resolution MTProto merely
because a route exists. Static `BrowserOnly` kinds short-circuit before the
account loads, so classifying a pasted link costs no session at all.

### Communities resolve by username or by observed reference — with readback

`community` and `community_admins` in `crates/tg-client/src/community.rs` take
a `TgCommunityLocator`: a public username, or a saved stable ID with its kind.
`validate` rejects a malformed locator before admission or network access. The
stable-ID path uses only an observed peer reference, from this session's cache
or at most 2,000 dialogs, never a fabricated hash. Both paths verify the peer
is a channel/supergroup, and the stable-ID path also verifies that its ID and
kind still match, before returning the community or its administrators.

A conclusion the client reached itself is a `TgCommunityReadError` variant:
`InvalidLocator`, `NotFound`, `UnsupportedKind`, `IdentityMismatch` or
`PeerUnavailable`. `community_read_evidence` in `crates/tg-platform` turns it
into local evidence with a Telegram-owned code and explanation, and a Telegram
failure into `evidence`, which keeps the MTProto RPC code and name. The runtime
reads through its shared operator-account read, checks a bound search again
once the connection opens, and closes an opened connection on every outcome
the read returns; a cancelled or panicking read drops it, which aborts the
transport pool task. MCP projects the result without a Telegram classifier.

### Administrators become Creators only through candidate save

`community_administrator_actions` in
`crates/creator-runtime/src/discovery/candidate.rs` offers `SaveCreatorCandidate`
only for a Telegram `User` with a non-empty public handle and a Handle-kind
Telegram contact route; `Bot` administrators never gain the save action, and
`creator_candidates/persistence.rs` rejects a `Bot` save outright. Adapters
and UI never write Creator or contact rows directly (CREATOR-03).

## Checklist

- [ ] Every inbound locator passes through `parse_locator` before any network call.
- [ ] `MessageLink`, `PrivateMessageLink`, `SpecialLink`, and `Unknown` return `BrowserOnly` without loading an account.
- [ ] No code path invents, defaults, or zeroes an access hash; numeric targets without an observed hash fail closed.
- [ ] `external_id` stays the bare-ID string across transport, client, facade, adapter, and Creator mappings.
- [ ] DM and conversation paths accept only human `User` targets and re-verify kind and stable ID at the boundary.
- [ ] `Bot` targets stay read-only and are rejected by candidate save.
- [ ] Admin extraction targets `Channel`/`Supergroup` only and keeps `CHAT_ADMIN_REQUIRED` as Telegram's RPC code and name.
- [ ] Community read failures cross as `community_read_evidence`, keeping RPC code and name and no proxy address or phone number.
- [ ] Invite previews surface as `BrowserOnly` and never gain a fabricated ID.
- [ ] Resolution runs only from an explicit operator-account selection and explicit resolve action.
- [ ] No `Tg*` provider model crosses into `creator-runtime` or frontend contracts; MCP receives platform-native data and the runtime-re-exported locator only.
