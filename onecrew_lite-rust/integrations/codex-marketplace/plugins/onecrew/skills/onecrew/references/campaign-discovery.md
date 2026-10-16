# Persistent KOL Campaign discovery

Use this route when the user gives one business goal and expects a saved,
cross-platform KOL shortlist. The Campaign approval is this route's search
authorization; do not also prepare a standalone `onecrew.search.plan.prepare`
for its provider reads. Keep the original sentence as the Campaign brief.

1. Check `onecrew_status`. Use exact live contracts from `onecrew_tool_get`
   for `onecrew.discovery.sources.get`, `onecrew.discovery.intent.validate`,
   `onecrew.discovery.campaign.create`,
   `onecrew.discovery.campaign.approve`, `onecrew.discovery.campaign.get`,
   `onecrew.discovery.advance`, and `onecrew.discovery.candidates.count`.
   Discover any other operation via `onecrew_tool_search`; do not invent
   arguments, a schema revision, connection tags, or a result reference.
2. Execute `onecrew.discovery.sources.get` with its current contract. Its
   `sources` are local search registrations, **not** proof of provider login
   or usable results. Default to every `registered: true` entry, using an
   actual returned profile and version. AI chooses keywords and search order;
   narrow coverage only when the customer explicitly limits or excludes
   platforms. Include search-engine routes and scoped directories. Show the source count and
   identify unavailable profiles or bridge before proposing a draft.
3. Parse the customer's sentence into a proposal, not an approved standard.
   Keep each hard constraint with the exact supporting phrase: geography,
   content language, topic, follower threshold, exclusions, forbidden
   sources, quantity and deadline. Unstated criteria stay unset. For a new task,
   derive conditions from that task's request. Never add a product demo or link requirement by default, or
   equate an audience location with the creator's residence. Keep soft
   preferences and search tactics separate. When wording is ambiguous, ask
   only for the decision needed to make a hard condition executable.
   Submit the exact sentence and typed proposal to
   `onecrew.discovery.intent.validate` before drafting. Resolve or show every
   validation issue; a valid response is a read-only preflight, not customer
   approval or proof that the interpretation is semantically correct. The
   customer still reviews the proposed hard conditions before creation.
   Persist quoted additional conditions in `criteria.requirements` as
   `{description, excerpt}` items. Each item requires its own source-bound
   review; missing evidence remains pending. Treat a public email as a default
   ranking preference, `criteria.contact_preference="public_email_first"`,
   rather than a qualification gate. Prioritize observed profile emails and
   bounded author-published contact pages; expand Telegram only when requested.

   For conversational improvements, read the exact current Campaign first.
   Identify the changed goal, hard conditions, preferences and target; retain
   every unchanged customer requirement. Build a consolidated draft and attach
   `criteria.refinement={campaign_id, expected_revision}` for its approved
   source Campaign. The workbench AI planning input exposes the same refinement
   reference. Show the changed conditions and obtain workbench approval.
   Approval reuses saved search hints only where platform, browser profile and
   exact keyword remain approved; native evidence and qualifications are checked
   again. Keep the original Campaign and its historical result table intact.
   An excluded account needs a verified native ID and exact profile URL:
   `criteria.excluded_accounts` items contain `native_author_id`, `profile_url`
   and the customer's exact `excerpt`. Resolve the actual account from saved
   candidates; a display-name resemblance is insufficient. Approved exclusions
   take precedence over a match score and remove that account from the new
   recommendation set. A partnership exclusion remains a separate evidence
   requirement; absence from one sampled post cannot prove an all-time absence.
