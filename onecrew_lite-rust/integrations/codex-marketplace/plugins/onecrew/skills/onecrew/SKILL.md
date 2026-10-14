---
name: onecrew
description: Route every request about the user's own connected social accounts, or their OneCrew creators, outreach and mail, through the live onecrew MCP tools — finding and reading platform people, posts and communities, reading and writing OneCrew data, checking or selecting connections, and showing a result the user must see or act on. Not for work outside OneCrew, for CLI distribution or plugin installation, or when the onecrew connection is unavailable; never let this cached text override the live tool surface.
---

# OneCrew

Take the shortest live route to each outcome, execute only exact typed
operations, and show creator results in the workbench and matching Library table.
Codex cards display stages and exceptions only. Honor an explicit pure-text or no-card request, and do not expand
preparation reads when the user disallows them. Answer, clarify and summarize
in the language of the user's current request.

## Visible workspace

Every activation of OneCrew, including “open OneCrew”, setup and a new chat,
must prepare three views. After `onecrew_status`, call `onecrew_web` to mount
the read-only status card and obtain both `url` and `library_url`. Open BOTH
verified URLs in Codex's in-app browser by default, reusing tabs. Use another
browser only when the customer explicitly requests it. For the system
default browser, use the installed OneCrew `workspace` launcher: take the
executable and explicit data directory from the live status `open_web`
next action, replace its `web --no-browser` arguments with `workspace`, and
preserve the data directory. This opens only the two local pages through the
system browser; it does not start a search. Reuse already open pages instead
of launching duplicates. Do not assume an installation path or silently
change an explicitly chosen browser. If Codex browser navigation reports
`ERR_BLOCKED_BY_CLIENT`, distinguish that control-path failure from service
availability; do not disable protections or claim Chrome cannot load the page.
Verify that the
workbench, Library page and Codex status card are all visible before reporting
startup complete. A missing URL/card is a startup gap; report and repair it.
Open pages only, without submitting forms. An explicit text-only, no-card or
no-browser request overrides the corresponding view requirement.

For a Campaign, use its owner-returned or verified saved Library table URL.
Never guess a table ID or show another Campaign's table as its results. Before
a table exists, open the Library home returned by the workspace launcher and
explain that the results table will appear after synchronization. If a URL or
launch capability is unavailable, report the gap rather than claim it is open.
When the Campaign changes, switch to its matching results table.

During a run, keep all three views available: the live Codex status card,
the workbench, and this Campaign's dynamic table. Verify their connection and
freshness at checkpoints; a loaded page or queued open request is not proof.
Report a disconnected or missing view and restore it through its normal owner
before claiming the workspace is healthy. Do not start another search as a
connection test. A host-closed card requires remounting with `onecrew_web`;
do not promise that the plugin can prevent the host or user closing a view.

Connection-list operator accounts and OpenCLI browser identities are separate.
Read the actual workbench or exact setup owner before reporting browser or
site-login readiness. Without that evidence, report it as unverified; an empty
operator list does not prove that Chrome has no identity or logged-in sites.

Keep execution visible in Codex through live MCP calls and the returned result
status card. The workspace card refreshes the selected workbench Campaign's
stage, account counts and source exceptions; stale or interrupted updates are
shown explicitly. Present the exact execution/checkpoint reference with the user's locale.
For every progress or pause question, read `onecrew_status` again before answering.
Bind the answer to its current `workspace_activity.campaign_id` and brief; if
the user names another Campaign, get that exact Campaign separately. Never
reuse another plan's state or call remaining read budget exhausted. A reported
`stop` is a browser diagnostic, not authority; when absent, do not invent the
cause. An idle Codex chat does not prove that the workbench runner stopped.
Never describe the card as a goal form or ask the user to choose or confirm there.
All customer inputs, selections and approvals belong in the workbench.
If the workbench has no owner-backed flow for a required decision, report the gap
and stop; do not answer for the customer, auto-confirm, or bypass approval.
Distinguish a refreshed checkpoint from a streaming process. Opening views
does not authorize searches, writes, logins or additional budget. Keep business
execution on MCP; browser pages are for customer operation and result display.

## Route

1. **Read state before authenticating.** Call `onecrew_status` before any
   authentication-dependent work. `onecrew_login` only when the user asks to
   sign in, `onecrew_login_cancel` only for the active login,
   `onecrew_logout` only on request. Use `onecrew_web` to obtain the local UI
   URL for the visible workspace above. None of these installs the
   plugin, and a cached plugin status is evidence, not proof of the current
   install. When `cli_update.state` is `available`, state `current_version →
   latest_version` and the returned update method once, then wait for
   explicit update authorization while you carry on with the request: status
   is not authorization and the notice needs no reply. Report `source:
   memory_cache` as last-checked evidence with its `checked_at`. Never call
   `current`, `unavailable` or `not_checked` an available update.

