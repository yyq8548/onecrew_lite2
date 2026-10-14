---
name: onecrew-guidance-drift
description: Audit standing project guidance against code when requested or a contradiction is evidenced. Excludes ordinary defect fixes, diff reviews, and unrelated historical material.
---

# OneCrew Guidance Drift

Audit active standing guidance against executable truth when requested, or
when a concrete contradiction has already been found. Keep the audit bounded
by that request or contradiction.

## Hard Constraints

- **DRIFT-01** Cite both the guidance claim and the contradicting source or authoritative contract. Distinguish proven contradictions, stale references, and hypotheses; memory, empty search results, and file existence alone do not prove behavior.
- **DRIFT-02** For a rule that teaches a defect, trace the relevant mechanism and intended boundary. Expand only along concrete ownership or reference dependencies, not every defect since an arbitrary past audit.
- **DRIFT-03** Keep routing decisions and mandatory first gates in the entry. Detailed commands may live in a reachable reference or verification owner; do not copy every executable gate into Hard Constraints.
- **DRIFT-04** Check relevant ownership handoffs actually reach an owner. Two documents deferring to each other leave a contract unowned.
- **DRIFT-05** Select useful [Mechanical checks](references/mechanical-checks.md) for the affected scope. A full baseline belongs to broad audits, not every narrow correction.
- **DRIFT-06** Edit only evidence-backed issues within authorization. A requested broad rewrite already authorizes its included owners; report and seek scope only for additional work outside that request.
- **DRIFT-07** Prefer executable sources for moving inventories and versions. Preserve valid dated or version-specific records, and leave excluded historical plans/analysis untouched.
- **DRIFT-08** Report audit findings when the request is read-only; when corrections are authorized, complete them and their required checks before handoff.

## Workflow

Identify the active guidance and behavior in scope. Use targeted mechanical
checks to locate broken references, then verify substantive claims from source
and follow relevant ownership handoffs. Correct authorized contradictions and
routing gaps. Report findings, placement, evidence, and checks once the scoped
audit or correction is complete.

## Placement

| Finding | Destination |
|---|---|
| Wrong behavior or missing owner | The owner entry or its routed reference |
| Unreachable required first gate | The owner entry |
| Consequential unrecorded decision | An ADR when decision documentation is in scope |
| Volatile inventory | Executable source, linked from the explanation |
| Historical statement | Preserve as dated evidence unless correction is requested |

## Related Skills

- onecrew-code-commentary — code comments and owner READMEs.
- onecrew-new-skill — changed ownership, triggers, or lifecycle.
- onecrew-review — defects in one delivery diff.
- onecrew-verify — executable evidence for corrections.

## When NOT to use

Do not run as routine background work, for every defect or public-contract
change, or because another Skill finished. Do not use a drift audit as license
to rewrite unrelated documentation, historical records, or product code.
