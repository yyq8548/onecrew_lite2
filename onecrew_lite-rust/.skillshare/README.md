# Shared Agent Skills

`.skillshare/skills` is the canonical discoverable source.
`.agents/skills` and `.claude/skills` are relative symlinks to it.
Development-only governance lives in `development/skills` and loads by path.

The selected upstream engineering Skills retain their original names and files.
Their approved source paths
and pinned revision are recorded in the ownership graph; only those imports
receive the project-name and upstream-layout exceptions. See the bundled
[upstream license](mattpocock-skills.LICENSE).

`grill-with-docs` is an explicit design interview with glossary/ADR work. It
replaces the retired `onecrew-phased-delivery` Skill, without adding a default
phase or approval gate to ordinary implementation work.

## Upstream engineering workflow

The available delivery chain is `grill-with-docs` → `to-spec` → `to-tickets` →
`implement` → `code-review`. Choose the steps a task needs: an existing spec can
go directly to implementation, and existing code can go directly to review.

`grill-with-docs` composes `grilling` and `domain-modeling`. Implementation can
use `tdd`; unsettled interface design can use `codebase-design`.
`setup-matt-pocock-skills` supplies the optional first-time setup workflow for
tracker and document locations. Installation alone does not select an external
tracker or create specs, tickets, or project configuration.

The repository's existing Creator, Rust, review and verification owners remain
in force. Upstream workflow suggestions do not authorize compatibility layers,
external publication, repeated approvals, or repeated checks beyond the user's
request and this repository's rules. In particular, implementation review must
include uncommitted candidate changes; the upstream `code-review` example
`<base>...HEAD` covers committed changes only.

The pinned upstream setup skips a triage-label file when `triage` is absent,
while `to-spec` and `to-tickets` still reference that vocabulary. Configure any
needed label mapping when selecting the tracker; this is not a dependency on
the separate `triage` workflow. Existing `CONTEXT.md` and `docs/adr` serve the
glossary and decisions; do not recreate the retired phase system.

## Routing and authority

`development/lint/project-skill-ownership.json` is the schema-v2 executable
graph for ownership, path roles, exact references, effects, verification,
conditional relationships, and stop conditions. Its adjacent routing cases
exercise positive and negative boundaries; they are not a second taxonomy.

Frontmatter selects a Skill. The entry gives the first decision and necessary
boundaries; direct references supply detail only when relevant. Related Skills
are conditional routes, never instructions to activate every neighbor.
Fixture checks establish contract consistency, not actual model selection;
use a forward Agent evaluation only when routing quality needs that evidence.

## Progressive disclosure

Keep entries short and within the budgets enforced by
`development/lint/agent-context.config.json`. Root guidance has its own
budget and contains stable authority and routes, not copied Skill procedures.
Keep each rule in one authoritative place, link references directly, and use
the smallest useful structure rather than repeating constraints as workflows,
checklists, and output contracts.

A large reference merits a relevance/ownership review; size alone does not
justify a split. Put volatile facts in executable sources or dated evidence.
Add readiness and eval packets when drift and failure risk justify them.
Readiness checks anchors and eval assets; it does not prove product behavior
or run an Agent evaluation.

## Validation

During edits, use the focused checks affected by the change. On the final
Skill-system diff, run the repository gate once:

```bash
node development/scripts/agent-check.mjs --profile skills
```

The runner defines the actual steps: graph/delivery-contract tests, declared
readiness checks, official Skillshare audit and sync preview, context lint,
and size/budget analysis. It emits schema-v2 evidence; `--reuse-evidence`
reuses only a matching passing identity. Rerun only when relevant inputs change
or the user requests fresh evidence. Inspect the final diff and run
`git diff --check` as the separate final-diff check.

The gate previews mirror sync without editing it. Reconcile mirrors with
`skillshare sync -p` when that effect is already authorized; request scope
only if mirror changes are outside the task.

Repository Python tooling uses `uv`. A support script distributed independently
to end users may keep a standard Python entrypoint: `portablePythonFiles` in the
lint config records exact script/guide paths and their portability reason.
It is not a directory-wide exclusion or permission to bypass other checks.
