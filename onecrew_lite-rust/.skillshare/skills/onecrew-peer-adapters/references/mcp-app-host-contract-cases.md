# MCP Apps Host Contract Cases

Use this pattern when an MCP App works through a direct client or one Host but
fails after `tools/call`, remount, or a user transition in another Host.

## Critical Rules

### Optional is not explicit null

An MCP Apps Host may preserve an optional field as explicit `null` or omit it.
The consumer implements the registered JSON Schema, not the one serialization
shape produced by a direct Rust test.

```ts
// ❌ BAD: Option<T> is optional in the schema, but the parser requires the key.
const snapshot = exactRecord(value, [
  "binding",
  "pending_action",
  "continuation",
]);

// ✅ GOOD: required and optional keys stay closed, then omission is normalized.
const snapshot = exactRecordWithOptional(
  value,
  ["binding", "continuation"],
  ["pending_action"],
);
const pendingAction = snapshot?.pending_action ?? null;
```

Normalization is not an open-object fallback. Require every mandatory field,
allow only declared optional fields, reject unknown keys, and validate the
normalized value with the same semantic invariants. Apply this recursively to
nullable projection fields such as summaries, headings, labels, descriptions,
pending actions, and continuation timestamps.

```text
❌ BAD
Rust serialization with explicit null passes
→ test declares the wire safe
→ Host omits null on an App-only response
→ card reports one generic invalid_response

✅ GOOD
tools/list proves required/optional schema
→ tests cover explicit null and omitted property
→ available card transitions through confirm/select/cancel/edit
→ terminal snapshot normalizes missing optional values
→ unknown fields and inconsistent state still fail closed
```

Authority-bearing fields such as result reference, origin, view identity,
fingerprint, and revision remain required and exact.

## Quick Reference

| Symptom | Required response |
|---|---|
| Direct client passes, one Host fails | Isolate the Host bridge before production edits |
| Optional null field disappears | Normalize omission to null, retain closed keys and semantics |
| Demo advertises empty properties | Fix the actual `tools/list` schema before trusting the result |
| Unknown message ID after success | Remove the second JSON-RPC client from the iframe |
| Generic invalid response | Classify a safe boundary/field group without logging payload values |

## Common Patterns

### Isolate the Host boundary

When a failure exists only in one Host, isolate one variable before editing
production:

1. Build a disposable server and App with the same transport, `ui://` resource,
   App-only visibility, and `tools/call` path, but no product runtime.
2. Establish a simple object-schema baseline, then add the production-shaped
   nested result. Inspect the real `tools/list`; the wrong schema helper form
   can advertise `{ type: "object", properties: {} }` and create a false-positive
   demo.
3. Use either the MCP Apps SDK or one hand-written JSON-RPC client in an iframe,
   never both. Two clients consume the same response IDs and can manufacture an
   `unknown message ID` error after a successful server result.
4. Classify failures by boundary (`transport`, root result, snapshot field
   group, semantic consistency) using bounded safe categories. Never display or
   log the raw result, binding, fingerprint, provider payload, or rejected
   secret value.
5. Compare the opening result, App-only result, current production parser, and
   exact served asset. Keep a production change only after an observation names
   the failing boundary.

## Checklist

- [ ] Validate the actual `tools/list` input/output descriptors, not only the source
  type or local serializer.
- [ ] Exercise the same App-only tool through a real MCP Apps bridge.
- [ ] Cover missing and explicit-null forms for every optional field.
- [ ] Cover the available-to-terminal transition after confirm, select, cancel, and
  Agent-edit actions, because terminal projection often removes pending state.
- [ ] Reject unknown fields, malformed authority bindings, stale revisions, and
  semantically inconsistent state after normalization.
