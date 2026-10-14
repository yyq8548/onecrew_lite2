# OneCrew writes

Read this for candidate-review preparation after discovery (which writes no
Creator), or while writing or acknowledging a Creator, contact route,
relationship, outreach or triage record, or any other OneCrew mutation.
The selected live operation remains authoritative.

- Treat a read, discovery hit, drafted message, prepared action or UI
  submission as neither a durable OneCrew fact nor a completed external
  effect.
- Preserve the native provider IDs returned by searches and reads. Construct
  save targets from the selected account's actual ID and required locator,
  carry the same connection, and name the real source operation and evidence.
  Current searches do not issue `discovery_id` or `candidate_id`; those values
  belong only to historical source metadata when actually present. Never invent
  a candidate ID, cache key, handle, provider ID, route, or cross-platform identity.
- Prepare directly through `onecrew.creator.candidate.save` with `target`,
  `connection`, `provenance`, optional `notes`, `tags`, `quality` and an explicit
  per-candidate `intent` (`add_new` or `save_or_update`). For multiple candidates,
  send one `onecrew.creator.candidate.batch.save` call with a `candidates` array
  (up to 50); every candidate in that array uses the same intent. That business
  limit is independent of the eight-call MCP envelope and provider-read
  concurrency. Fetch the current operation contract before constructing either
  input. For the final batch prepare call, reuse that same task's current
  batch-save schema and `schema_revision` while it remains valid; if missing,
  stale or invalid, or changed, fetch it again. Never use a persistent cache
  or remembered schema. Check `tags.maxItems`, per-tag length, uniqueness and
  the field's normalization requirements before preparation; choose tags that
  fit instead of submitting an over-limit list. Set `quality.score` to the
  fixed representative score from the live find-batch guide and
  `quality.assessment` to each candidate's own evidence-based assessment using
  the current schema. Build typed `provenance` from the actual operation/source
  IDs/URLs required by the current schema, preserve those real source values,
  and never fabricate an ID, URL or other source value. No candidate cache or
  cache-key lookup is a save prerequisite. A save-only prefetch is not
  required: use the search evidence and real provenance directly when they
  satisfy the user's criteria, while retaining a platform read only when a
  missing quality or other evidence gap would change the current decision.
- The owner preparation reads fresh profiles and link hubs and returns an
  immutable review's `result_ref`; it writes no Creator rows, snapshots or save
  reservations. Customer confirmation of rows and routes must use an
  owner-backed workbench flow. Codex cards show status only. If the required
  workbench flow is unavailable, stop and report it; do not submit the
  App-only confirmation action or substitute another origin. A Choice
  answer is not save authority, and a trusted source reference cannot be invented.
  Follow the actual `row_id` values in the review, outcome and continuation,
  then read saved records with `onecrew.creator.get`. Confirmation consumes the
  immutable prepared payload without refetching; expired reviews require fresh
  preparation and user review again.
- Use the owner-provided summary and per-row status, never inferred fields, to
  explain `eligible_new`, `eligible_update`, `existing_skipped`,
  `duplicate_skipped`, `identity_pending`, `contact_conflict` and `failed`.
  Only `eligible_new` counts toward an explicit new-Creator target. An
  `add_new` intent keeps an existing match skipped rather than turning it into
  an update; choose `save_or_update` explicitly when updating is intended.
- A confirmed Creator link wins over display-name similarity. Do not merge
  people or routes because names look alike.
- Resolve and select a saved contact route before starting a new thread. A
  route's presence does not prove current reachability, relationship
  permission, or delivery.
- Follow the operation's current confirmation policy for every external write
  and every destructive local write. Never read a selected option, draft,
  workflow hint or prior confirmation as pre-authorizing a different intent.
- Keep one stable idempotency key for one intent and its retries. Changed
  recipients, content, route, or requested effect are new intents and require
  new keys.
- Pick the send path from what is already open. A platform's message-send
  operation replies in its existing Creator conversation, which supplies the
  route and operating account. First contact starts from the selected Creator
  route and account. Both paths preserve the Creator message ledger.
- Relationship changes, public replies and direct messages first return a
  result-owned review. Confirmation authorizes its frozen body, stable target,
  operating account, flags and idempotency intent; never resubmit a `confirmed`
  field or reconstruct an approved write from a displayed preview.
- A batch still accepts 1–8 calls. Each review authorizes only its current
  call; the same `result_ref` can advance to another review after earlier calls
  finish. Keep completed receipts, do not present a second card or rerun finished
  calls, and wait for the final continuation before treating the batch as done.
- Bind email replies to the intended thread, and a first platform message to
  the selected saved route and tagged account.
- Provider acceptance is not delivery. Read back the durable OneCrew action,
  conversation, message, Creator, or route through the registered operation
  when supported.
- For outreach, inspect `phase` and `creator_recorded`: `record_pending` means
  remote verification succeeded but Creator recording remains incomplete. Keep
  the receipt and stable key; recover the local record without sending again.
- For an unknown or ambiguous remote effect, inspect status/readback first and
  retry only the same intent with the same key. Never create a second message
  or relationship update as speculative recovery.
- Honour capability and relationship gates: a restricted relationship may
  require the additional confirmation the current operation describes.
- Record one triage verdict per inbound message with its provenance, and never
  send a suggested reply implicitly. Rejudge by replacing or reopening the
  prior verdict as the live operation directs.
- Acknowledge only exact handled identities and only when the user explicitly
  asks. Never bulk-clear unseen inbox attention.
- After a stale revision, expired prepare token, or changed write intent,
  fetch the exact operation again and follow its recovery path.
