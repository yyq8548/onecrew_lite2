import { randomUUID } from "node:crypto";
import { resultContinuationRequest, type ResultContinuationRequest } from "./contracts.ts";

type ContinuationState = "not_requested" | "pending" | "dispatching" | "delivered" | "paused" | "delivery_unknown";
function increment(value: bigint): bigint {
  if (value < 1n || value >= 18446744073709551615n) throw new Error("continuation_revision");
  return value + 1n;
}

export class ResultContinuation {
  #state: ContinuationState = "not_requested";
  #revision = 1n;
  #request: ResultContinuationRequest | null = null;
  #lease: string | null = null;
  #startedAt: number | null = null;
  #notAfter: number | null = null;
  #deadline: number | null = null;

  apply(request: ResultContinuationRequest | null): void {
    const parsed = request === null ? null : resultContinuationRequest.parse(request);
    const revision = parsed === null ? this.#revision : increment(this.#revision);
    this.#request = parsed;
    this.#revision = revision;
    this.#state = parsed === null ? "not_requested" : "pending";
    this.#lease = null; this.#startedAt = null; this.#notAfter = null; this.#deadline = null;
  }
  snapshot() {
    this.recoverExpired();
    return { state: this.#state, continuation_revision: this.#revision, dispatch_started_at_ms: this.#startedAt, dispatch_not_after_ms: this.#notAfter };
  }
  claim(expectedRevision: bigint, mode: "pending" | "retry_paused") {
    this.recoverExpired();
    this.#validateRevision(expectedRevision);
    if (this.#state === "paused" && mode !== "retry_paused") throw new Error("continuation_retry_requires_explicit_mode");
    if (!((this.#state === "pending" && mode === "pending") || (this.#state === "paused" && mode === "retry_paused"))) throw new Error("continuation_not_claimable");
    if (this.#request === null) throw new Error("continuation_request");
    const revision = increment(this.#revision), startedAt = Date.now();
    if (!Number.isSafeInteger(startedAt) || startedAt < 0 || !Number.isSafeInteger(startedAt + 120000)) throw new Error("continuation_clock");
    const lease = `rcl_${randomUUID().replaceAll("-", "")}`;
    this.#revision = revision; this.#state = "dispatching"; this.#lease = lease;
    this.#startedAt = startedAt; this.#notAfter = startedAt + 120000; this.#deadline = performance.now() + 120000;
    return { continuation_revision: revision, lease_id: lease, request: structuredClone(this.#request), dispatch_started_at_ms: startedAt, dispatch_not_after_ms: startedAt + 120000 };
  }
  complete(expectedRevision: bigint, lease: string, outcome: "delivered" | "paused" | "delivery_unknown") {
    this.recoverExpired();
    this.#validateRevision(expectedRevision);
    if (this.#state !== "dispatching") throw new Error("continuation_not_dispatching");
    if (lease !== this.#lease) throw new Error("continuation_lease_mismatch");
    if (!["delivered", "paused", "delivery_unknown"].includes(outcome)) throw new Error("continuation_outcome");
    const revision = increment(this.#revision);
    this.#revision = revision; this.#state = outcome; this.#lease = null; this.#deadline = null;
    if (outcome === "delivered") this.#request = null;
    return this.snapshot();
  }
  recoverExpired(): void {
    if (this.#state === "dispatching" && this.#deadline !== null && performance.now() >= this.#deadline) {
      const revision = increment(this.#revision);
      this.#revision = revision; this.#state = "delivery_unknown"; this.#lease = null; this.#deadline = null;
    }
  }
  heldRequest(): ResultContinuationRequest | null { return structuredClone(this.#request); }
  #validateRevision(expected: bigint): void {
    if (typeof expected !== "bigint" || expected < 1n || expected > 18446744073709551615n) throw new Error("continuation_revision");
    if (expected !== this.#revision) throw new Error("continuation_revision_conflict");
  }
}
