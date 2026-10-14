# Skill Lifecycle and Verification

Complete the authorized canonical change and validate its affected contracts.
Neither a commit nor user acceptance is a prerequisite for delivering ready work.

## Preconditions

Establish scope and unrelated changes. For structural creation or lifecycle
work, inspect affected owners, routes, links, mirrors, size/overlap evidence,
and current official guidance not already verified for this delivery.
A narrow edit needs only its affected contract; do not repeat a full baseline.

## Workflow

Edit canonical files in `.skillshare/skills` or `development/skills`.
Update graph/cases when ownership, routing, effects, references, or lifecycle
actually change. During create/merge/split/rename/retire, move dependent links
and assets together; preserve one owner and leave no compatibility alias.

Add readiness/eval assets only when the content-placement risk test supports
them. Use focused checks while editing. On the final Skill-system diff, run:

```bash
node development/scripts/agent-check.mjs --profile skills
git diff --check
```

Use the verification owner's identity/reuse contract; do not duplicate its
mechanics here. Review only if the repository review trigger applies, then
close findings with affected evidence. Inspect final paths and diff stat.
Commit only when requested, and continue that action when already authorized.

## Failure recovery

Correct graph/tree/reference mismatches at the canonical source instead of
weakening validation. Resolve actual ownership overlap rather than adding a
permanent ignore. Preserve unrelated dirty work; if it blocks a readiness
packet, report that lane and continue independent authorized work.
Rerun only affected evidence after a relevant change. Respect the finite review
budget and report unresolved material risk instead of opening another loop.

## Verification

The runner is the authority for its declared graph, routing, mirror, readiness,
structure, budget, audit, and delivery-contract checks. Its passing result
proves those checks, not real model selection or product behavior. Reuse valid
receipts and evidence rather than requiring a new semantic review for each edit.

## Stop condition

Deliver when the authorized canonical change, affected graph/routes/assets,
mirrors, and required evidence agree, or report a concrete blocker after all
independent in-scope work is complete. Explicit acceptance ends the existing
delivery loop; do not wait for it before returning ready work.
