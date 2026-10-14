# OneCrew OpenCLI source-evidence adapter

This development adapter is additive. It registers
`onecrew-youtube/video-evidence`, not `youtube/video`, and uses the official
OpenCLI page API, daemon and extension. The reviewed compatibility target is
OpenCLI 1.8.8. It does not send messages, override an official command, install
itself, or provide arbitrary URL/JavaScript execution to callers.

The input is one exact YouTube video ID. The adapter fetches the fixed same-origin
watch page with a 20-second deadline and 8 MiB limit. Redirects are rejected. The
projection verifies the video and channel owner IDs, preserves the original
`shortDescription`, and separately labels links from the original description
and description-only endpoint runs. It does not collect recommendations or
comments. The original HTML/player response and redirect tracking token are not
returned. Text and link counts are bounded; oversized descriptions are rejected,
not cut into plausible partial URLs. A link observation is not proof that the
destination is a tool, and metadata alone does not prove a genuine demonstration.

`onecrew-opencli::OpenCli::youtube_video_evidence` is a separate typed facade.
It checks the selected connected profile, registered read capability, OpenCLI
version, and exact installed adapter bytes before and after execution. File
checks are drift detection, not a sandbox against a hostile local administrator
or a malicious OpenCLI loader. The Campaign video-metadata step now invokes this
facade after its local preflight and the existing budget claim. The persisted
metadata keys and video-completion format are unchanged; it uses only the raw
description's literal links, not endpoint links absent from that description.
Existing records are not rewritten. A complete source-bound Campaign tool-page
probe has not yet been rerun after this wiring: platform expansion was prioritized.

Developer tests (from `onecrew_lite-rust`):

```text
node --test integrations/opencli/evidence.test.mjs
cargo test -p onecrew-opencli --target-dir target/gnu-check
cargo clippy -p onecrew-opencli --all-targets --target-dir target/gnu-check -- -D warnings
```

For the explicitly authorized development installation, copy only the two files
under `onecrew-youtube/` into the previously absent
`%USERPROFILE%/.opencli/clis/onecrew-youtube/`. The official loader resolves its
own package imports. Never overwrite another directory or the official adapter.
Verify file equality and command registration before a read. This is not the
customer installation experience: an idempotent, conflict-aware installer and
connection-wizard integration are now implemented in source as described below;
customer installation acceptance is still pending.

The opt-in Rust test `source_evidence_contract_matches_connected_extension`
requires `ONECREW_OPENCLI_TEST_PROFILE` and the explicit adapter installation.
The 2026-09-24 Windows test used the dedicated profile and passed: video
`htZRCE2GgIs`, stable owner `UCwAnu01qlnVg1Ai2AbtTMaA`, original description
1,805 UTF-8 bytes, 23 link observations across two source types. These are not
23 unique links or creators. The reported subscriber text was `1.9M subscribers`.
Seven JavaScript tests and 27 default Rust tests passed; ten environment tests
were ignored by default, with this one subsequently executed and passed.
Clippy passed. This does not prove Campaign integration or the three-run target.

## TikTok read-only search adapter

`onecrew-tiktok/search` is a separate additive command for OpenCLI 1.8.8. It uses
the official daemon, extension and page API with the explicitly selected profile.
It does not replace `tiktok/search`. The official navigation path returned
`Navigation rejected` in the development environment. A bounded comparison found
that an owned `newTab` followed by `setActivePage` reaches the fixed TikTok origin;
the internal cause of the original navigation failure remains unproven.

The adapter disables automatic pre-navigation, checks the origin and visible
verification challenges, and makes one fixed same-origin search GET. Query and
limit are validated before browser use. The request has a 15-second deadline and
4 MiB response limit, rejects redirects, and does not retry. Authentication,
verification and rate-limit failures stop the read. No credentials, raw response,
arbitrary target URL or caller-supplied script are exposed.

The typed Rust facade retains string video and author IDs, the original caption,
and an exact source URL. It rejects changed adapter bytes, unknown output fields,
invalid identities, duplicate video IDs and oversized output. The generic search
facade projects these records into source-backed leads with a bounded caption
preview. Neither facade claims follower evidence or qualified creators.

