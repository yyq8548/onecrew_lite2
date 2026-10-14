# MCP Failure Guidance

## Critical Rules

MCP has two failure boundaries: application failures returned after a handler
starts, and framework failures raised while parameters are extracted, results
are serialized, or the router dispatches. Both are adapter presentation
concerns. Neither may expose a lower-layer cause or rejected secret value.

```rust
// ❌ FORBIDDEN: a String or Display cause crosses the public MCP boundary.
let value = runtime.operation(input).await.map_err(|error| error.to_string())?;

// ✅ CORRECT: preserve typed application meaning, then project safe guidance.
match runtime.operation(input).await {
    Ok(value) => structured(value),
    Err(OperationError::ConfirmationRequired) => guided_failure(PublicFailure {
        code: "confirmation_required",
        message: "Explicit confirmation is required.",
        parameter_source: "this tool's confirmed field",
        next_action: "Review the target, then retry with confirmed=true.",
    }),
    Err(OperationError::UnknownEffect { audit_id }) => {
        guided_unknown_effect(audit_id) // preserves no-retry/readback semantics
    }
    Err(OperationError::Internal(_)) => guided_unknown_failure(),
}
```

The shared helper accepts a safe `PublicFailure`, never an error/cause or an
unbounded public string. Domain-specific callers own variant classification;
the helper only chooses and fills RMCP `CallToolResult::error` or
`structured_error`. It does not define a second MCP error protocol.
`structured_error` is valid only when its typed negative value conforms to the
registered operation's exact output schema. Otherwise the helper emits text error content
and no shared structured envelope.

## Quick Reference

| Failure source | Required projection |
|---|---|
| Typed runtime/application outcome | Match every variant; preserve confirmation, idempotency, blocked, recovery, readback, and unknown-effect meaning |
| Platform read failure (`PlatformReadError`) | Register the shared `read_failure_projection` for the binding's declared connection; never add a per-platform classifier |
| Legacy untyped runtime failure | Do not expose the string; use an operation-specific safe explanation and next action, then replace the owner error with a typed contract when semantics require branching |
| Unknown handler failure | Stable non-leaking code/message; no retry advice unless retry is known safe |
| Parameter extraction / schema rejection | Router/extractor sanitizer keeps public field names but removes rejected values and secret material |
| Result serialization / router failure | Safe framework code and recovery instruction; no DTO dump or lower-layer cause |
| Typed negative result that stops work | Keep the structured result when useful and add safe guidance content |
| Failure outside the tool output schema | RMCP text `CallToolResult::error`; never inject a universal structured error object |

## Common Patterns

### Shared failure directory, binding-owned classification

Keep one private crate-level MCP guidance directory with a thin facade:

```text
crates/mcp-adapter/src/failure/
  mod.rs          # private facade
  presentation.rs # PublicFailure -> RMCP CallToolResult projection
  framework.rs    # RMCP ErrorData project policy
```

It defines thin policy over RMCP result/error primitives, but knows nothing
about the Creator owner, providers, or operation policy. Each binding matches its owner error
variants and supplies the safe code, explanation, public parameter source, and
next step. Successful values keep their typed `structuredContent`; shared
helpers add text content only when guidance is useful. Tests stay beside the
owned presentation/framework module rather than accumulating in one test file.
Do not refactor every owner into one common error enum: add owner typing only
where that workflow has real semantic branches the handler must preserve.

Never implement a blind sanitizer that takes `impl Display`: redaction after
formatting cannot prove a cookie, API key, SQL value, provider body, or vault
path was absent. Unknown causes stay below the boundary.

### Platform reads share one protocol-semantic projection

Every platform read reports the runtime's `PlatformReadError`: local variants
name OneCrew's own outcomes, and a platform failure carries the platform
crate's protocol-native evidence (`DmDiagnostic`). Each platform read binding
registers `read_failure_projection` from
`domains/platform_reads/read_failure.rs` for the connection it declares. The
projection never branches on the platform, and the adapter neither inspects
platform, client or transport errors nor depends on a platform crate (ADR 0017).

| Outcome | Projection |
|---|---|
| Local variant | Its fixed public code and recovery; search-plan and connection refusals reuse the shared projections |
| Platform failure without any platform response (provider origin, no HTTP status, RPC code or native code) | `network_unreachable`, recovered by the declared toolkit's connection check |
| Any other platform failure | `provider_read_failed`; HTTP status or RPC code 401 reconnects, 420 or 429 retries once after at least 60 seconds, 5xx after at least 30; local evidence and other codes get no recovery |

