# Skills Token Monitoring

Agent Platform Local keeps project skills small enough for progressive loading
and splits a skill only when its topics have independent triggers and
workflows. The analysis covers both `.skillshare/skills` (discoverable) and
`development/skills` (skill-governance, loaded on demand).

## Commands

```bash
uv run development/skills-analysis/analyze-skills-tokens.py --sort-by-size --check
uv run development/skills-analysis/analyze-skills-tokens.py --detailed
```

Run both the context lint and token analysis after creating, merging,
splitting, or substantially expanding a project skill:

```bash
uv run development/lint/agent-context.py
uv run development/skills-analysis/analyze-skills-tokens.py --sort-by-size --check
```

## Thresholds

| Surface | Threshold | Action |
|---|---:|---|
| `SKILL.md` | `< 5,000` estimated tokens | Required; trim or route conditional detail |
| `SKILL.md` | `<= 200` lines | Default entry budget |
| One reference | `> 10,000` UTF-8 bytes | Review whether distinct triggers, owner, or verification justify a split |
| Whole bundle | `< 2,000` estimated tokens | Good |
| Whole bundle | `2,000-5,000` | Monitor growth |
| Whole bundle | `5,000-10,000` | Split only when topics are independently invoked |
| Whole bundle | `> 10,000` | Strong split signal, not permission to fragment one lifecycle |

Decide merge or split from concern and required context, not a frequency ratio
alone. An explicit owner-to-companion pair (for example, translation behavior
with React rendering) can recur without duplicating ownership. The executable
overlap check counts unexplained joint activation; handoff edges and helpers
co-selected by a third owner do not exempt it. Its threshold and routing cases
live in `development/lint/agent-context.config.json` and
`development/lint/project-skill-routing-cases.json`.

Shorten descriptions first when discovery costs grow: they are loaded before
the body is selected. Bundle totals are a maintenance signal, not an estimate
of every task's context cost; references should be loaded only when relevant.
