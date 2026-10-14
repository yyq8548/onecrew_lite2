# Capability Placement and Wiring

Choose the canonical owner from dependency, protocol, trust, lifecycle,
persistence, transaction, and reusable-facade evidence. Confirm every path
against current source before relying on it.

## Critical Rules

Inbound adapters stop at one application owner. Provider mechanics remain
below the facade, and peer adapters own no business policy.

```rust
// ❌ FORBIDDEN: HTTP reaches transport and invents a second write path.
async fn send(input: SendInput) -> Result<SendResult, String> {
    let session = ProviderSession::from_secret(input.secret)?;
    transport_send(&session, input).await
}

// ✅ CORRECT: HTTP maps input and calls the typed application owner once.
async fn send(state: &AppState, input: SendInput) -> Result<SendResult, String> {
    state.platform().await?.creators()?.send(input).await
}
```

Do not create a generic model merely because several providers or surfaces can
be listed together. Preserve each real identifier and typed follow-up contract.

```rust
// ❌ FORBIDDEN: universal identity and arbitrary config erase real owners.
struct Connection { provider: String, connection_id: String, config: Value }

// ✅ CORRECT: a bounded index keeps platform-specific summaries and IDs.
struct ConnectionIndex {
    operator_accounts: Vec<OperatorAccountSummary>,
    service_credentials: Vec<ServiceCredentialSummary>,
}
```

Secrets remain in `local-vault`; public boundaries carry opaque references and
safe status only.

```rust
// ❌ FORBIDDEN
struct AccountProjection { cookies: Vec<Cookie>, passcode: Option<String> }

// ✅ CORRECT
struct AccountProjection {
    operator_account_id: String,
    credential_availability: CredentialAvailability,
}
```

## Quick Reference

| Concern | Canonical owner |
|---|---|
| Creator use case, canonical DTO, transaction, confirmation, idempotency, audit | `creator-runtime` |
| Shared operator/service-credential workflow and secret-free projection | `platform-runtime` |
| Shared provider-session restoration and session-cache ownership | `platform-sessions` |
| Deterministic public-schema canonicalization and cross-language manifest identity | `schema-manifest` |
| Codex CLI protocol, supervision, plugin staging/install cache | `codex` |
| Credential, cookie, passcode, API key, encrypted material | `local-vault` |
| Wire/auth/retry/session/signing mechanics | matching transport |
| Typed provider parsing and operation orchestration | matching client |
| Stable application-facing provider capability | matching provider facade |
| Typed toolkit reads and fresh save preparation from direct candidate inputs | `platform-runtime` |
| Email gateway and conversation read-port implementation over shared credential/session owners | `platform-runtime` |
| Agent-facing schema, registry, resource, and failure projection | `mcp-adapter` |
| Web command/process/lifecycle projection | Web HTTP shell |
| Web React composition | Web UI owner |
| Web locale selection, fixed-copy catalogs, and locale-aware formatting | Web i18n owner |
| MCP App iframe source, Host SDK state, and self-contained bundle | MCP App UI owner |
| Released or potentially persisted format | local-data-evolution owner |

## Common Patterns

### Boundary evidence

A new crate, module, port, repository, or adapter needs one independently
meaningful boundary and a real caller:

- dependency inversion around a narrow contract;
- protocol churn below a stable API;
- trust ownership and secret-free projection;
- construction, cancellation, cleanup, or shutdown lifecycle;
- a reusable typed facade with multiple real consumers;
- durable atomic state requiring a repository/transaction owner.

File length, endpoint count, naming symmetry, or a hypothetical future caller
is not boundary evidence. When owner and dependency direction are already
settled, hand a structure-only split to Rust architecture instead.

### Canonical application path

For migrated reads and Creator save preparation, record the actual chain:

```text
HTTP ─┐
       ├-> platform-runtime toolkit function -> provider facade -> client -> transport
MCP ───┘          |
                  +-> save preparation -> immutable typed review
                       -> explicit confirmation -> creator-runtime transaction
```

Save preparation accepts the selected locator, connection, real provenance and
judgement directly and validates them at the input boundary. It reads fresh
provider data and link hubs, then holds the exact typed review in the result
owner without writing Creator rows, snapshots or save reservations. Confirmation
consumes that immutable payload without refetching. Do not insert a candidate
cache, cache-key prerequisite or cross-platform selection framework into this
chain. Legacy search still writes snapshots until its migration; historical
tables remain until the backed-up v36-to-v37 upgrade drops them. Search and
outreach bridges and the email and DM ports remain until their respective
migrations; inspect current source before moving them. No adapter or background
supervisor bypasses the facade.

### Public identity

Use stable keys for resources persisted by a prior step and let the owner
resolve the authoritative snapshot. Keep a value inline only when it is
self-contained, boundary-validated, stable across callers, does not duplicate
persisted state, and cannot express inconsistent workflow facts.

### External-write evidence

Keep the sequence with its owner: explicit confirmation; stable idempotency;
provider execution; readback or typed unsupported-readback; atomic durable
state, idempotency completion, and audit; secret-free result with verification,
warnings, unknown-effect state, and next action. HTTP/MCP only project it.

### Secret and change-scope classification

Classify every sensitive input as transient secret, durable secret, opaque
reference, or public projection. Decrypt only for the operation that needs it.
Emit change scope only after authoritative state commits; never include raw
provider or secret payload. Verify cancellation, failure, expiry, disconnect,
and shutdown cleanup for new secret state.

### Related boundaries

Placement ends after naming the canonical owner and handoffs. Use peer-adapters
for public projections, local-data-evolution for persisted identity,
native-desktop for OS/distribution mechanics, and Rust architecture for module,
API, supervisor, or transaction structure after placement is settled.

## Checklist

- [ ] Owner and dependency direction follow source evidence, not file size.
- [ ] Every new boundary names one invariant and one real caller.
- [ ] Transaction, persistence, lifecycle, and trust owners are explicit.
- [ ] Provider mechanics stay below the facade.
- [ ] HTTP and MCP project the same use case and never call each other.
- [ ] Public identity preserves the real domain key; no arbitrary config model appears.
- [ ] External-write confirmation, idempotency, readback, unknown effect, and audit stay with the owner.
- [ ] Secrets appear in no public DTO, error, log, fixture, or change event.
- [ ] Placement stops once owner and handoffs are unambiguous.