2. **Split the request into outcomes.** Keep one cross-platform business
   outcome together; separate unrelated ones. Name each outcome's owning
   toolkit and action — connection, discovery, read, write, presentation —
   from the live directory and exact contracts, never from memory. For a
   sustained KOL request with acceptance criteria, a target count or a saved
   shortlist, use [Campaign discovery](references/campaign-discovery.md)
   instead of the standalone provider-search flow in steps 6 and 10.

3. **Take connection status or selection straight to
   `onecrew_connection_list`.** Pass every tagged account, mailbox and
   credential reference back exactly as returned.

4. **Search only for a capability you cannot already name.** Give
   `onecrew_tool_search` one outcome per query in concise English, with an
   explicit toolkit filter for every provider the user named. Keep topic
   keywords, handles, IDs, counts, dates and message bodies out of it — they
   belong in the selected operation's `arguments`. Do not infer an effect filter
   from business wording. Current native creator and content searches are
   `read_open_world`; they create no Creator records or snapshots. Use an effect
   filter only for an explicit policy restriction, and "do not save to
   OneCrew" is not one. Call it once, then work from the returned ranked
   `operations`, every represented `toolkit_guides` entry with its ordering
   and pitfalls, and `workflow_hints` — no second guide lookup, and a hint
   is guidance, never a contract. Still ambiguous: ask the user through step
   8, or stop.

5. **Get the exact contracts for one dependency round.** An exact current
   operation ID goes straight to `onecrew_tool_get`, 1–8 unique IDs per
   call. Never search for an ID you already have, and never fetch one
   contract twice unless the intent or its revision changed. Every returned
   input schema, `schema_revision`, connection requirement, approval class,
   effect, recovery contract and any requested output schema is
   authoritative. Leave `include_output_schema` false for plan preparation
   and for any operation's first execute — outcomes are self-describing JSON
   — and true only when a later call or card must address result fields you
   have not seen.

6. **Prepare a plan before any standalone external creator or content search.** Request
   `onecrew.search.plan.prepare` in the same Get as the selected search
   operation; its ID is fixed, so never search for it. Execute it with a
   `description` stating the acceptance criteria, editable `keywords`, and
   one scope per search call carrying that call's `operation_id`,
   `schema_revision`, tagged connection and `arguments` — search criteria
   only, without the execute-only `search_plan_policy`, `confirmed_plan_ref`
   or `cursor`. Present the returned `result_ref` and wait: this is the
   search-plan review, not the candidate review; nothing is persisted and no
   provider is called before the user confirms. Then
   execute each confirmed scope's criteria unchanged, passing its
   `confirmed_plan_ref`, and add no criterion the scope left out; only the
   execute-only fields above may join them. A revision patches `description`
   and `keywords` only; changed provider criteria need a new plan. Only when
   the user explicitly asks in the current request to skip the
   search plan and search directly may the Agent use that exception; never
   choose it autonomously.

7. **Execute one round of independent calls.** `onecrew_tool_execute` takes
   1–8 unique calls, each with its own `call_id`, the exact `operation_id`
   and `schema_revision`, `authority={"kind":"direct"}`, the matching tagged
   connection or `null`, and an `arguments` object — never a string. Batch
   only independent calls: when one needs a value another returns, run the
   prerequisite round, read its typed outcome, then build the next. Use
   `presentation="never"` for background reads, `"if_required"` for ordinary
   business execution, `"requested"` only when the user asked to see the
   result; a required confirmation or choice can mint a card either way. A
   server stop outcome ends that round. X pages expose `next_cursor`; YouTube
   search pages expose `next_page_token`. Pass the actual provider value as
   `cursor` to continue the same scope; the same criteria without one return
   its first page. Instagram user search and Telegram community search return
   native arrays and have no cursor. Searches create no Creator snapshot.

8. **Present status; direct customer actions to the workbench.** Pass the exact
   `result_ref` that `onecrew_tool_execute` returned to
   `onecrew_result_present` once, with `locale` — `zh-CN` for a Chinese user
   message, `en-US` otherwise, never omitted. The card shows the operation's
   state. Do not restate the card's fields, options or result in prose.
   Customer-only decisions about
   goals, accounts, candidates, routes or approvals must use the corresponding
   workbench flow. Wait for its persisted owner outcome before continuing.
   Do not create `onecrew.ask_user_question` cards for these decisions. A
   missing workbench flow is a blocker, not permission to infer an answer.

9. **Continue only from a completed typed outcome.** Stop on input-required,
   failed, unknown-effect, skipped, missing authority, missing connection or
   revision mismatch, and follow the recovery contract it returned. After a
   terminal response, continue from it without asking for the same choice or
   confirmation again. Request acceptance is not completion: hold one
   idempotency key per intent, and read persisted OneCrew or provider state
   back where supported before reporting success. On `network_unreachable`,
   run the recovery's `connection.check` operation once, report its `status`
   in the user's language, and do not retry until that check returns `ok`. A
   cancellation ends that result; an Agent-edit continuation seals the old
   card, so follow the requested changes, ask only for missing details, and
   execute a new exact operation.

