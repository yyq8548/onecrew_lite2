# Initial match evaluation

Implementation reference: [upstream rubric at 0cebf122](https://github.com/onecrewai/onecrew-plugins/blob/0cebf1220c15f4252533e6b4a6bddb079cfdc101/plugins/onecrew/skills/onecrew/references/match-evaluation.md).

Evaluate the current objective, explicit requirements, exclusions and preferences.
Do not invent weights or extra thresholds. Requirements are gates; preferences
cannot override their failure.

Assess each relevant fact as supported, contradicted or unknown. A confirmed gate
failure or exclusion means low. Otherwise an unresolved gate or insufficient
core evidence means no score. Missing optional facts do not impose a penalty.

With gates satisfied, use excellent for strong fit with distinct direct
observations, good for clear but narrower support, medium for evidenced partial
fit and low for evidenced weak fit. An initial assessment has no grade ceiling.
Saved search/profile content can suffice; a name or generic tag cannot.

Persist a level and a concise reason describing observations, implications and
limits; do not claim the sample covers all posting history. Retain unrated and
low records. Repeated discovery preserves scores; explicit reassessment replaces
level and reason together. Rank excellent, good, medium, low, then unrated before
pagination. Keep the existing Campaign table.

For this workbench's typed submission contract and shortlist behavior, follow
[Campaign discovery](campaign-discovery.md#initial-matching-and-shortlist).