Development installation uses only `onecrew-tiktok/search.js` and `search.mjs`
under the previously absent `%USERPROFILE%/.opencli/clis/onecrew-tiktok/` directory.
Never overwrite an unrelated directory or an official adapter. Verify byte
equality and registration before reading. Customer-safe installation through the
connection wizard now has a source implementation; these copy steps are not onboarding.

```text
node --test integrations/opencli/evidence.test.mjs integrations/opencli/tiktok-search.test.mjs
cargo test -p onecrew-opencli --target-dir target/gnu-check
```

On 2026-09-24, the native opt-in test
`tiktok_search_contract_matches_connected_extension` returned three source-bound
videos for `ecommerce`. A separate generic-facade test returned five leads.
These are separate bounded searches, not eight unique creators or Campaign
qualification. The selected profile was `8pqz35v6`; no messages were sent.

## Explicit connection-wizard installation

The Web wizard exposes an explicit approval button for the five bundled add-ons.
Its typed HTTP command calls the shared runtime and the OpenCLI owner; the GUI
does not call MCP. Status inspection has no installation effect. Installation
accepts no arbitrary path, URL, script or platform operation and requires the
reviewed OpenCLI version. The user still installs official OpenCLIApp and the
Chrome extension through their official interfaces.

The installer creates only missing fixed add-on directories, writes bundled core
files before command entry files, and reads back exact bytes. Matching installs
are no-ops. An existing partial, modified, linked or unexpected directory is a
conflict: no overwriting, automatic cleanup, repair or upgrade is attempted.
Known symlinks and Windows junctions are rejected; this is not a security sandbox
against a hostile local administrator changing paths concurrently. Filesystem
work runs on a blocking worker. Public states expose only IDs and bounded enums.

After every install outcome the Web client rereads setup status; it never retries
installation automatically or equates verified files with platform login. Missing
commands, invalid files, browser connectivity and successful site reads remain
separate facts. Safe replacement/upgrades, a real GUI click-through and clean
customer-machine installation remain acceptance work; no new installer has been
published. Outreach features and plans are retained but are outside current work.

## TikTok current-account login probe (2026-10-02)

OneCrew routes TikTok login checks to `onecrew-tiktok-auth/whoami`, bundled
with `onecrew-site-auth`. The selected OpenCLI profile opens and binds a new
`https://www.tiktok.com/explore` tab. It does not use the upstream `goto` path
that failed during the reported login check.

The probe verifies the current viewer in `webapp.app-context` against the
visible `nav-profile` link. Public author payloads, cookies alone, zero IDs,
missing hydration, mismatched links and challenge pages cannot qualify.
Only a boolean leaves the page; no account ID, handle or cookie is returned.
Missing signals remain unknown, rather than being reported as logged out.
The shared add-on is hash-verified before and after the check; existing owned
files are replaced only through the explicit repair owner, with a backup.

```text
node --test integrations/opencli/site-auth.test.mjs
```

## Bilibili public search-card adapter

`onecrew-bilibili/search` uses the official OpenCLI page API with automatic
pre-navigation disabled. It binds a new tab to the fixed public video-search
page, verifies its origin and exact query, then reads at most 100 rendered cards.
The output contains at most the requested 1–50 videos, with exact BVID, author MID,
author name, title and canonical video/author URLs. This is the current rendered
page only, not exhaustive search coverage or pagination; fewer rows than the
requested maximum are permitted. No direct API calls or extra content reads occur.

Observed paid redirect cards (`cm.bilibili.com/cm/api/fees/pc/sync/v2`) and course
cards (`www.bilibili.com/cheese/play/ss…`) are explicitly excluded. They are never
followed or reinterpreted as ordinary creator videos. Other ambiguous cards,
source conflicts, missing fields, login walls and verification challenges stop
the read. An empty/unloaded result page is an unverified failure, not a valid zero
result. The adapter currently does not distinguish a genuine empty page from
an incompletely loaded one and does not retry automatically.

