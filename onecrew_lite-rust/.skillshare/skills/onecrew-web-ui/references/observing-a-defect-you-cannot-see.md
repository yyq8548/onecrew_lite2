# Observe browser defects before editing

A visible scroll, focus, layout, or event defect requires an observation in the
running webpage before changing code. Record route, viewport, relevant data,
interaction sequence, expected result, and actual symptom. Source inspection
alone does not establish a runtime layout cause.

Use the browser's development tools or an authorized browser automation surface.
The frontend lives under apps/web and talks through a typed HTTP client. Reproduce
with the normal local service and web build; a frontend development server requires
the configured service connection. Native WebView debug flags do not apply.

Inspect one hypothesis at a time: element geometry and overflow ownership for
scroll issues, active element and focus transfer for overlays, and request/event
ordering for stale state. Keep instrumentation narrow and secret-free. Do not
record tokens, account credentials, or customer content in logs or screenshots.

Fix the observed mechanism in its owner: pure state/derivation helpers, query
invalidation, shared controls, or feature composition. A stalled HTTP request may
require the local-service owner instead of a presentation workaround.

Repeat the exact interaction on the changed webpage, verify accessibility and
narrow-viewport behavior, and remove temporary instrumentation. Report the
observation and focused tests. If no running browser is available, report that
visual/runtime acceptance is blocked rather than claiming a speculative fix.
