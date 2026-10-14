# Domain documentation

This repository uses one domain glossary, `CONTEXT.md`, and the existing architecture decisions in `docs/adr/`.
Multiple Rust crates and frontend applications do not create additional domain contexts by themselves.

Before proposing a domain or ownership change, read the root glossary, `ARCHITECTURE.md`,
and the ADRs relevant to the affected responsibility. Source, tests, and manifests resolve drifting prose.

Use the glossary's terms in specifications and issues. Keep implementation plans, file inventories,
and verification evidence out of the glossary. Record new terminology only when its meaning is resolved.

Create an ADR only for a meaningful architectural trade-off that is costly to reverse and would be surprising without context.
State conflicts with an existing ADR explicitly. Do not recreate retired phase plans or add a new delivery approval system.