The Rust native facade and generic search entry both use this registered add-on,
never fall back to official `bilibili/search`, and verify installed bytes before
and after execution. The existing approved local installer now includes its two
files, preserving the YouTube and TikTok files. Setup reports and the Web schema
recognize the third add-on without claiming site login or successful reads.

On 2026-09-24, the dedicated development profile passed a native-facade search for
`跨境电商`: five videos from five distinct author MIDs. An independent generic
search returned five source-backed leads. These results cannot be added together
as ten creators and do not establish followers, relevance to customer criteria,
qualified KOL, three-run acceptance or clean customer installation. The original
official adapter's navigation failure remains unresolved.

```text
node --test integrations/opencli/bilibili-search.test.mjs
# Explicitly selected profile, after approved local add-on installation:
cargo test -p onecrew-opencli --target-dir target/gnu-check bilibili_search_contract_matches_connected_extension -- --ignored --nocapture
```

## TikTok public-profile evidence

`onecrew-tiktok-profile/read` is a separate additive read command. It requires
both the username and stable author ID observed during search. The target is a
fixed TikTok profile URL, opened through the selected official OpenCLI browser
identity. It reads one public profile bootstrap, bounded to 4 MiB, and never
returns the raw bootstrap or requests another API. Navigation, login challenges,
missing/ambiguous bootstrap data and identity drift stop the read without retry.

The adapter uses `wait({ time: 3 })`: the installed OpenCLI's numeric `wait(3)`
may return early when the DOM stabilizes, before profile hydration is available.
One initial diagnostic lacked the bootstrap; an explicit-wait observation passed.
This is not a proof that every future load will finish in three seconds. A slower
or changed page remains an explicit failed read, never fabricated profile data.

The native result retains the exact account ID, username, original biography,
display name, native integer follower count, verified/private-account flags and
canonical profile source. Counts are required, nonnegative, JavaScript-safe
integers. A real zero is accepted; missing/null/rounded text counts are not
converted to zero. Search-author ID and username must both match the profile.

`OpenCli::tiktok_public_profile` verifies the read command and installed adapter
files before and after execution. `LocalPlatform::read_opencli_tiktok_profile`
delegates to that owner for an explicit native read. The Campaign flow additionally
uses the fixed `creator_profile_evidence` facade for observed authors, with budget
reservation and a durable native-evidence receipt. It never treats profile-read
success as proof of content relevance. See the budgeted receipt section below.

The separate install directory permits adding this capability without modifying
the existing TikTok search files. The connection wizard recognizes its status and
installs it only through the existing explicit local setup action.

The 2026-09-24 native search-to-profile test read `https://www.tiktok.com/@dropship`,
matched author ID `6864157692300477445`, and observed native follower count
`4700000` with a 72-byte biography and `private_account=false`. This is one public
profile observation, not customer eligibility, audience geography, AI-tool demo
verification or three-run acceptance.

## Bilibili public-profile evidence

`onecrew-bilibili-profile/read` accepts the exact author MID observed in search.
It opens a fixed public profile through the selected official OpenCLI identity,
checks the origin and MID, and reads the unique public follower metric labelled
`粉丝数`. The exact integer comes from that metric's `title` tooltip; the visible
abbreviation is retained separately and is never expanded into an estimated count.
Missing, malformed or ambiguous metrics, identity drift, login walls and visible
verification challenges stop the read. There is no API fallback or automatic retry.

The result preserves MID, display name, canonical profile URL, exact tooltip text,
display text and `evidence_source=profile_tooltip`. A genuine zero is permitted;
missing counts are not zero. The Rust facade independently validates the result,
registered read capability and bundled file bytes. The shared runtime exposes
`read_opencli_bilibili_profile`; the Campaign flow now also supports a budgeted
author-profile step. The native read alone does not change Creator records.

The fifth add-on has its own fixed installation directory. Existing installations
are neither overwritten nor repaired. The connection wizard recognizes all five
IDs and rejects incomplete or duplicate setup receipts.

On 2026-09-24, a native search-to-profile test for `跨境电商` matched author MID
`8420618` at `https://space.bilibili.com/8420618`. The observed tooltip was
`1,534,709`, while the display was `153.4万`. This is one time-bound public profile
observation, not qualification against customer criteria or a stable-count claim.

