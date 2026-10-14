---
name: onecrew-review
description: Independently review a requested diff or a substantial risky delivery; reuse unchanged findings and review identity. Excludes routine implementation and standing-guidance audits.
---

# OneCrew Review

Review the uncovered risk surface of one logical delivery. Remain read-only;
return actionable findings and evidence gaps to the delivery coordinator.

## Hard Constraints

- **REVIEW-01** Review only when explicitly requested or when the final handoff has unreviewed cross-owner, trust, security, persistence, or integration risk. Subtasks and phases do not independently trigger review.
- **REVIEW-02** Allow one initial review per logical delivery. Reuse its receipt when the base commit, sorted reviewed paths, canonical diff fingerprint, and sorted risk boundaries all match.
- **REVIEW-03** Allow at most one re-review for Critical/High findings, changed public contracts, owner/dependency direction, persistence/security/secret boundaries, or fixes outside reviewed scope. Report unresolved Critical/High risk after that review; do not start a third loop.
- **REVIEW-04** Close Medium/Low findings with relevant regression evidence and parent diff inspection. Do not require another `No findings.` receipt or add a test that merely repeats a prose edit.
- **REVIEW-05** Group findings by root cause and give severity, path:line, impact, minimal fix direction, and confidence. `No findings.` is a valid verdict, not a mandatory delivery credential.
- **REVIEW-06** Consume available verification evidence; report gaps without executing tests, implementing fixes, or recursively invoking another Skill.
- **REVIEW-07** Acceptance, cancellation, or supersession ends this review loop. A completed review also returns immediately; waiting for acceptance is not review work.

## Workflow

Establish the delivery boundary and apply the trigger, identity, and review
budget above. Trace the changed behavior and immediate consumers in proportion
to risk. Return the receipt once the eligible review or targeted closure is
complete; the coordinator continues authorized implementation and verification.

## Review Receipt

Record the action (`RUN_INITIAL`, `REUSE`, `RUN_REREVIEW`,
`TARGETED_CLOSURE`, `SKIP`, or `STOP`), delivery identifier, identity,
review count, covered/uncovered surfaces, findings, and missing closure
evidence. Link prior receipts and logs instead of copying them.

## Related Skills

- onecrew-local-data-evolution — durable identity or persisted-shape risk.
- onecrew-new-skill — Skill ownership, trigger, or lifecycle defects.
- onecrew-verify — consume executable evidence; do not auto-trigger it.
- onecrew-rust-architecture — Rust module, API, supervisor, or structure boundaries.

## When NOT to use

Do not use for implementation or test execution, unchanged review identities,
routine subtask completion, or low-risk wording/generated/version edits alone.
