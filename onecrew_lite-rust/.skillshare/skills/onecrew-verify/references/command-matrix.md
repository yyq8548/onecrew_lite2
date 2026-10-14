# Verification Command Matrix

Select commands from the changed behavior, affected owner, risk, and evidence
stage. Resolve package, script, filter, target, and host capability from current
manifests and CI. A path touch alone does not require every owner lane: prose,
formatting, and other low-impact changes need the relevant structural checks;
behavior and contract changes need executable coverage.

## Evidence identity and reuse

Every executable result is keyed by:

```text
exact argv
+ canonical cwd
+ command-relevant diff fingerprint
+ dependency/manifest fingerprint
```

Only an identical `PASS` is reusable. A changed command, cwd, relevant file,
dependency, manifest, or gate contract invalidates that lane. `FAIL` and
`BLOCKED` remain history and never become pass.

The unified runner prints one concise `START` and terminal status line per step
to stderr, including its log path; stdout remains the single final JSON result
for machine consumers. A structured readiness blocker is reported as
`BLOCKED` with its code instead of being collapsed into a generic failure.

The current unified runner conservatively fingerprints the full final candidate
for its selected lane. This can invalidate more evidence than necessary but
cannot create unsafe reuse. Per-crate/frontend dependency cones are a separate
future optimization and must not be claimed as implemented here.

The unified runner writes schema-v2 evidence keys. Reuse is explicit:

```text
node development/scripts/agent-check.mjs --profile skills \
  --reuse-evidence .agent-check/<run>/result.json
```

Omit the reuse option when the user asks for a fresh run.

## Stage selection

| Stage | Run | Do not run |
|---|---|---|
| Implementation | Focused changed branch, error path, or immediate consumer | Full owner gate merely because implementation started |
| Finding closure | Finding-specific regression and affected-owner focused checks | Full revalidation for Medium/Low wording or local fixes |
| Final commit/handoff | Required owner lanes on the final diff, reusing valid evidence | Unrelated full suites or a second identical final gate |

Phases do not create stages automatically. Reuse unchanged frozen phase
evidence and run only missing or invalidated final identities. Do not add tests
that merely mirror a reversible prose or formatting edit. Existing required
repository checks still apply.

## Owner routes

### Skill and guidance system

From the repository root:

```text
node development/scripts/agent-check.mjs --profile skills
git diff --check
```

The runner defines the Skill profile: graph/delivery tests, declared readiness
checks, official Skillshare audit and sync preview, context lint, and token/size
analysis. Run affected checks during editing and this profile once on the final
Skill-system diff. `git diff --check` remains a separate final-diff assertion.

### Root Rust crate

For a focused implementation or finding regression, run the nearest declared
test or filter. For changed Rust behavior, the standard final owner gate is:

```text
cargo fmt -p <crate> -- --check
cargo clippy -p <crate> --all-targets -- -D warnings
cargo test -p <crate>
```

Add immediate consumers only when a shared contract changes. Never point
`rustfmt` at a module root; confirm scripted blast radius afterwards.

When Rust source documentation or rustdoc policy changes, add relevant
documentation and doctest evidence instead of treating compilation as proof.
Use the affected crate for local changes; use these workspace lanes when the
change or selected required profile spans the workspace:

```text
cargo doc --workspace --no-deps
cargo test --workspace --doc
```

The commit/full unified runner includes these workspace lanes. Its existing
`cargo test --workspace` step remains the product-test lane and is not relabeled
as documentation-only evidence.

### Web frontend

Use the nearest Vitest target while implementing. For frontend behavior or
build-contract changes, the standard final gate runs from `apps/web`:

```text
npm run typecheck
npm test
npm run build
```

This build is a frontend compilation, not HTTP packaging, signing, installing,
or native runtime proof.

### MCP UI generated assets

When `apps/mcp-ui` source changes, run its package-owned test, check, and build
scripts, then build a second time and confirm the generated asset hashes stay
identical. Add MCP adapter and Web consumers when the embedded bytes or
interaction contract changes. Never run MCP UI tests through the Web suite
or hand-edit generated HTML.

### Bundled Codex plugin

Run the web marketplace suite that pins the Router Skill, references, stdio MCP
contract, retired-name absence, and content digest. Add the Codex staging test
when staged bytes change. A Rust-only run does not cover this distribution
surface. Router, plugin manifest, MCP App fixture or preview export, and Web
runtime-change scope edits also need the `onecrew-mcp-adapter` Router drift
test (`cargo test -p onecrew-mcp-adapter router_references`); the unified
runner selects its cargo group for those paths.

### Vendored crypto workspaces

Keep parent integration, vendor core, language bindings, and generated assets
as separate evidence identities. Resolve the current vendor commands from the
vendored workspace. Never infer one binding passed from another target.

## Specialized matrices

- Local data evolution supplies fresh/current/predecessor/failure/recovery,
  lifecycle, Web gate, and MCP-absence rows; execute its full matrix on the
  final diff.
- Native CLI supplies host/target/runtime/distribution rows. Detect the
  current host and toolchain at run time; unsupported targets are `BLOCKED`.
- X Chat crypto supplies parent/vendor/binding/generated rows.

## Hand-audited boundaries

Compilation cannot prove hand-maintained cross-language mirrors. When a Rust
public DTO changes, compare its TypeScript consumer shape explicitly and run
the focused consumer suite. Keep this as a temporary verification row until a
separate authorized code-generation delivery replaces the manual boundary.

## Host and effect boundaries

Inspect the actual host, target, installed toolchain, credentials, and service
availability at execution time. Never encode “this machine” or “the user's Mac”
as a reusable Skill fact. A lane the current environment cannot execute is
`BLOCKED`, not passed elsewhere by assumption.

Verification does not itself authorize live writes, local durable upgrades,
application launch, packaging, signing, installation, publication, or production
access. Check the user's existing scope and continue effects already authorized;
request new authority only for additional effects outside that scope.

## Interpretation

Compilation proves compilation. A mock proves the mocked path. A cross-build
proves candidate construction. A readiness check proves only stable anchors and
eval assets are ready. Report the strongest executed evidence, reuse source,
and every omitted or blocked lane without escalating into semantic review.
