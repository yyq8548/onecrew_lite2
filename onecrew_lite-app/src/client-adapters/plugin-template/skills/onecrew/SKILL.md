---
name: onecrew
description: Use live OneCrew MCP for the user's connected account reads, public creator research, saved Creators, evidence, local mail history and Library results. Use current typed operations and server-issued reviews. Source-code development, plugin installation and unavailable connections use their own tools.
---

# OneCrew

Read `onecrew_status` before account-dependent work. Use `onecrew_login`,
`onecrew_login_cancel` and `onecrew_logout` only for the user's requested
authentication action. Open the returned `authorize_url` for the user; keep
credentials and callback secrets inside the authorization owner.

Call `onecrew_web` to mount the read-only status card and obtain workbench
`url` and `library_url`. Open both verified URLs in Codex's in-app browser,
reusing tabs. Show the right browser panel with `visibility.set(true)`, select
each destination tab and read its rendered content. Finish with the workbench
tab selected and keep the matching results table tab; use Library when no plan
exists. `markDeliverable()` preserves a tab but does not show the panel or select
that tab. Report displayed only after checking panel visibility, the selected
tab and rendered content. Report missing tools, queued requests and read failures
as incomplete. Respect explicit no-browser, no-card or text-only requests.
Report `library_error`, stale data and missing capabilities accurately.
Use a server-returned table URL for the current Campaign; table identifiers
and account readiness require current evidence.

## Typed operations

1. Read actual connection references with `onecrew_connection_list` when
   selecting an account, mailbox or service credential. An unavailable
   inventory is a technical stop. Operator accounts and browser identities
   have separate owners.
2. Discover an unknown capability with one concise English
   `onecrew_tool_search` query and explicit toolkit filters. Keep business
   keywords and identifiers in operation arguments. Use returned toolkit
   guidance and workflow hints as guidance, then obtain the exact contract.
3. Send 1–8 unique operation IDs to `onecrew_tool_get`. Use each current
   `schema_revision`, input schema, connection requirement, effect and
   approval class exactly. A missing operation remains unavailable.
4. Execute 1–8 independent calls with unique `call_id`, exact operation ID,
   revision, `authority={"kind":"direct"}`, tagged connection or `null`, and
   an arguments object. Dependent calls require the previous typed output.
   Use `presentation="never"` for background reads, `"if_required"` for
   ordinary work and `"requested"` when the user requests the result.
5. Present each returned `result_ref` once with `onecrew_result_present`,
   passing `locale="zh-CN"` for Chinese conversations or `"en-US"` otherwise.
   Wait for the actual user review. App-only state tools, bindings, revisions
   and continuation leases belong to the App and service; the agent never
   constructs or replays them.
6. Continue from the completed typed outcome. On input-required, failure,
   unknown effect, skipped calls, connection errors or revision changes,
   follow the returned recovery contract. Preserve one idempotency key per
   intent and inspect existing receipts before retrying an uncertain write.

## Research and saving

Public creator and content searches require the current editable
`onecrew.search.plan.prepare` review by default. Execute confirmed scopes
unchanged using their real `confirmed_plan_ref`. Only the user's explicit
current direct-search request can select the documented direct policy.

For persistent Campaign discovery, retrieve the live workflow guide and use
the user's actual criteria, target and budget. Keep qualified, rejected and
unverified candidates distinct; a technical stop differs from a valid zero
result. Reuse saved evidence and current cursors. Stop on the owner's stated
budget or completion boundary. Read current status at progress checkpoints.

Save quoted additional conditions in `criteria.requirements` as
`{description, excerpt}`. Review each index with `criterion={requirement:index}`
and exact citations from the current owner material. Missing evidence stays
pending; a good match score cannot override it. Silence in a sample cannot
prove absence of a historical partnership or a private fact.
Use `contact_preference="public_email_first"` for the default soft ranking:
missing email does not reject an otherwise qualified creator. Report actual
public emails separately from profile links and hidden-email controls.
For task revisions, read the source Campaign and preserve unchanged customer
conditions. Bind `criteria.refinement={campaign_id, expected_revision}` and
obtain approval of the consolidated draft. Old qualifications are rechecked.
Native account exclusions require the exact `native_author_id`, `profile_url`
and customer `excerpt` in `criteria.excluded_accounts`.
For a repaired technical search, use only the current operation's recovery
checkpoint with the exact stop code and timestamp. Never bypass an auth/risk
stop, repeat an uncertain read automatically, or increase the approved budget.

Prepare Creator candidate saves only when requested. Use real source-bound
provenance, selected connections and the current row/route review. Preparation
does not create a Creator; only the user's reviewed selections can write.
Use the actual saved result and receipt to report completion.

## Mail and capability boundaries

MCP can read retained mail history and perform only current registered local
maintenance. Gmail bulk sending belongs exclusively to the Mail page's
GmailBulk flow and its real user confirmation. MCP provides no sending
operation. Social private messages, replies, following, social writing and
old mail sending workflows are retired; historical records stay readable.

Use live OneCrew results for business facts. A cached skill or dependency
package never expands the available product surface. Plugin setup, update
and removal require the explicit CLI ClientAdapter workflow; routine MCP
startup and status never install or update Codex configuration.
