# Upstream watch

Defects that live in code we do not own, that we have decided to carry rather
than work around.

An entry belongs here when all three are true:

- a third-party dependency behaves wrongly, and we can point at the defect in
  its source or in a filed upstream report;
- the wrong behaviour reaches our users;
- we chose **not** to fork, patch, or hand-roll a replacement.

That third condition is the point of this folder. A local workaround for an
upstream bug is invisible: it looks like ordinary code, it accumulates, and
nobody ever deletes it once upstream ships the fix. Recording the decision
instead keeps the cost visible and gives the fix a defined end.

Each entry states what we carry, what it costs us, what we did instead, what we
deliberately did not do, and the condition that closes the entry. Delete an
entry when its condition is met — a stale entry is worse than none, because the
next reader will avoid a fix that already landed.

This is not an ADR: an ADR records a decision about our own architecture and
stays accepted. These entries are expected to die. It is not a plan either —
nothing here is scheduled.

## Entries

- [shadcn message-scroller](./shadcn-message-scroller.md) — transcript scroll
  and streaming follow behaviour.
