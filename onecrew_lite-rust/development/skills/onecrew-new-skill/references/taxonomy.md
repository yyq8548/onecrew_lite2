# Skill Taxonomy and Graph

Choose an owner from the affected routing evidence before choosing a name.
For a narrow rule change, inspect adjacent owners; a catalog-wide refactor
requires the broader overlap and size baseline.

| Evidence | Decision |
|---|---|
| Same concern, trigger family, workflow, and verification | Extend the existing Skill |
| Same owner, conditional detail | Refine one direct reference |
| Same concern repeatedly co-activated for the same lifecycle | Consider merge and deduplicate |
| Distinct concern, first decision, or evidence lane | Consider split or create |
| Name changed, concern unchanged | Rename atomically |
| No remaining independent concern | Retire atomically |
| Large file, same concern | Trim or route before splitting |

The source of graph fields and constraints is
`development/lint/project-skill-ownership.json` and its schema-v2 validator.
Canonical `owns` paths are exclusive; `appliesTo`, `governs`, and
`observes` may overlap. The adjacent routing fixture maps one intent to one
owner, conditional cross-cutting Skills, exact references, verification, and
rejected neighbors. Do not create another manifest in prose or root docs.

An explicitly declared owner-to-companion `pair` can legitimately recur, such
as i18n with React rendering. The overlap check counts unexplained joint
activation, not that declared composition. A handoff alone does not justify
loading both, and shared selection by a third owner remains overlap evidence.

Keep WHAT/WHEN/WHEN NOT concise in frontmatter. Cases should separate realistic
nearby intents, not merely repeat the description. Related Skills reflect
typed graph targets without recursive invocation.

Split when distinct intents repeatedly load irrelevant detail. Merge when
separation copies constraints for the same task. Do not decide from a broad
keyword, file size, or one new rule alone. A new owner needs a distinct concern
that existing owners cannot naturally absorb.

Discoverable workflows live in `.skillshare/skills`; governance lives in
`development/skills`. Lifecycle changes update cases, references, graph
edges, and mirror expectations together without aliases or orphan owners.

Use current official guidance for structural authoring decisions when it has
not already been established for this delivery. Do not refetch it merely to
correct a narrow sentence. Graph/case checks prove internal consistency;
actual model selection requires separate evaluation when needed.
