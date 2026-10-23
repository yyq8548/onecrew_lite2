import { createHash } from "node:crypto";

export type BulkBatchState = "draft" | "running" | "paused" | "cancelled" | "completed";
export type BulkItemState = "pending" | "attempting" | "accepted" | "sent" | "failed" | "unknown" | "cancelled";
export type BulkControl = "confirm" | "pause" | "resume" | "cancel";
export type SendEffect = "not_sent" | "sent" | "unknown";
export interface BulkRecipient { email: string; source: string }
export interface PrepareBulkInput { mailbox_ref_id: string; subject: string; body: string; recipients: readonly BulkRecipient[] }
export interface BulkMailbox { id: string; provider: string; email_address: string; available: boolean; capabilities: readonly string[] }
export interface BulkItem {
  id: string;
  recipient: string;
  source: string;
  fingerprint: string;
  state: BulkItemState;
  provider_message_id: string | null;
  detail: string | null;
}
export interface BulkBatch {
  id: string;
  mailbox_ref_id: string;
  subject: string;
  body: string;
  state: BulkBatchState;
  created_at: number;
  items: readonly BulkItem[];
}
export type PreparedBulk = Pick<BulkBatch, "mailbox_ref_id" | "subject" | "body"> & {
  items: readonly Pick<BulkItem, "recipient" | "source" | "fingerprint">[];
};

// Rust str::trim 使用 Unicode White_Space，不包含 JavaScript trim 接受的 BOM。
function rustTrim(value: string): string {
  if (!value.isWellFormed()) throw new Error("invalid Unicode string");
  return value.replace(/^\p{White_Space}+|\p{White_Space}+$/gu, "");
}

export function normalizeEmailAddress(value: string): string {
  const trimmed = rustTrim(value);
  const start = trimmed.lastIndexOf("<");
  const end = trimmed.lastIndexOf(">");
  const address = rustTrim(start >= 0 && end >= 0 && start < end ? trimmed.slice(start + 1, end) : trimmed)
    .replace(/[A-Z]/g, letter => letter.toLowerCase());
  const parts = address.split("@");
  if (parts.length !== 2 || parts[0] === "" || parts[1] === "") throw new Error("email address is invalid");
  return address;
}

export function mailFingerprint(sender: string, recipient: string, subject: string, body: string): string {
  for (const value of [sender, recipient, subject, body]) {
    if (!value.isWellFormed()) throw new Error("invalid Unicode string");
  }
  return createHash("sha256").update(JSON.stringify([sender.toLowerCase(), recipient.toLowerCase(), subject, body])).digest("hex");
}

export function prepareBulk(input: PrepareBulkInput, mailbox: BulkMailbox): PreparedBulk {
  const subject = rustTrim(input.subject);
  const body = rustTrim(input.body);
  if (input.recipients.length === 0 || input.recipients.length > 100) throw new Error("Select 1–100 email recipients");
  if (subject === "" || Buffer.byteLength(subject) > 998 || /[\r\n]/u.test(subject) || body === "" || Buffer.byteLength(body) > 100_000) {
    throw new Error("Invalid email subject or body");
  }
  if (mailbox.id !== input.mailbox_ref_id) throw new Error("mailbox reference mismatch");
  if (mailbox.provider !== "gmail") throw new Error("Bulk mail requires a Gmail mailbox");
  if (!mailbox.available) throw new Error("mailbox is unavailable");
  for (const capability of ["send", "sent_readback"]) {
    if (!mailbox.capabilities.includes(capability)) throw new Error(`mailbox does not support ${capability}`);
  }
  const seen = new Set<string>();
  const items: PreparedBulk["items"][number][] = [];
  for (const recipient of input.recipients) {
    const email = normalizeEmailAddress(recipient.email);
    const key = email.toLowerCase();
    if (seen.has(key)) continue;
    seen.add(key);
    if (!recipient.source.isWellFormed()) throw new Error("invalid Unicode string");
    if (Buffer.byteLength(recipient.source) > 2048) throw new Error("Recipient source is too long");
    items.push({ recipient: email, source: recipient.source, fingerprint: mailFingerprint(mailbox.email_address, email, subject, body) });
  }
  return { mailbox_ref_id: mailbox.id, subject, body, items };
}

export function completeIfSettled(batch: BulkBatch): BulkBatch {
  return batch.state === "running" && !batch.items.some(item => item.state === "pending" || item.state === "attempting")
    ? { ...batch, state: "completed" } : batch;
}

export function controlBulk(batch: BulkBatch, action: BulkControl): BulkBatch {
  const state = batch.state;
  if ((state === "running" && (action === "confirm" || action === "resume")) || (state === "paused" && action === "pause") || (state === "cancelled" && action === "cancel")) return batch;
  let next: BulkBatchState;
  if ((state === "draft" && action === "confirm") || (state === "paused" && action === "resume")) next = "running";
  else if (state === "running" && action === "pause") next = "paused";
  else if ((state === "draft" || state === "running" || state === "paused") && action === "cancel") next = "cancelled";
  else throw new Error("invalid bulk mail state transition");
  return completeIfSettled({ ...batch, state: next, items: next === "cancelled" ? batch.items.map(item => item.state === "pending" ? { ...item, state: "cancelled" } : item) : batch.items });
}

export function interruptBulk(batch: BulkBatch): BulkBatch {
  return {
    ...batch,
    state: batch.state === "running" ? "paused" : batch.state,
    items: batch.items.map(item => item.state === "attempting" ? {
      ...item, state: "unknown", detail: "Service interrupted after reservation; automatic resend disabled",
    } : item),
  };
}

export function duplicateAttempt(item: Pick<BulkItem, "id" | "fingerprint">, history: readonly BulkItem[]): boolean {
  return history.some(other => other.id !== item.id && other.fingerprint === item.fingerprint &&
    (other.state === "attempting" || other.state === "accepted" || other.state === "sent" || other.state === "unknown"));
}

// 仓储必须在同一事务内选择 pending、查重并保存结果，纯函数不提供并发保证。
export function claimBulkItem(batch: BulkBatch, itemId: string, history: readonly BulkItem[]): BulkItem {
  const item = batch.items.find(candidate => candidate.id === itemId);
  if (batch.state !== "running" || item?.state !== "pending") throw new Error("bulk item is not claimable");
  const duplicate = duplicateAttempt(item, history);
  return { ...item, state: duplicate ? "failed" : "attempting", detail: duplicate ? "Identical mail was already attempted; duplicate blocked" : null };
}

export type BulkSendResult = { kind: "success"; provider_message_id: string } | { kind: "failure"; effect: SendEffect; message: string };
export function recordBulkResult(item: BulkItem, result: BulkSendResult): BulkItem {
  if (item.state !== "attempting") throw new Error("bulk item has no active send attempt");
  if (result.kind === "success") return { ...item, state: "sent", provider_message_id: result.provider_message_id };
  return { ...item, state: result.effect === "not_sent" ? "failed" : result.effect === "sent" ? "accepted" : "unknown", detail: result.message };
}

export function bulkIdempotencyKey(item: Pick<BulkItem, "id">): string { return `gmail-bulk-${item.id}`; }

export function requireFreshGmailAttempt(key: string, attempts: ReadonlySet<string>): void {
  if (attempts.has(key)) throw new Error("A Gmail send was already attempted. Check sent mail; automatic resend is disabled.");
}
