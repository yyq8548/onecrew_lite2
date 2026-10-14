# Display boundaries and verification

All source paths below are relative to the repository root.

## Localized presentation, unchanged data

Map fixed UI choices to translated display labels while preserving the values
sent to runtime. Do not translate IDs, provider codes, URLs, creator names,
user-authored messages, email subjects/bodies, stored templates, or execution
prompts merely because their surface is being localized.

`apps/web/src/lib/provider-error.ts` renders structured owner diagnostics with
localized labels while preserving their protocol, origin, code and stage.
Never infer a cause or recovery step from human text, translate native data,
or publish unstructured error bodies. `src/pages/tasks-common-templates-i18n.test.ts` protects translated picker
copy while keeping the selected template's execution prompt unchanged.

Dates, compact counts, and calendar labels must follow the app language, not a
hard-coded locale or implicit browser default. Reuse `formatDate`/`formatCount`
in `apps/web/src/lib/creator.ts` where their semantics fit. Calendar and date
filters already map the resolved language in `src/components/ui/calendar.tsx`
and `src/components/data-table/date-filter.tsx`. Preserve epoch units, time-zone
semantics, and machine-readable date values while changing display formatting.

Ensure language changes rerender consumers and invalidate locale-dependent
memoized display values. Include the translator/resolved language in relevant
memo dependencies; do not rely on a formatter reading the singleton to trigger
React rendering. Check both languages in constrained layouts; fix wrapping,
spacing, and control sizing rather than shrinking fonts to hide longer copy.

## Proportional validation

Use the existing Web test runner from the repository root:

```sh
npm --prefix apps/web test -- src/i18n/resources.test.ts
npm --prefix apps/web run typecheck
```

These cover catalog parity and typed key use for catalog-only edits. Do not
add wording-matching tests or require a full app test/build cycle for a small
copy correction. Select additional existing tests only for the changed behavior:

| Change | Relevant evidence |
|---|---|
| Detection, persistence, provider bootstrap | `src/i18n/locale.test.ts`, `src/i18n/bootstrap-contract.test.ts` |
| Formatters, calendar, language switching | `src/lib/locale-formatting.test.tsx` plus affected rendering |
| Mail plurals or literal template variables | `src/components/mail/mail-i18n.test.ts` |
| Error diagnosis or template picker behavior | `src/lib/provider-error.test.ts`, `src/pages/tasks-common-templates-i18n.test.ts` |

For React changes, check visible and accessible copy in both languages, including
the affected empty/error/disabled state. Use the Web UI observation guidance
when the claim depends on actual overflow, focus, or window layout. A passing
typecheck does not prove those observations; report unobserved behavior plainly.
Broader build evidence follows `onecrew-verify` when the final diff needs it.

## Scope examples

- “Correct the English retry label.” Edit the existing English catalog entry;
  keep its key and interpolation contract, run catalog/type checks, and do not
  recruit the UI owner or change runtime error handling for this wording edit.
- “Show English dates and selected-row counts after switching languages.” Pair
  with Web UI; use the active locale, `count` plural messages where needed,
  and reactive display dependencies. Verify both languages without changing
  stored timestamps or selection values.
- “Localize the MCP selection card.” Route to MCP App UI and keep any translator,
  catalogs, and dependencies package-local. Do not import Web i18n or rewrite
  the server-owned continuation message. Pair with Peer Adapters if Host locale
  projection needs a public contract change.
