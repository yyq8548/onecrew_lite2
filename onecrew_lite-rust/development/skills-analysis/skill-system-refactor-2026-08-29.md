# Skill System Refactor Evidence — 2026-08-29

This is a dated implementation-evidence snapshot, not standing routing
guidance. Baseline commit: `45df23742e66e7bc1e669469e6370028a9f91fe6`.
The worktree was clean when the baseline was measured.

## Scope and method

Audited both canonical catalogs, mirrors, root guidance, the executable lint
and gate, and the completed MCP plans. The comparison repository was read-only.
No product source, Tauri command, MCP contract, provider behavior, CRM/vault
schema, release, package, sign, install, or live effect was changed.

Metrics use the repository analyzer's `ceil(characters / 4)` estimate. Bundle
tokens include `SKILL.md` plus Markdown references, not scripts, evals, or
templates. Reference size is UTF-8 bytes.

## Baseline inventory

| Catalog | Skill | Entry lines / tokens | References / KiB | Bundle tokens | Baseline decision |
|---|---|---:|---:|---:|---|
| product | architecture-placement | 65 / 935 | 1 / 13.1 | 4,278 | keep; trim owner map |
| product | capability | 45 / 776 | 1 / 9.0 | 3,068 | keep |
| product | code-commentary | 82 / 1,114 | 0 / 0 | 1,114 | keep |
| product | codex-plugin | 131 / 1,907 | 1 / 9.2 | 4,246 | keep |
| product | crm | 45 / 865 | 0 / 0 | 865 | keep |
| product | desktop-ui | 51 / 1,263 | 3 / 31.2 | 9,209 | keep; trim two large references |
| product | local-data-evolution | 53 / 1,001 | 3 / 27.9 | 8,119 | keep; add high-risk packet |
| product | native-desktop | 52 / 864 | 5 / 24.4 | 7,081 | keep; remove standing host assumption |
| product | operator-sessions | 45 / 764 | 0 / 0 | 764 | keep |
| product | peer-adapters | 58 / 2,195 | 1 / 7.8 | 4,195 | keep; rewrite thin router; add packet |
| product | phased-delivery | 65 / 1,429 | 0 / 0 | 1,429 | keep; deduplicate phase/final coverage |
| product | review | 81 / 1,214 | 0 / 0 | 1,214 | keep; add identity and one-review budget |
| product | rust-architecture | 78 / 962 | 3 / 24.6 | 7,229 | keep; centralize standard commands |
| product | service-credentials | 43 / 817 | 0 / 0 | 817 | keep |
| product | tiktok-web-signing | 46 / 926 | 3 / 29.2 | 8,409 | keep; move volatile layout to source/dates |
| product | verify | 113 / 1,498 | 1 / 8.9 | 3,775 | keep; add evidence identity/reuse |
| product | x-chat-crypto-vendor | 45 / 783 | 0 / 0 | 783 | keep |
| governance | guidance-drift | 101 / 1,485 | 1 / 7.6 | 3,426 | keep |
| governance | new-skill | 148 / 1,869 | 3 / 18.7 | 6,638 | keep; replace with graph-first router |

Baseline totals:

- 19 Skills: 17 discoverable, 2 development-only;
- 1,347 entry lines and about 22,667 entry tokens;
- 26 references, 216,534 bytes;
- about 76,659 bundle tokens;
- 5 references above 10,000 bytes;
- 66 unique Related edges, 19 reciprocal pairs, and a 17-node strongly
  connected component;
- no structured path-role, effect, verification-owner, or stop declaration;
- no `agents/openai.yaml`, eval suite, readiness gate, or feature template.

## Trigger-overlap baseline

The lexical comparison was a diagnostic, not a routing oracle. Pairs at or
above the review threshold:

| Pair | Baseline signal | Decision |
|---|---:|---|
| native-desktop / verify | .136 | owner supplies rows; verify owns commands/evidence |
| review / verify | .125 | keep distinct; remove return edge and lifecycle recursion |
| rust-architecture / new-skill | .116 | cross-catalog generic-word false positive |
| crm / peer-adapters | .103 | legal owner-to-projection pairing |
| capability / peer-adapters | .091 | legal provider-to-projection handoff |
| architecture-placement / review | .081 | review is conditional, not implied by placement |
| review / new-skill | .081 | one final review only |
| native-desktop / peer-adapters | .077 | native mechanic versus public projection |
| capability / crm | .075 | provider mechanic versus application policy |
| architecture-placement / native-desktop | .071 | placement then native owner |
| architecture-placement / rust-architecture | .067 | placement only when owner/direction changes |
| codex-plugin / guidance-drift | .067 | generic-word false positive |
| native-desktop / tiktok-web-signing | .064 | native/"signing" lexical false positive |
| phased-delivery / review | .061 | subtract frozen reviewed phase coverage |
| tiktok-web-signing / verify | .061 | owner rows plus verification execution |

