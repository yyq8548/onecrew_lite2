# Catalogs and locale

All source paths below are relative to the repository root.

## Editable source and typed keys

Catalogs live in `apps/web/src/i18n/locales/{zh-CN,en-US}/{namespace}.ts`.
Search the owning namespace for an existing semantic key before adding one;
share only wording with the same meaning and context. These are maintained
source modules, not generated JSON or remote translation-service output.

Chinese defines key shape; English uses `as const satisfies CatalogShape<typeof
zhCN…>`. Keep both catalogs aligned when adding or removing keys, including
nested interpolation tokens. `i18next.d.ts` enables `strictKeyChecks`; do not
silence a missing key with casts, untyped lookups, or inline fallback copy.
Namespaces and resource composition belong to `i18n/resources.ts` and each
locale's `index.ts`, not to a component-local dictionary.

React consumers use `useTranslation` with the owning namespace. Non-React
helpers use the existing instance or accept a translator/locale at their
boundary; avoid translating once at module load because that freezes the
language. Preserve intentional pre-initialization fallback behavior in shared
helpers unless changing that behavior is the task.

Use complete messages with named interpolation rather than concatenated
translated fragments. For plural wording, use i18next `count` and the existing
`_one`/`_other` key convention, maintaining catalog key parity. Email-template
syntax such as `{{first_name}}` can be literal product content: preserve it
through interpolation rather than renaming or consuming the template variable.
`components/mail/mail-i18n.test.ts` exercises both plural selection and literal
template placeholders.

## Startup and switching

`src/main.tsx` awaits `initializeI18n()` before mounting React and passes
`desktopI18n` to `I18nextProvider`. `i18n/index.ts` owns initialization and
document `lang` updates. Keep those responsibilities explicit.

`i18n/locale.ts` resolves initial language in this order:

1. Exact supported saved preference (`onecrew-locale`).
2. Normalized HTTP OS locale.
3. First supported normalized navigator language.
4. `zh-CN` fallback.

The current supported values are `zh-CN` and `en-US`. Environment detection
normalizes language variants; saved preferences must match an actually shipped
locale exactly. Storage or OS-detection failures must not prevent startup.
`components/language-switcher.tsx` persists the preference and changes the
existing instance's language. Do not turn switching into a page reload or
silently persist an environment-derived preference.

## Source anchors

| Concern | Source under `apps/web/src/` |
|---|---|
| Resource shape and key typing | `i18n/resources.ts`, `i18n/catalog-shape.ts`, `i18n/i18next.d.ts` |
| Editable English catalog pattern | `i18n/locales/en-US/common.ts` |
| Key and interpolation parity | `i18n/resources.test.ts` |
| Detection and boot contract | `i18n/locale.test.ts`, `i18n/bootstrap-contract.test.ts` |
