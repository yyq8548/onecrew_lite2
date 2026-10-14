# Local status vocabulary

Use these values in the `Status:` line of a local specification or issue.
They provide the vocabulary required by `to-spec` and `to-tickets`; the separate `triage` Skill is not required.
`done` has no Skill role; it is a local-only closing status.

| Skill role | Local status | Meaning |
| --- | --- | --- |
| `needs-triage` | `needs-triage` | Scope or priority still needs evaluation. |
| `needs-info` | `needs-info` | A named required decision or fact is outstanding. |
| `ready-for-agent` | `ready-for-agent` | Specified and ready for the authorized implementation workflow. |
| `ready-for-human` | `ready-for-human` | Requires human implementation. |
| `wontfix` | `wontfix` | Will not be implemented. |
| — (local only) | `done` | Implemented and accepted; the completing commit and the acceptance basis are recorded. |
