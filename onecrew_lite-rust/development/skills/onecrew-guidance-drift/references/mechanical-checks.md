# Mechanical Checks

Select checks that help the current audit. These locate structural drift;
they cannot establish whether a behavioral rule is true.

## Preconditions

Scope the active files and distinguish current guidance from dated records.
Use `development/lint/agent-context.py` and its configuration as the
structural authority; do not reimplement its checks in ad hoc scripts.
Search canonical `.skillshare/skills` and `development/skills` paths,
not their duplicate mirror views.

## Workflow

For a broad audit or final correction gate, run from the repository root:

```bash
node development/scripts/agent-check.mjs --profile skills
```

Reuse valid evidence under the verification contract. For a narrow correction,
start with the affected structural check or reference search and reserve the
full gate for the final diff.

Select additional searches by the suspected contradiction:

| Suspected drift | Check |
|---|---|
| Dead prose path or renamed symbol | Search the old path/name in affected active guidance; resolve each claimed target |
| Duplicate constraint identifier | Search that identifier in its owner entry |
| Unreachable mandatory gate | Follow entry routing to its reference or verification owner |
| Wrong command or script | Compare the cited invocation to current manifests, CI, and runner |
| Unclaimed owner handoff | Read the referenced owner's contract |
| Stale bundled Router operation | Compare the explicit named operation with live registry/source and its contract test |

Prefer bounded `rg` queries over dumping entire files or scanning all history.
For example, replace the placeholder with the concrete old symbol or path:

```bash
rg -n -F '<old-symbol-or-path>' .skillshare/skills development/skills
```

Expand to additional active owners only when the request or a found dependency
warrants it. A required gate may be correctly implemented in the shared command
matrix; do not demand a copied command inventory in every Hard Constraints
section. Identifier gaps are not defects by themselves.

## Failure recovery

Correct a flagged mismatch at its owner without relaxing validators or budgets.
Classify missing references separately from incorrect behavior. Investigate
only relevant history when it is needed to interpret a specific claim;
historical presence does not mean that text is current guidance.

A nonzero search exit may mean no matches, not a failed audit; an empty search
is also not proof that a universal behavioral claim holds. Verify such claims
from implementation and tests.

## Verification

Record which checks actually ran, their scope, and unresolved flags with
evidence. Do not claim that unselected checks passed. Substantive findings need
the guidance and contradicting source/contract; path existence alone supports
only a structural conclusion.

## Stop condition

Return the classified structural findings once the selected checks are complete.
For authorized corrections, complete the required final gate and affected diff
inspection. Semantic source verification remains necessary before claiming the
scoped drift audit is complete.
