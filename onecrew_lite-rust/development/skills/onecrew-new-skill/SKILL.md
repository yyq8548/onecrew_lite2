---
name: onecrew-new-skill
description: Create or restructure project Skills, references, ownership, and routing. Excludes ordinary product implementation and personal/global Skill management.
---

# OneCrew Skill Governance

Keep one canonical Skill graph with clear ownership and small entries that
route to conditional detail. The user-authorized task overrides obsolete
process rules in the existing guidance.

## Hard Constraints

- **SKILL-GOV-01** Match discovery to scope. For creation or structural lifecycle changes, inspect adjacent owners, routing, references, and current official guidance; for a narrow correction, inspect the affected contract without repeating a catalog baseline or external fetch.
- **SKILL-GOV-02** Use `.skillshare/skills` as the discoverable source and relative `.agents/skills` / `.claude/skills` symlinks as mirrors. Governance stays in `development/skills`.
- **SKILL-GOV-03** Preserve schema v2 in `development/lint/project-skill-ownership.json` as the sole executable owner/relationship graph, with its adjacent routing cases. Use evidence of ownership, trigger, workflow, and verification overlap to choose extend/create/merge/split/rename/retire.
- **SKILL-GOV-04** Keep one concern per Skill, concise triggers, explicit exclusions, exact reference routes, role-aware paths, effects, verification owner, conditional relationships, and a reachable stop condition. Related Skills are routes, not automatic invocations.
- **SKILL-GOV-05** Keep entries within repository budgets and references one direct level. Put necessary decisions before optional detail; use examples, readiness, and eval assets only when they resolve evidenced ambiguity or risk.
- **SKILL-GOV-06** Move affected graph entries, routes, links, and mirrors together during lifecycle changes. Do not leave aliases, duplicate ownership, or orphan assets.
- **SKILL-GOV-07** Use focused checks during edits and one final Skill-system gate for the final diff. Review only when the review contract requires it; reuse unchanged evidence and receipts.
- **SKILL-GOV-08** Deliver when the authorized change and required evidence are ready. Do not require a commit, additional approval, or user acceptance to finish already authorized work.

## Router

| Need | Read |
|---|---|
| Decide owner or lifecycle action | [Taxonomy and graph](references/taxonomy.md) |
| Place a rule, reference, eval, template, or readiness asset | [Content placement](references/content-placement.md) |
| Execute lifecycle changes and validate the final system | [Lifecycle and verification](references/lifecycle-and-verification.md) |

## Workflow

Locate the affected owner and choose only the routes needed for the request.
For structural changes, record the baseline and decision evidence relevant to
the affected catalog; a broad refactor warrants a broader baseline. Edit the
canonical source and dependent contracts, validate the final change, and report
the result and any remaining limitation. These are decision boundaries, not a
mandatory sequence of every possible authoring activity.

## Output Contract

Report what changed and why, affected ownership/routing, proportionate
before/after size evidence, checks or reused evidence, and any unresolved
scope. Include lifecycle maps or readiness/review decisions only when relevant.

## Related Skills

- onecrew-review — an eligible independent semantic review.
- onecrew-verify — execute or reuse Skill-system evidence.
- onecrew-guidance-drift — source-backed standing-guidance contradictions.

## When NOT to use

Do not use for ordinary product implementation or personal/global Skill
management. Extend an existing owner for a fitting rule; do not create a broad
Skill without a distinct concern and negative cases, or load every reference
for a narrow edit.