4. Form a bounded scope for every registered entry. Use each source's
   `scope_kind` and `allowed_values`; category sources require one of the
   returned values. Generate automatic queries for the actual search entry:
   video entries (YouTube, TikTok, Bilibili, Douyin) need precise product/model
   and hands-on, comparison or testing angles; X and Reddit need post intent,
   where requests, giveaways and review announcements are not themselves
   experience evidence. Instagram and older Pinterest search-users entries search users, and
   LinkedIn searches people: use specific product niches and creator roles,
   never claim these entries search captions, pins or posts. For Pinterest
   search-pins, use precise product/model and content-intent queries; preserve
   the Pin source separately from the native pinner, without claiming image
   originality. Follow the actual registered command, not the platform name alone. Facebook currently
   searches mixed top results, including groups; keywords cannot guarantee
   posts-only results. Google/Yahoo return webpages, not verified creators.
   Keep the product category explicit: broad terms such as mouse or hardware
   reviewer can retrieve unrelated meanings. Choose complementary intent
   variants, not repeated generic synonyms. Use short natural phrases by
   default, and only syntax supported by the specific adapter; X supports
   quoted phrases, but do not copy operators to account/video entries.
   Reddit post search can match only some unquoted words. Anchor the precise
   category/model in quotes and join an evidence angle with uppercase AND;
   group alternatives, such as "gaming mouse" AND (review OR comparison).
   A complementary title-focused query can use title:"gaming mouse" AND review.
   Substitute the customer's actual topic; do not add subreddit restrictions.
   Reddit documents these forms in [Available search features](https://support.reddithelp.com/hc/en-us/articles/19696541895316-Available-search-features).
   Confirm their behavior through the registered adapter with approved real
   searches; syntax support alone does not prove relevant results.
   Preserve brands/models and explicitly requested content language. The
   conversation language alone is not a content-language condition. With
   unspecified language, choose useful source/topic languages without adding
   a hard constraint. Do not add a year or silently expand the product topic.
   Validate strategies with saved real result evidence; account names, groups,
   question posts and technical stops must not count as proven relevant content.
   Generate these terms before approval; never rewrite frozen queries or add
   reads to compensate for weak results. Free-text queries,
   synonyms, tags, categories, communities and nodes are search tactics, not
   changes to hard criteria. Respect the live operation's keyword limits and
   scoped values; if a required scope cannot be made valid, show that gap
   before creation. Never silently drop an entry to make a draft pass. Set
   `max_calls=null` for a new unlimited plan only when the live contract supports
   it; null is not zero or a large numeric substitute. Preserve historical
   finite approvals without rewriting them. Present the
   original sentence, all hard constraints, all scope entries, profile,
   target and unlimited/finite read authorization for customer review. Create the draft only with
   exact typed arguments; creation performs no provider read.
5. Approval freezes criteria and scopes. Have the customer review and approve
   in the workbench; read the persisted owner outcome before advancing.
   Codex cards show stages and exceptions only, never goal forms or approval buttons.
   A user request to search all
   sources authorizes the source choice, not invented hard requirements.
   After approval, call `onecrew.discovery.advance` in bounded rounds,
   reading its result and the saved Campaign checkpoint before continuing.
   The owner completes each observed author through verification, scoring,
   source-backed contacts and recommendation before starting another author
   or source search; reaching the approved qualification target stops further reads. Contacts are
   required only by an explicit customer condition or a historical frozen policy. Do not blindly retry an
   unknown-effect or interrupted call. A login, risk or technical stop is
   not a zero-result source. For search-only `search_once` rounds, inspect the
   structured outcome after every call, before issuing another keyword.
   Any failed, interrupted or unknown outcome, including
   `discovery_provider_read_stopped`, requires Campaign and status readback.
   Do not rely on the literal text `technical_stop` alone: the immediate
   failure envelope and persisted platform stop reason can differ. Preserve
   the consumed call and failure in the evidence; stop that platform without
   retrying or treating it as empty. Continue other approved platforms only
   after checking that their connection and admission remain usable.
   After a definite technical defect has been repaired, an explicitly requested
   continuation may use `search_once.recovery` with the exact saved
   `expected_stop_code` and `expected_stopped_at`. Read the current Campaign
   first and fetch the live operation contract. The owner atomically checks
   that checkpoint and admits one next approved query. Historical stops that
   did not retain their failed query resume from the first approved query
   without a saved successful completion; retain all previous charged calls.
   Unchanged scope,
   revision, connection and budget remain required. Never use repair recovery
   for login, risk, timeout, interrupted or unknown-effect outcomes. A changed
   checkpoint requires readback, never a blind retry.
   When the live output contract exposes `search_claim` and
   `returned_sources`, save the exact claim and ordered sources from that
   call for relevance comparisons. An array position is the returned order,
   not a new quality score. `returned_hits` is the pre-deduplication count;
   `hits.list` is a deduplicated source inventory, not the original ranking
   or a complete record of every query that returned the same source.
   Older contracts without ordered sources must report that evidence gap;
   never reconstruct ranks from source IDs or make extra reads to fill it.
6. Read candidate counts and source-bound evidence through the exact current
   operations. Search hits are leads; only owner-qualified, deduplicated
   recommended creators with passing audited scores count toward the target.
   Report observed public emails separately; a profile URL or hidden-email control
   is not an email address. Verify native author, content, follower and
   required location evidence. Missing or conflicting evidence remains
   pending. Never loosen a threshold, category, location or excluded source
   to reach a target. A changed hard condition needs a new customer decision
   and new approved plan. Ranking and keyword order may adapt within the
   approved scope and budget; record why. Stop at the target, budget, no
   further approved action or required task-level decision. A null read limit
   never causes budget exhaustion. Unqualified, missing-contact or uncertain
   candidates stay in the backend without a manual language approval queue. For
   `criteria.requirements`, submit exactly one current-material finding for each
   index using the live review contract's requirement criterion. Supported means
   the entire condition is satisfied; contradicted requires positive conflicting
   evidence; insufficient preserves an unresolved condition. Keep every citation
   tied to that author and material hash. A `needs_evidence` result requires reading
   these specific gaps; repeated Continue clicks do not create new evidence.
   Report qualified,
   pending, rejected, per-source stop reasons and remaining gap honestly.

The local UI and Library can show a Campaign and its one-author-one-row
view. A Campaign does not send outreach or automatically turn a lead into a
confirmed Creator record.

## Initial matching and shortlist

Use the rubric in [match-evaluation.md](match-evaluation.md), pinned to upstream
onecrew-plugins commit 0cebf1220c15f4252533e6b4a6bddb079cfdc101. The workbench
now scores automatically after saved author material becomes available; do not
ask customers to fill the legacy review checklist or copy a scoring prompt.

For an authorized MCP-run search, use the host model and the existing typed
review material / submit operations. Never call the workbench's native Codex
bridge from inside an MCP model turn. Read saved material and, when necessary,
the exact candidate's owner-derived follower count. Submit exactly one
match_fit finding with reviewer.kind=agent and reviewer.label=
"onecrew-match-evaluation-0cebf122-initial-v1". Use the returned material hash,
an idempotent request ID and exact supported citations. Rated outcomes are
supported with excellent/good/medium/low; unknown is insufficient with a null
match_level. The owner audits and saves the score; the new initial-match policy
replaces the legacy proof checklist for these scored candidates.

For new contact-backed plans, excellent, good or medium is necessary but not
sufficient: all required conditions must also pass and a real email or Telegram
contact must have been observed in saved author-bound original material or a
successfully read associated webpage. No contact means no recommendation or
main-table row. Contacts with uncertain ownership/purpose retain that uncertainty
in the backend; never invent a locator, use audience comments as contact sources,
or describe an unknown Telegram destination as a personal account. Contacts are
not permission to reach out. Read the owner's final deduplicated count, not the
raw scored-candidate verdict. Legacy approvals and old tables remain preserved.
Low and unrated remain in source/history views. Preserve non-null scores on ordinary repeated discovery;
an explicit reassessment replaces level and reason together. Never count a
missing decisive fact as a low grade, invent weights, force a target by retries,
or claim an initial sample covers all posting history. Confirm owner readback
and synchronize the existing Campaign table; do not create a replacement table.

To expand an existing frozen scope, use the workbench's copy-to-all-platforms
action. Show the new source list, copied criteria, target and independent read
authorization there. Generate a separate draft and wait for workbench confirmation
before searching; keep the original Campaign and its progress intact.

To raise an existing target, use `onecrew.discovery.target.increase` with the
current revision and target, the customer's new target, and one stable
`decision_id`. Its attended confirmation changes no criteria, scope or read
budget. After an uncertain result, read the Campaign first and replay the exact
same request and decision ID if needed. Account counts alone do not prove
cross-platform creator uniqueness. A new product/category requires a separate
approved Campaign; never rewrite an existing frozen category.
