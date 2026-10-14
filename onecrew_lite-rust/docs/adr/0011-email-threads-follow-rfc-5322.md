# ADR 0011: Email threads follow RFC 5322, not the recipient

## Status

Accepted.

## Context

Outbound mail joined a thread by looking one up on `(mailbox, creator,
contact route)` and taking the most recently updated row. A shared recipient
was, in effect, the thread key.

That is not how mail threads work, and the gap surfaced as a defect a user hit
directly: they wrote a fresh email to someone they had mailed weeks earlier,
and it was filed under the earlier thread — which they had deleted. The new
message landed in the bin, and the Sent folder read zero, because folder state
is a property of the thread and the thread was in the bin.

RFC 5322 threads on `Message-ID`, `In-Reply-To` and `References`, with a
normalised subject as the fallback heuristic when those headers are missing.
Every mail client and every threading implementation in the JWZ lineage works
from those. A shared recipient has never been a criterion in any of them.

Inbound mail was already correct: the sync path resolves a thread through
`find_email_thread_by_internet_message_ids`, walking the reference chain. Only
the send path had invented its own rule.

## Decision

A send joins a thread when, and only when, the caller names the message it is
answering. `reply_to_email_id` present means the reply belongs to that
message's thread; absent means this is a new outreach and gets a thread of its
own.

The thread key carries the idempotency key for keyless sends
(`creator:<id>:route:<id>:send:<key>`), so a retry of one intent resolves back
to its own thread while a genuinely new intent opens a new one. The key alone
decides this: leaving it stable while returning no thread from the lookup
still merges the two, because the snapshot upsert finds the row by key.

Quoting rides the same signal. A reply carries the parent's body — as `>`
prefixed text in `text/plain` and as a `div.gmail_quote` + `blockquote`
structure in `text/html` — so the recipient's client can fold the history that
the threading headers alone would never make visible.

Deliberately not adopted: subject-based fallback on the send path. It is a
heuristic for reconstructing threads at the receiving end from messages whose
headers are missing, and it is known to over-merge. On the sending side the
caller's intent is available directly, and "the caller did not say this was a
reply" is a better answer than a guess from a matching subject line.

## Consequences

- A second outreach to the same person starts a separate thread even when the
  subject is identical. This is the intended reading of "new outreach".
- Threads accumulate per intent rather than per person. Creator-level history
  remains reachable through the creator record, which was always the canonical
  place for it; the thread is a conversation, not a contact.
- An agent that omits `reply_to_email_id` when answering silently opens a
  second thread. The failure is invisible at the call site, so the constraint
  is stated in the mail operation contract and the email draft/send playbook,
  and the draft path carries the field explicitly.
- Threads deleted or archived before this change still absorb inbound replies
  that reference them, because inbound attribution correctly follows the
  reference chain. Whether a new inbound message should lift a thread out of
  the bin is a separate product question, left open here; a test currently
  pins the archive case as deliberate.
