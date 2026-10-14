# Skill Content Placement

Put each rule where its authority and rate of change belong. Prefer one clear
definition and a route over copies in constraints, workflow, and checklist.

| Content | Placement |
|---|---|
| Stable authority, safety, owner hierarchy | `CLAUDE.md` (`AGENTS.md` symlink) |
| Stable system shape and decision index | `ARCHITECTURE.md` |
| Consequential design decision and tradeoffs | `docs/adr/` when in scope |
| Owner interface, failure behavior, usage | Owner-local README |
| Trigger, first decision, required boundary, completion | `SKILL.md` |
| Conditional procedure, matrix, pattern, evidence rubric | One direct reference |
| Moving version, schema, DTO, command or tool inventory | Source, tests, manifest |
| Historical capture or snapshot | Dated evidence |
| Machine routing, ownership, effects, relationships | Executable graph and cases |

An entry is a short router, not a miniature manual. Do not impose a uniform
Core Model/Scenario/Workflow/Checklist itinerary on entries or references.
A simple table or paragraph can teach a narrow
decision. Include examples only when they remove ambiguity.

References are direct children with a declared kind (`pattern`, `workflow`,
or `map`), linked once and loaded only for the current intent. Keep the
decision to load a required reference in the entry. Detailed commands belong
to the verification owner rather than being copied into every entry.

Keep repository-wide safety in the constitution; owners add domain-specific
consequences. Route volatile facts to source, and retain explicit versions
when needed to identify a historical or compatibility boundary.

Use full readiness/eval packets for evidenced drift-prone cross-module work
where failure affects security, persistence, credentials, cryptography, or
external effects. Small deterministic guidance can remain instruction-only.
Do not add assets solely to satisfy a uniform template.

Readiness is read-only and reports structured pass/fail/blocked for stable
anchors and asset validity. Evals test taught decisions with representative
positive, negative, and verification cases; neither substitutes for the entry
or proves product runtime behavior. Enforce repository size budgets rather
than restating their values in every document.
