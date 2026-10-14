---
name: onecrew-code-commentary
description: Update code comments and active owner READMEs when a mechanism changes. Excludes historical records and standing Skill/architecture-guidance audits.
---

# OneCrew Code Commentary

Keep code comments and owner READMEs accurate when mechanisms or their
explanations change. This governs claims, not comment density or language.

## Hard Constraints

- **COMMENT-01** Search the affected owner and known active documentation for explanations invalidated by a mechanism change. Broaden only when references or ownership evidence lead elsewhere; historical plans and dated analysis are not a routine rewrite target.
- **COMMENT-02** Scope guarantees to the behavior actually established by code and tests. Searching for counterexamples can locate evidence; an empty search cannot prove "only", "always", or "every".
- **COMMENT-03** Route changing inventories, counts, and versions to executable source. Keep an exact version when it is needed to identify a protocol, compatibility boundary, or dated historical claim; distinguish that from a moving current-state assertion.
- **COMMENT-04** Verify named identifiers and paths still exist, and that stated reasons and deliberate limitations still match behavior. Correct materially false explanations within the authorized scope.
- **COMMENT-05** Owner READMEs explain interfaces, failure behavior, and usage; enumerate only what helps users and has a maintainable source of truth.
- **COMMENT-06** Preserve intentional English/Chinese commentary. Do not rewrite correct prose merely for language, tone, or uniformity.

## Workflow

Follow the changed mechanism to its explaining comments and active owner
documentation. Check claims against implementation and relevant tests, correct
or remove invalid explanations, and inspect the affected diff. Broaden the
search only for a concrete dependency or stale reference. Leave historical
records intact unless the user requested their correction.

## Related Skills

- onecrew-verify — evidence needed for a changed behavioral claim.
- onecrew-review — defects in one delivery diff.

## When NOT to use

Do not use for a repository-wide historical sweep, translation, comment quotas,
or a standing-guidance audit. Skills, ADRs, and root guidance belong to
`onecrew-guidance-drift` when that audit is requested or evidenced.