```text
node --test integrations/opencli/bilibili-profile.test.mjs
```

## Bilibili public video-page fallback

On 2026-09-29, the official `bilibili video` command received an HTML response
from its view API for a saved shoe-video BVID, so JSON parsing failed. The public
video page itself displayed a title and author and carried one `VideoObject`
JSON-LD record. The additional `onecrew-bilibili-video/read` command runs only
after that official command reports a command failure. It opens the exact saved
BVID, checks the canonical page URL, visible title and author, JSON-LD video URL
and author MID against the saved source, and rejects missing or conflicting
evidence. A generic login prompt for likes is not treated as a video login wall
when the video evidence is present. CAPTCHA/risk text, an actual login wall,
payment marker or identity drift stops the read.

The fallback leaves metrics empty and marks `payment_status_known=false`; it
cannot prove free playback or paid rights. Title and page description are
review clues, not proof that a video demonstration was watched. A direct test
of `BV1bTYv6cELe` and author MID `168310298` passed on the connected test
profile; it does not change the saved shoe Campaign's earlier failed receipt
or silently upgrade a candidate.

```text
node --test integrations/opencli/bilibili-video.test.mjs
OPENCLI_TEST_PROFILE=<connected-profile> cargo test -p onecrew-opencli public_video_page_fallback_matches_the_saved_author --lib -- --ignored --nocapture
```

## Budgeted Campaign profile receipts (source 0.6.8)

After first-round platform coverage, Campaign advance can select `read_profile`
for a saved TikTok/Bilibili author. The Creator owner derives the platform,
browser identity, author ID and exact profile source from the search hit. It
reserves one global read call and commits `reading` before dependency/provider
access. One successful or failed receipt is stored per Campaign/platform/browser
identity/author ID; repeated videos from the same author do not trigger rereads.

Only an explicitly interrupted read is eligible for restart, with a fresh budget
charge and call number. Old receipts cannot complete a newer claim. Authentication,
risk-control and other provider failures remain durable stops, not retry loops.
HTTP `onecrew_discovery_profile_get` and MCP `onecrew.discovery.profile.get` read
the saved native public evidence locally without another provider call. Web's
typed client accepts this receipt and continues budgeted foreground steps; a
dedicated visual profile-evidence panel is not yet implemented.

Successful receipts with an observed display name now add a discovery candidate
in the same transaction, using the existing candidate format and list/count
operations. Platform plus native author ID is the identity, never the display
name. Existing candidates and decisions are preserved on conflict. Exact native
follower counts are assessed against the frozen criteria, but profile text or
search titles cannot prove a creator's own demonstration, topic, market or tool
link. Missing content proof stays pending; an observed count below the required
minimum is rejected. Missing names retain the native receipt without inventing
a candidate. Historical receipts are not backfilled, and no Creator is saved.

Creator schema 43 adds the receipt table using the adjacent v42 -> v43 upgrade.
This is source-level integration, not a released installer or customer acceptance.
Content relevance, authentic AI-tool demonstrations, independent tool-link checks
and final qualification remain separate requirements.


## Substack, Nowcoder and Xiaohongshu search repairs (0.6.30)

`onecrew-substack/search` returns bounded public search posts, including the
platform post/publication IDs and canonical custom-domain source. `onecrew-nowcoder/search`
and `onecrew-xiaohongshu/search` are registered by the `onecrew-source-search`
bundle and read actual rendered cards in one owned browser tab. They retain
post/author source bindings and return explicit login, risk and hydration errors.
They do not replace upstream commands or infer qualification from search hits.

`OpenCli::creator_search` selects these fixed commands and verifies exact bundle
bytes before and after a read. Installation/repair uses the existing bundled
addon lifecycle. Google Images keeps its upstream adapter with `--resolve=false`:
Campaigns need publishing-page URLs, not original-image downloads.

Run `node --test integrations/opencli/source-search.test.mjs` and the
`onecrew-opencli` Rust tests. Verify live behavior against the target platform's current pages and login state.
