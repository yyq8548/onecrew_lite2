import { createHash } from "node:crypto";
import { inspect } from "node:util";

const issued = new WeakSet<OwnerReviewContext>();
const maxPeerResumeBytes = 64 * 1024 * 1024;

function identifier(value: string, field: string): string {
  if (!/^[A-Za-z0-9._:-]+$/.test(value) || Buffer.byteLength(value) > 128) throw new Error(field);
  return value;
}

export interface OwnerFrame {
  outcomes: readonly { call_id: string; operation_id: string; disposition: { kind: string } }[];
}

// 由完成合同解码的协议适配器创建；操作输入不能反序列化为此对象。
export class OwnerReviewContext {
  readonly origin: string;
  readonly call_id: string;
  readonly operation_id: string;
  readonly call_digest: string | null;

  constructor(origin: string, callId: string, operationId: string, exactPeerCall?: Uint8Array) {
    this.origin = identifier(origin, "origin_scope");
    this.call_id = identifier(callId, "call_id");
    this.operation_id = identifier(operationId, "operation_id");
    this.call_digest = exactPeerCall === undefined ? null : `sha256:${createHash("sha256").update(exactPeerCall).digest("hex")}`;
    issued.add(this);
    Object.freeze(this);
  }

  validateFrame(frame: OwnerFrame): void {
    OwnerReviewContext.assertIssued(this);
    const call = frame.outcomes.find(value => value.call_id === this.call_id);
    if (!call) throw new Error("owner_review_call_missing");
    if (call.operation_id !== this.operation_id || call.disposition.kind !== "not_run") throw new Error("owner_review_call_mismatch");
  }

  validatePeer(frame: OwnerFrame, peer?: PeerResumePayload): void {
    this.validateFrame(frame);
    if ((this.call_digest !== null) !== (peer !== undefined)) throw new Error("owner_review_peer_context");
    if (peer === undefined && frame.outcomes.length !== 1) throw new Error("owner_review_peer_required");
    if (peer !== undefined && !(peer instanceof PeerResumePayload)) throw new Error("owner_review_peer_context");
  }

  static assertIssued(value: OwnerReviewContext): void {
    if (!issued.has(value)) throw new Error("owner_review_context_untrusted");
  }
}

export class PeerResumePayload {
  readonly #bytes: Buffer;

  constructor(bytes: Uint8Array) {
    if (bytes.byteLength === 0 || bytes.byteLength > maxPeerResumeBytes) throw new Error("peer_resume_payload");
    this.#bytes = Buffer.from(bytes);
    Object.freeze(this);
  }

  copyBytes(): Buffer { return Buffer.from(this.#bytes); }
  toJSON(): never { throw new Error("Peer continuation must not be serialized"); }
  [inspect.custom](): string { return `PeerResumePayload { bytes: ${this.#bytes.byteLength} }`; }
}
