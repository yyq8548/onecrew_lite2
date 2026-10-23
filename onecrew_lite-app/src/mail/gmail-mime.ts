import { createHash } from "node:crypto";
import MailComposer from "nodemailer/lib/mail-composer";
import { simpleParser } from "mailparser";
import { z } from "zod";
import { bulkIdempotencyKey, mailFingerprint } from "./gmail-bulk.ts";
import type { BulkBatch, BulkItem } from "./gmail-bulk.ts";

const text = z.string().refine(value => value.isWellFormed(), "invalid Unicode string");
const address = z.email().refine(value => !/[\r\n\0]/u.test(value));
const messageSchema = z.object({
  from: address, to: address,
  subject: text.min(1).refine(value => Buffer.byteLength(value) <= 998 && !/[\r\n\0]/u.test(value)),
  body: text.min(1).refine(value => Buffer.byteLength(value) <= 100_000),
  idempotencyKey: text.min(1),
}).strict();
export type GmailBulkMessage = z.infer<typeof messageSchema>;

export class GmailReadbackValidationError extends Error {
  readonly code: string;
  constructor(code: string) { super(code); this.code = code; }
}

export function messageForGmailBulkClaim(batch: BulkBatch, item: BulkItem, from: string): GmailBulkMessage {
  const stored = batch.items.find(candidate => candidate.id === item.id);
  if (batch.state !== "running" || item.state !== "attempting" || !stored ||
    stored.recipient !== item.recipient || stored.source !== item.source || stored.fingerprint !== item.fingerprint ||
    item.fingerprint !== mailFingerprint(from, item.recipient, batch.subject, batch.body)) throw new Error("gmail_bulk_frozen_claim_mismatch");
  return messageSchema.parse({ from, to: item.recipient, subject: batch.subject, body: batch.body, idempotencyKey: bulkIdempotencyKey(item) });
}

export function gmailMessageId(from: string, idempotencyKey: string): string {
  return `<${createHash("sha256").update(`${from}\0${idempotencyKey}`).digest("hex")}@onecrew.local>`;
}

export function gmailReceiptMessageIdMatches(actual: string | undefined, from: string, key: string): actual is string {
  // Gmail 可重写 MIME 编号；原发送回执的 provider ID 和冻结内容仍须分别核对。
  return actual === gmailMessageId(from, key) || actual !== undefined && actual.startsWith("<") && actual.endsWith("@mail.gmail.com>");
}

export function messageForGmailBulkReceipt(batch: BulkBatch, item: BulkItem, from: string): GmailBulkMessage {
  const stored = batch.items.find(candidate => candidate.id === item.id);
  if (!stored || !["accepted", "unknown"].includes(stored.state) || !stored.provider_message_id ||
    stored.provider_message_id !== item.provider_message_id || stored.recipient !== item.recipient || stored.fingerprint !== item.fingerprint ||
    stored.source !== item.source || item.fingerprint !== mailFingerprint(from, item.recipient, batch.subject, batch.body)) throw new Error("gmail_bulk_receipt_claim_mismatch");
  return messageSchema.parse({ from, to: item.recipient, subject: batch.subject, body: batch.body, idempotencyKey: bulkIdempotencyKey(item) });
}

export async function compileGmailBulkMessage(input: GmailBulkMessage): Promise<{ raw: string; bytes: Buffer; messageId: string }> {
  const message = messageSchema.parse(input);
  const messageId = gmailMessageId(message.from, message.idempotencyKey);
  const bytes = await new MailComposer({
    from: { address: message.from, name: "" }, to: [{ address: message.to, name: "" }],
    subject: message.subject, text: message.body, messageId,
    textEncoding: "base64", newline: "\r\n", disableFileAccess: true, disableUrlAccess: true,
  }).compile().build();
  return { raw: bytes.toString("base64url"), bytes, messageId };
}

const readbackSchema = z.object({ id: z.string().regex(/^[a-f\d]{1,128}$/iu), raw: z.string().regex(/^[A-Za-z0-9_-]+={0,2}$/u), labelIds: z.array(z.string()), internalDate: z.string().regex(/^\d+$/u) });
const newline = (value: string) => value.replace(/\r\n/gu, "\n").replace(/\n$/u, "");
export interface VerifiedGmailSendReceipt { providerMessageId: string; internetMessageId: string; from: string; to: string; subject: string; body: string; occurredAt: number; headers: Record<string, string> }

export async function verifyGmailBulkReadback(value: unknown, providerId: string, original: GmailBulkMessage): Promise<VerifiedGmailSendReceipt> {
  const expected = messageSchema.parse(original);
  const message = readbackSchema.parse(value);
  if (message.id !== providerId || !message.labelIds.includes("SENT") || message.labelIds.includes("DRAFT")) throw new GmailReadbackValidationError("gmail_sent_message_mismatch");
  const bytes = Buffer.from(message.raw, "base64url");
  if (bytes.toString("base64url") !== message.raw.replace(/=+$/u, "")) throw new GmailReadbackValidationError("gmail_mime_invalid");
  const parsed = await simpleParser(bytes, { skipHtmlToText: true, skipTextToHtml: true, skipImageLinks: true });
  const recipients = parsed.to === undefined ? [] : Array.isArray(parsed.to) ? parsed.to.flatMap(part => part.value) : parsed.to.value;
  if (!gmailReceiptMessageIdMatches(parsed.messageId, expected.from, expected.idempotencyKey) ||
    parsed.from?.value.length !== 1 || parsed.from.value[0]?.address?.toLowerCase() !== expected.from.toLowerCase() ||
    recipients.length !== 1 || recipients[0]?.address?.toLowerCase() !== expected.to.toLowerCase() ||
    parsed.cc !== undefined || parsed.bcc !== undefined || parsed.attachments.length !== 0 ||
    parsed.subject !== expected.subject || parsed.text === undefined || newline(parsed.text) !== newline(expected.body)) throw new GmailReadbackValidationError("gmail_sent_content_mismatch");
  const millis = Number(message.internalDate);
  if (!Number.isSafeInteger(millis) || !Number.isFinite(new Date(millis).getTime())) throw new GmailReadbackValidationError("gmail_date_invalid");
  const headers = Object.fromEntries(parsed.headerLines.map(item => [item.key.toLowerCase(), item.line.slice(item.line.indexOf(":") + 1).trim()]));
  headers.subject = parsed.subject!;
  headers.from = parsed.from!.text;
  headers.to = expected.to;
  return { providerMessageId: providerId, internetMessageId: parsed.messageId!, from: parsed.from!.text, to: expected.to, subject: parsed.subject!, body: parsed.text!, occurredAt: Math.floor(millis / 1000), headers };
}
