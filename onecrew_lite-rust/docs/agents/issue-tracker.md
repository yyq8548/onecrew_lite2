# Issue tracker: Local Markdown

The user selected the repository's local Markdown tracker on 2026-09-14.
Specs and implementation issues live in `.scratch/` and are versioned with the repository.

- Use one feature directory: `.scratch/<feature-slug>/`.
- Publish the feature specification as `spec.md` in that directory.
- When tickets are requested, create one file per ticket under `issues/<NN>-<slug>.md`, starting at `01`.
- Put a `Status:` line near the top of each spec or issue, using [the status vocabulary](triage-labels.md).
- Put supporting source evidence in a separate file when useful; keep the spec focused on decisions and observable behavior.
- Append later discussion under `## Comments`; update the specification when a decision changes.

When a Skill says to publish to the issue tracker, create or update the corresponding local file.
When it says to fetch an issue, read the referenced local file. This configuration does not select an external tracker.

Use `ready-for-agent` once the specification is complete and any explicitly required user decision is resolved.
An implementation request still follows the user's scope and the repository constitution.

Use `done` after the implementation is committed and accepted: by the user, or by the coordinator
after an independent verification when the user has delegated acceptance.
Update the existing `Status:` line in place, and record the completing commit and the acceptance basis
under `## Comments`.
