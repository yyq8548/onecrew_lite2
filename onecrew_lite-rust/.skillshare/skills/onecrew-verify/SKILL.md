---
name: onecrew-verify
description: Select and reuse checks for the changed behavior and its consumers. Excludes semantic review, repeated unchanged gates, and effects outside user authorization.
---

# OneCrew Verify

Choose the smallest evidence set that supports the changed behavior and
required repository checks. Use the [Verification Command Matrix](references/command-matrix.md)
for commands, stages, identity mechanics, and reuse.

## Hard Constraints

- **VERIFY-01** Select lanes from changed behavior, touched owners, risk, and immediate consumers. During implementation or finding closure, run focused checks; at handoff, run only required final lanes not already covered by valid evidence.
- **VERIFY-02** Reuse identical passing evidence under the matrix contract. A lifecycle label or fresher timestamp is not a reason to rerun a gate; relevant input changes invalidate only affected lanes.
- **VERIFY-03** Use current manifests and CI as command authority. Keep root/vendor workspaces and targeted/final evidence distinct; never infer runtime, native, or release proof from compilation, mocks, or cross-builds.
- **VERIFY-04** Preserve unrelated changes, prefer check-only formatting, and inspect scripted blast radius. For low-impact prose or reversible edits, use structural/diff checks rather than implementation-mirroring tests.
- **VERIFY-05** Check effect authority against the existing user request. Continue effects already in scope without requesting approval again; request new authority only for additional live, destructive, production, or release effects.
- **VERIFY-06** Report `PASS`, `FAIL`, or `BLOCKED` truthfully. Missing tools, services, credentials, targets, or hosts are blockers for that lane, not proof that it passed.
- **VERIFY-07** Return semantic defects to the coordinator without auto-starting review. End when selected evidence is complete or a concrete blocker is reported; do not wait for a commit or user acceptance.

## Workflow

Select the relevant matrix route, inspect existing evidence, and execute the
missing or invalidated checks. Diagnose a meaningful failure before rerunning;
fix in-scope causes and continue authorized work. Record environmental blockers
without repeatedly attempting the same unavailable lane.

## Output Contract

Give stage, verdict, exact command/cwd, evidence identity, log or result, and
reuse source when applicable. Identify skipped or blocked lanes and their
practical limits. Do not turn the report into a new review or verification loop.

## Related Skills

- onecrew-local-data-evolution — changed persisted identity or upgrade behavior.
- onecrew-native-cli — native host, target, runtime, and distribution evidence.
- onecrew-x-chat-crypto-vendor — separate parent, vendor, binding, and generated evidence.

## When NOT to use

Do not use for review-only or architecture discovery requests, repeat unchanged
passing evidence, or broaden a check into effects outside the authorized task.