A platform failure is known-no-effect and retry-safe unless it follows a local
commit, such as a YouTube credential health mark, which makes it applied and
not retry-safe. It carries the evidence message, the HTTP status as
`provider_status`, and the evidence itself as `provider_evidence`. The frozen
presentation fixture beside the platform read tests pins the exact rows; a new
or finer platform failure changes only the platform crate's evidence.

### Preserve recovery and observability

The adapter presentation never replaces runtime audit, durable claims,
readback, or recovery. Unknown-effect remains a distinct no-blind-retry path;
confirmation and idempotency remain distinct public failures. Adapter logs and
metrics record only the safe code, public meta-tool name, and validated
operation ID. The owning runtime
keeps whatever secret-safe durable evidence its contract already requires.

### Framework errors are not handler errors

`Parameters<T>` may fail before handler code runs, and result serialization
may fail after it returns. Native RMCP remains the outer meta-tool dispatcher: construct the
context and call `ToolRouter::call`, which owns disabled tools, unknown tools,
route invocation, parameter extraction, and its model-correcting argument
errors. Never copy its `has_route`, route-map lookup, or `route.call` sequence.
Inside `onecrew_tool_execute`, registry lookup, surface authorization, schema
validation, typed decoding, and binding dispatch begin only after the handler
starts; that is not a copied RMCP router.

A bounded schema prevalidation or extractor hook may reject secret-bearing
values before native dispatch. After native dispatch, preserve the safe
protocol `ErrorCode` and replace only unsafe message/data; only a genuine
internal or result-serialization failure becomes `INTERNAL_ERROR`. Tests
submit sentinel credentials in wrong types and unknown fields and assert the
sentinel is absent from protocol error data, content, logs, fixtures, setup,
and health output without changing unknown-tool or invalid-parameter semantics.

### Guidance stays executable

When an ID is wrong or missing, name the public field, the authoritative
`onecrew_tool_get` operation contract that supplies it, and the next
`onecrew_tool_execute` call. For writes, state the
confirmation/idempotency or readback requirement. Do not tell an Agent to
derive an internal ID, inspect local storage, retry an unknown-effect action,
or use a tool absent from the current fixed surface.

## Checklist

- [ ] No MCP handler returns a lower-layer `String`, uses `to_string()` on a cause, or applies `?` to an unclassified runtime/provider failure.
- [ ] Typed failure variants preserve confirmation, idempotency, blocked, unknown-effect, recovery, audit, and readback semantics.
- [ ] Business classification remains in the owning operation binding (or a private
      helper beside it), never in `failure/`.
- [ ] Platform read bindings use the shared read failure projection, with no
      per-platform classifier, platform error type, or platform crate import.
- [ ] The private shared helper accepts only bounded safe public fields, never an arbitrary error or display string.
- [ ] `failure/mod.rs` stays a thin private facade; `presentation.rs` only wraps
      RMCP `CallToolResult`, while `framework.rs` only wraps RMCP `ErrorData`.
- [ ] The project policy uses RMCP primitives directly and defines no parallel
      error protocol or universal business classifier.
- [ ] Unknown failures have a stable non-leaking fallback and no unsafe retry advice.
- [ ] Extractor/router/serialization `ErrorData` is sanitized outside handlers.
- [ ] Outer meta-tool dispatch calls native `ToolRouter::call`; no project code
      repeats route existence, disabled-tool, map lookup, extraction, or
      invocation. Internal registry dispatch starts only inside execute.
- [ ] Safe framework projection preserves the original `ErrorCode` unless the
      failure is genuinely internal or result serialization failed.
- [ ] Secret-sentinel tests cover deserialize-only inputs and rejected values.
- [ ] Actionable guidance names a real catalog entry/result source and next meta-tool step available on the same fixed surface.
- [ ] `structured_error` values conform to the operation's registered output schema;
      all other failures use text error content without a universal envelope.
- [ ] Runtime/domain DTOs and durable facts contain no MCP tool names.
- [ ] Safe code, public meta-tool name, and validated operation ID are sufficient for adapter observability; raw causes are not logged.
- [ ] Surface, schema, content, protocol-error, and failure-path tests pass.
