---
name: onecrew-i18n
description: Maintain Web translation catalogs, language selection, and locale-aware display. Excludes translating user/business data and MCP iframe internals.
---

# OneCrew i18n

Keep Web copy and formatting consistent with the selected language while preserving business data and wire values.

- Use the existing `desktopI18n` instance and typed, editable TypeScript catalogs. Preserve initialization before React mount and the explicit `I18nextProvider`; do not introduce another translation stack or a generated-catalog workflow.
- Translate product-owned labels, placeholders, tooltips, toasts, empty/error states, and accessible names. Keep runtime enum values, IDs, URLs, provider diagnostics, user content, and saved/template execution prompts as data. A separate user request to translate that data defines its own scope.
- Pair with `onecrew-web-ui` when editing React copy, presentation, or locale-dependent rendering. A catalog-only wording edit does not require a UI owner.
- Keep this instance and its catalogs inside Web. MCP App localization belongs to `onecrew-mcp-app-ui`; Host locale contract changes also require `onecrew-peer-adapters`.

Read [Catalogs and locale](references/catalogs-and-locale.md) when changing keys, interpolation, plurals, initialization, or language selection. Read [Display boundaries and verification](references/display-boundaries-and-verification.md) when changing formatters, rendered copy, error presentation, or choosing validation.

## Related Skills

- onecrew-web-ui — pair for React or locale-dependent rendering changes.
- onecrew-mcp-app-ui — hand off iframe localization to its package owner.
- onecrew-peer-adapters — pair only when a public Host locale contract changes.
- onecrew-verify — select broader evidence when the affected consumers require it.