## Duplicate-rule baseline

| Cluster | Baseline defect | Resolution |
|---|---|---|
| Review/verify | Both directions could start the other; no identities or budget | one-way evidence consumption plus executable delivery contract |
| Verification lifecycle | implementation, review fix, and handoff repeated full gates | focused/finding/final stages; identical PASS reuse |
| Phase/final review | reviewed phases were reviewed again at final | subtract unchanged frozen phase coverage |
| Adapter surface | fixed inventory and completed migration prose lived in Skill | point to executable registry/tests; remove legacy narrative |
| Persistence | current schema literal copied into guidance | point to source constant and adjacent registry |
| Desktop | route inventory copied into reference | point to current executable route owner |
| Native | standing Linux/Mac split and duplicated commands | detect host at execution; verify owns commands |
| Rust | owner reference copied standard gate commands | owner keeps behavior proof; verify keeps commands |
| Root docs | detailed commands, crate map, and workflow inventory | retain constitution/system map/ADR index only |

## Keep, merge, split, retire

| Action | Result |
|---|---|
| Keep | all 19 Skills; each retains an independent owner concern |
| Merge | none; no pair shared owner, first action, lifecycle, and evidence strongly enough |
| Split Skill | none; large bundles remain coherent owners with conditional references |
| Split reference | none after trimming; all direct references are below 10KB |
| Rename | none |
| Retire Skill | none |
| Retire semantics | review/verify return loop, duplicate peer-to-UI edge, fixed inventories, legacy adapter migration instruction, standing host identity |
| Add full packet | peer-adapters and local-data-evolution only |
| Keep lightweight | all other Skills; routing fixtures check owner/reject/reference contract integrity |

## Implemented graph and teaching model

`project-skill-ownership.json` schema v2 is the sole executable graph. Each
entry declares one owner concern, WHAT, positive/negative cases, role-aware
`owns`/`appliesTo`/`governs`/`observes` scopes, references, verification owner,
effects, typed conditional relationships, and stop condition. Only canonical
`owns` paths are exclusive. The adjacent routing fixture maps user intent to owner,
optional cross-cutting Skills, exact direct references, rejected neighbors,
verification lane, and stop.

Evidence boundaries are explicit: frontmatter is the real implicit-trigger
surface; Scenario Router teaches the first action after selection; routing
fixtures prove only the maintained owner/reject/reference/stop contract is
self-consistent; a future forward Agent eval would be needed to measure actual
model routing behavior.

The teaching order is:

```text
Core Model -> Scenario Router -> Workflow -> conditional Readiness Drills
           -> Hard Stops -> Evals
```

Routing fixtures check the maintained contract, while eval cases are designed
to test the teaching; readiness validates only the eval assets. Neither replaces
the entry/reference teaching or runs an Agent/grader. Peer, review, verify, and
phased-delivery now contain short paired scenarios that change the first action,
reference/owner choice, evidence lane, and stop.

## Review, verification, and plan closure

- Review identity: base commit + reviewed path set + diff fingerprint + risk
  boundary.
- Evidence identity: exact argv + cwd + relevant diff fingerprint +
  dependency/manifest fingerprint.
- Initial review budget: one per logical delivery.
- Re-review budget: one, only for Critical/High or declared boundary deltas.
- Medium/Low closure: finding regression + affected verification + parent diff
  inspection; no required second `No findings` receipt.
- Explicit user acceptance is terminal; later feedback creates a new delivery.
- `layered-mcp-platform-surface` is superseded and closed.
- `stateless-mcp-operation-registry` is archived in place through Phase 10;
  deferred user/native/live/release rows are not active agent phases.

## After metrics

| Metric | Before | After |
|---|---:|---:|
| Skill count | 19 | 19 |
| Entry lines | 1,347 | 1,243 |
| Estimated entry tokens | 22,667 | 21,154 |
| Estimated bundle tokens | 76,659 | 67,856 |
| Reference bytes | 216,534 | 187,250 |
| References over 10KB | 5 | 0 |
| Root constitution lines | 173 | 125 |
| Root architecture lines | 167 | 113 |
| High-risk feature packets | 0 | 2 |

## Known follow-ups

- Evidence reuse is safe but conservative: the unified runner fingerprints the
  full final candidate for selected lanes. A later change may scope Rust,
  frontend, and Skill identities to their dependency cones.
- Routing fixtures prove contract integrity, not actual Codex implicit
  selection. A forward Agent/model routing eval remains optional future work.
- Tauri command and DTO generation remains a separate delivery; this refactor
  keeps the manual cross-language mirror audit as an explicit verification row.

The final gate/review/commit evidence is reported at handoff so this dated
baseline does not claim work that had not yet run when the snapshot was written.