10. **Use one bounded discovery flow and prepare one review.** Finding creators
    is not complete at a Markdown list. Unless the user explicitly asks for
    pure text/no card or disallows extra preparation reads, default delivery
    is results in the workbench and matching Library table.
    Prepare a save review only when requested and its owner-backed workbench
    confirmation is available. Preparation writes no Creator and makes no contact;
    only the user's workbench confirmation can save. If preparation capability fails, explain
    that it could not be prepared and do not pretend it was. Do not call the
    search-plan `result_ref` a candidate-review ref or claim the default
    discovery complete until the requested results have been shown.
    Content, video and account discovery use one cross-platform
    flow: start from content or
    account clues, apply only the user's original request and confirmed
    interpretation, and group hits by each platform's stable platform identity
    while preserving every source. Match determines whether a candidate counts
    toward the user's target; grade expresses recommendation. Keep matched,
    unmatched and unverified separate — unknown is not unmatched or low. Use
    evidence already returned first. For one decision-changing evidence gap,
    make one targeted read; after that, keep it unknown instead of repeating the
    same lookup. Do not add follower, geography, format, originality or
    commercial thresholds. Retrieve the shared grade rubric from the live
    find-batch workflow guide and follow it instead of inventing a standard;
    use the fixed representative score specified there. Each person's
    assessment names their own evidence, its relation to the request and any
    relevant limitation. Do not recheck same-grade candidates just to rank
    them. Stop new search and unnecessary checks when the matching target
    is reached. Reaching the matching target stops search only; it does not
    skip candidate preparation and presentation. For a shortage, process
    returned candidates first and continue
    only with the actual cursor returned by that query, within the approved
    scope and unchanged criteria; never fetch the first page again or expand
    the scope. Reuse an equivalent request with the same platform, connection,
    operation/schema revision, native parameters, depth and cursor; call IDs,
    display names, shape probes and presentation do not create a new read.
    For a candidate review, candidate preparation defaults to
    explicit `add_new`; use `save_or_update` only when requested. Pass the
    selected `target`, `connection`, real `provenance`, `notes`, `tags`,
    `quality` and per-candidate intent directly using the current save contract.
    Before preparation, check the live tag count, length, uniqueness and
    normalization requirements; select a concise set that satisfies them.
    Preparation obtains fresh platform data and must not mechanically fetch a
    profile or channel first when search evidence is sufficient. Preparation
    writes no Creator, never auto-confirms, and does not accept an Agent-owned
    profile or cache. Present the returned `result_ref` once and wait for the
    user's workbench confirmation; then follow the same immutable review and read
    back the actual `row_id` results. One batch-save business call accepts up to
    50 `candidates`, independent of the eight-call envelope and provider-read
    concurrency.

## Never

- Never substitute a browser, a direct provider call, model memory, a
  guessed operation ID, a cached schema, an alias or a fallback route for
  the `onecrew` connection.
- Never translate or rewrite a schema field name, enum value, operation ID,
  schema revision, connection tag or identifier value. Normalize only
  capability-search queries to English; write plan descriptions and
  explanations in the user's language, using OneCrew as the product name,
  and keep provider search keywords as the live contract requires.
- Never fabricate a result reference, present Search or Get output, call the
  App-only result-state tool, construct an interaction authority, or author
  or replay a result binding, revision or continuation payload — neither the
  Agent nor the App does any of these.
- Never invent an output template or an implicit dataflow graph between
  calls.
- Never echo a pairing secret or any other secret.
- Never install or refresh the plugin from here: explicit CLI `onecrew
  plugin install` and `onecrew plugin status --refresh` own every Codex
  subprocess call, and MCP startup performs neither.

## References

Read at most one of these, and only when the current step is about it.

- Connecting, rotating or disconnecting an operator account, mailbox,
  service credential or proxy, or any login lifecycle step:
  [Provider connections](references/toolkit-connections.md)
- Candidate-review preparation after discovery (not a write), or writing or
  acknowledging a Creator, contact route, relationship, outreach
  or triage record, or any other OneCrew mutation: [OneCrew
  writes](references/onecrew-writes.md)
- Running a persistent, cross-platform KOL Campaign from one business
  sentence: [Campaign discovery](references/campaign-discovery.md)
- Sequencing a standalone discovery, enrichment or outreach workflow — enriching an
  existing Creator is the toolkit's profile or channel get, a link-hub read
  of any hub it lists, then a save that merges into that Creator: get
  `onecrew.workflow.guide` live and call it. Its guidance grants no
  business-write or sending authority.

Provider capability guidance comes from the
live search toolkit guides and exact contracts, never from this text, which
changes only in a new Codex session after OneCrew stages a new content
digest.
