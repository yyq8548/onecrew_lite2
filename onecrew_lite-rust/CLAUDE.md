# OneCrew / Agent Platform Local

OneCrew is a local-first Rust CLI with an embedded React webpage for social accounts, Creators, and
authenticated loopback MCP. Keep changes scoped, typed, and secret-free at
public boundaries. Source, tests, and manifests resolve drifting prose.

## Constitution

- Follow the user's current request, then this constitution, then the smallest
  matching set of repository Skills.
- Check `git status --short`, preserve unrelated changes, and use `apply_patch`
  for hand edits.
- Do not add compatibility aliases, fallbacks, duplicated models, or defaults
  that hide a missing required value without an explicit product requirement.
- Keep commits, publishing, packaging, signing, installation, application launch,
  live effects, and shared build-state cleanup within the user's authorized
  scope. Authorization persists across turns; do not ask again for an already
  authorized action or infer a new approval gate from a workflow suggestion.

## Owner hierarchy

- Shared typed application owners govern workflows: `platform-runtime` owns
  account and provider orchestration, transient search-plan authority, and
  process-local results; `creator-runtime` owns Creator business policy and data.
  The immutable runtime capability catalog declares identity and navigation,
  without handlers or schemas. `mcp-adapter` owns MCP bindings, schemas,
  execution, search, and guide projection; HTTP calls typed owners directly.
- `creator-runtime` owns Creator models, use cases, persistence, confirmation,
  idempotency, and audit. `platform-runtime` owns account workflows and governed
  dispatch; `local-vault` owns durable credentials.
- `onecrew` owns the thin CLI and embedded assets; `local-service` owns the
  shared application host and authenticated HTTP/MCP/control IPC. `codex` owns
  explicit plugin installation, cached/refresh status, and setup only.
- Provider code follows `transport -> client -> facade -> runtime -> adapter`;
  callers do not bypass the facade. Platform behavior (validation, failure
  evidence) stays in its platform crate and session construction in
  `platform-sessions`; the runtime and adapters do not re-implement it per
  platform (ADR 0017).
- HTTP and MCP are peer inbound adapters over the same application owners.
  Neither owns a second business model, and the GUI never calls loopback MCP.
- `ARCHITECTURE.md` maps owners and ADRs. Changing inventories, versions, and
  phase history belong with source or dated evidence, outside root guidance.

## Persistence and lifecycle

- Any released or potentially persisted schema, marker, fingerprint,
  constraint, index, trigger, serialization, or encoding change uses
  `onecrew-local-data-evolution`.
- Absence of a migration request is not evidence that persisted data cannot
  exist. Require positive release and storage evidence before reset-only work.
- Runtime open validates rather than migrates. Local-service startup owns any
  required automatic data upgrade after owner/old-process checks: preflight,
  backup, adjacent upgrades, validation, and typed recovery. Compatible data
  starts directly. Inspecting/Upgrading/Ready/Failed gates business; failures
  block business even after partial commits. Data status is read-only; no CLI,
  HTTP, MCP, or webpage action executes upgrades.
- Never repair state by changing a version marker alone.
- Preserve lifecycle ownership for the complete async operation; do not drop
  guards or owners before provider, supervisor, or upgrade work ends.
- Blocking runtime work stays off async workers. Creator read/write/CPU segments
  acquire bounded admission before blocking dispatch; repository locks cover
  one bounded call and never provider I/O. See `onecrew-local-service`.

## Security and external effects

- Persist durable secrets only through `local-vault`; keep public DTOs, logs,
  audits, fixtures, screenshots, and status responses secret-free.
- Keep OTPs and transient tokens in memory for the shortest necessary lifetime
  and never echo them. Accept a macOS login password only through the native
  Keychain authorization dialog.
- Treat loopback networking as an authenticated, validated input boundary.
- External writes use the owning runtime's trusted confirmation or grant,
  stable idempotency, and provider readback where supported. Existing user
  authorization does not bypass runtime enforcement, and request acceptance
  is not completion.
- Preserve provider IDs, provenance, unknown-effect states, and durable audit
  evidence through the owning runtime.

## Frontend boundary

- Use shared shadcn primitives and upstream headless behavior. Keep wire mapping
  and non-visual derivation in testable TypeScript; runtime projections own
  capability, freshness, and safety state.
- Route visible fixed copy, locale selection, and date/number formatting to
  `onecrew-i18n`. Use editable local TypeScript i18next catalogs for
  `zh-CN` and `en-US`; preserve runtime protocol identifiers. White/light Web
  surfaces and the canonical icon system belong to `onecrew-web-ui`.

## Skill system

- `.skillshare/skills` is the canonical discoverable catalog;
  `.agents/skills` and `.claude/skills` are relative symlink mirrors.
  Skill-governance entries live under `development/skills` and load by path.
- `development/lint/project-skill-ownership.json` is the single executable
  owner/relationship graph; routing examples live in the adjacent cases file.
- Read the matching `SKILL.md`; load references as needed. Descriptions must
  distinguish intent, and entries retain essential boundaries.
- Use `development/skills/onecrew-new-skill` for Skill lifecycle and
  taxonomy changes, not ordinary product implementation.
- Add cross-cutting Skills only for boundaries changed by the task; use the
  executable graph to resolve ownership.
- A Skill may narrow a workflow but may not weaken this constitution.

## Collaboration

- With user authorization, delegate broad exploration to read-only,
  non-overlapping explorers and wait instead of duplicating their search.
  Keep narrow known-owner edits local.

## Agent skills

- Issue tracker: local Markdown in `.scratch/`; see [tracker conventions](docs/agents/issue-tracker.md).
- Triage labels: use the [local status vocabulary](docs/agents/triage-labels.md).
- Domain docs: one root glossary and ADR directory; see [domain guidance](docs/agents/domain.md).

## Completion and verification

- Complete the authorized outcome and inspect its final diff. Use
  `onecrew-verify` for checks appropriate to the changed owners and
  `onecrew-review` when independent review is warranted. Do not repeat
  passing checks or review unchanged work without a concrete reason.
- Review the complete candidate, including staged and unstaged changes when
  present. A commit-range-only review does not cover uncommitted implementation.
- Report what changed, evidence of completion, and any unresolved limitation.
  If only unavailable native/live/release acceptance remains, state exactly
  what still needs verification; never report it passed. User acceptance closes
  the delivery; later feedback is a new change request.
- Wait for long-running jobs on their returned ID until terminal; silence or
  timeout is not failure. Terminate only for user request or verified resource
  or artifact risk, targeting that job; never use broad kill patterns.
- Run at most one Cargo build, test, or clippy command per target directory at
  a time.
- The check entry is `node development/scripts/agent-check.mjs`; verification
  guidance selects its profile. Before handoff inspect the diff, run
  `git diff --check`, and confirm changed paths with `git diff --stat`.
