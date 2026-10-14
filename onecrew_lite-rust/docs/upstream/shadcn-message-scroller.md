# shadcn message-scroller

**Dependency:** `@shadcn/react` `message-scroller`. Installed version 0.3.0,
which is also the latest published version — there is nothing to upgrade to.

**Where it runs:** the outreach thread (`apps/desktop/src/components/global-outreach.tsx`).

## Why we use it rather than our own

We had our own. It was removed because it could not tell a user's scroll from a
layout shift: dragging the scrollbar and pressing PageUp produce no `wheel` or
`touchmove` event, so every upward drag was read as content moving and yanked
back to the bottom. Anything we write ourselves has to solve that problem again,
and the primitive already solves it correctly for the ordinary cases.

## What is broken upstream

Verified 2026-08-21; all three still open.

**1. A manual scroll during an anchored streaming turn strands the reserved tail
spacer.** [Issue #11125](https://github.com/shadcn-ui/ui/issues/11125), fix in
[PR #11465](https://github.com/shadcn-ui/ui/pull/11465) — open, unmerged. The
issue names the `autoScroll` + `scrollAnchor` combination we use. Re-arming
follow mode is not gated on the spacer being drained, and nothing shrinks a
released spacer back to zero.

**2. `autoScroll` silently disengages at the clamped bottom and never re-arms.**
[Issue #11224](https://github.com/shadcn-ui/ui/issues/11224), fix in
[PR #11223](https://github.com/shadcn-ui/ui/pull/11223) — open, unmerged. Wheel
events release follow without checking direction, and re-arming needs a `scroll`
event that a non-scrolling gesture never produces. The issue body specifically
names a wheel consumed by a nested scrollable inside a message. We have three
such scrollers in the transcript: the markdown `pre` and table
(`chat-markdown.tsx`) and the tool row's output (`tool-call-row.tsx`).

**3. A viewport height change releases follow permanently, and nothing re-arms
it.** Not filed upstream. Same family as defect 2 and not covered by its PR,
which only handles wheel gestures. Observed directly: resizing the transcript
viewport sends `scrollTop` to 0 — not a clamp, the new maximum was well above
the old offset — and the mode machine then reads "scrolled up with content still
below" and latches to `free-scrolling`. Both automatic re-pin paths gate on that
mode, so a single involuntary displacement is permanent; only scrolling back
within 8px of the bottom or pressing the button recovers.

Our own trigger for the displacement was a percentage height on the viewport and
is fixed on our side. The latch is not: anything else that moves the offset
without a gesture will still stick. Worth filing — it is easier to hit than the
wheel path.

**4. The anchored mode cannot be released by dragging the scrollbar.** Not filed
upstream. Read from the vendored source rather than reported by anyone: the mode
machine promotes only out of non-anchored modes, and the user-intent hook is
wired to `onWheel`, `onTouchMove` and a key list — not to `onScroll`. So a
scrollbar drag cannot leave `anchored-to-message`, while the documented
behaviour is that a manual scroll releases the view. This is our reading of
minified source, not a confirmed upstream position.

## What we did instead

Nothing that touches the primitive. We removed our own inputs to its state
machine, so the paths above stop being reachable from this app:

- tool-call groups derive fold eligibility monotonically, so a group no longer
  oscillates by ~700px twice per tool call and stops driving the resize path;
- the "executing" indicator is no longer a child of `MessageScroller.Content`,
  so a terminal event can no longer produce a net-zero child mutation and land
  in the equal-count anchor scan;
- `scrollAnchor` is scoped to the running turn, so neither anchor scan can
  target a historical message;
- the provider's `key` no longer follows a ref that is assigned mid-turn, so the
  scroller is not torn down and rebuilt while a turn streams.

These do not fix the upstream defects. A user who scrolls with the trackpad
during a streaming turn, or whose wheel is swallowed by a code block inside a
message, can still land in defect 1 or 2.

## What we deliberately did not do

- **`patch-package` against `node_modules`.** Reconsider only if the symptoms
  survive the changes above; a patch is a fork with no owner.
- **Pin the dependency to an exact version.** `^0.3.0` resolves to 0.3.0 today.
  Pinning would also mean not receiving the fix when it ships.
- **Hand-written wheel gating or `deltaY`-sign detection.** That is the intent
  detection we already failed at once.
- **`scrollToEnd()` on turn completion.** It yanks a reader who deliberately
  scrolled up, and the primitive exposes no mode to make it conditional.

## Condition that closes this entry

Either fix PR merges and a version above 0.3.0 ships. Then: upgrade, re-run the
three symptoms below on a Mac, and delete the corresponding section here. Delete
the file when all three are gone.

Symptoms to re-test:

1. a tool group collapsing and expanding while output streams;
2. the transcript following streamed output after scrolling up and back down;
3. a turn that ends by cancellation or error leaving the viewport where it was;
4. resizing the window, toggling the MCP banner, or dragging the dock's width
   while the transcript sits at the bottom — it should stay at the bottom.

Defects 3 and 4 need filing upstream before they can close.
