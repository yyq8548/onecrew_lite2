# MCP App UI

`onecrew_lite-mcp-ui` owns the React documents served inside MCP Apps iframes. It is
an independent application package: the Desktop may host the generated
document, and the MCP adapter may embed it, but neither is a source or runtime
dependency of this package.

## Boundary

The package receives an initial MCP Apps tool result through the official SDK,
parses the closed result contract, reads or mutates state through the exact
App-only descriptor tool, and renders the resulting projection. Runtime policy,
interaction authority, continuation leases, and external effects remain with
their Rust owners; the App only projects typed states and acknowledged Host
outcomes.

Source dependencies point inward from widget entries to `app`, feature
controllers/views, result clients, and pure contracts. The executable import
boundary in `build.mjs` rejects Desktop paths and retired aliases. Package
tests own their fixtures and Host mocks and run without the Desktop test suite.

## Choice flow and language

Choice questions are answered one at a time, with paged options or custom text.
Back and review edits retain local drafts; only the final review page submits
the complete answer set. Custom answers are limited to 2,048 UTF-8 bytes each,
and the serialized answer set is limited to 24 KiB. Choice completion records
clarification answers and does not authorize a downstream operation.

Fixed copy lives in `src/locales/en-US` and `src/locales/zh-CN`. Each snapshot
carries the native runtime's shared presentation locale, initialized from the
Desktop language preference. Focus and visibility refreshes read that locale
without changing result identity or discarding drafts. Questions, options, and
user-entered text remain unchanged. Server-owned continuation copy has separate
locale resources in the MCP adapter.

The App sends model context only when the server marks it required
(`requires_model_context`), because Hosts may show that context to the user;
the Host must then accept it before the App sends the server's message
verbatim. Missing or failed required context delivery pauses the continuation
instead of sending an incomplete answer. The App starts the continuation in
the same async chain as the card action. Host mocks cover this boundary; native
Host acceptance is a separate verification step.

## Build output

`build.mjs` discovers widget entries, produces one self-contained HTML document
for each, enforces source and size contracts, and writes the committed assets
consumed by `mcp-adapter`. Paths derive from the script location, so invoking
the package through `npm --prefix` or from its own directory has the same
result. Generated HTML is replaced by the build and is not an editing surface.

## Failure model

Missing, malformed, expired, mismatched, terminal, or unknown result authority
stays actionless. Host response mismatches are classified without response
values; they do not broaden the parser or guess a server namespace. A pending
continuation is claimed once, sends only server-owned message/context, and is
acknowledged as delivered, paused, or delivery-unknown without blind retry.

Package tests, typecheck, build budgets, deterministic asset regeneration, and
affected adapter/Host consumers are the executable acceptance evidence.
