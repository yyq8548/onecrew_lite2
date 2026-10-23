import { simpleParser } from "mailparser";
import type { AddressObject } from "mailparser";
import { z } from "zod";
import { gmailReceivedMessageSchema } from "./gmail-reply-repository.ts";
import type { GmailReceivedMessage } from "./gmail-reply-repository.ts";

const header = z.object({ name: z.string().regex(/^[!#$%&'*+.^_`|~\da-z-]+$/iu), value: z.string().refine(value => value.isWellFormed() && !/[\r\n\0]/u.test(value)) });
interface Part { mimeType: string; filename: string; headers: z.infer<typeof header>[]; body: { data?: string | undefined; attachmentId?: string | undefined }; parts: Part[] }
const part: z.ZodType<Part> = z.lazy(() => z.object({ mimeType: z.string(), filename: z.string().default(""), headers: z.array(header).default([]), body: z.object({ data: z.string().optional(), attachmentId: z.string().optional() }), parts: z.array(part).default([]) }));
const message = z.object({ id: z.string().regex(/^[a-f\d]{1,128}$/iu), labelIds: z.array(z.string()).default([]), internalDate: z.string().regex(/^-?\d+$/u), payload: part });
const options = { skipHtmlToText: true, skipTextToHtml: true, skipImageLinks: true };
function addresses(value: AddressObject | AddressObject[] | undefined): string[] {
  return value === undefined ? [] : (Array.isArray(value) ? value : [value]).flatMap(group => group.value.map(address => {
    if (!address.address) throw new Error("gmail_address_invalid");
    return address.address;
  }));
}

// Gmail MessagePart 是结构化协议结果；RFC 头及选中的单个正文仍交由成熟 MIME 库解码。
export async function decodeGmailReplyMessage(value: unknown, expectedId: string, recipient: string): Promise<GmailReceivedMessage> {
  const input = message.parse(value);
  if (input.id !== expectedId || input.labelIds.includes("DRAFT")) throw new Error("gmail_received_message_mismatch");
  const timestamp = Number(input.internalDate);
  if (!Number.isSafeInteger(timestamp) || timestamp < 0 || !Number.isFinite(new Date(timestamp).getTime())) throw new Error("gmail_date_invalid");
  let plain: Part | undefined;
  let html: Part | undefined;
  async function visit(node: Part, depth: number): Promise<void> {
    if (depth > 16) throw new Error("gmail_mime_depth_exceeded");
    if (node.filename || node.mimeType === "message/rfc822") return;
    const disposition = node.headers.find(item => item.name.toLowerCase() === "content-disposition")?.value;
    if (disposition?.toLowerCase().startsWith("attachment")) return;
    const mimeHeaders = node.headers.filter(item => ["content-type", "content-disposition"].includes(item.name.toLowerCase()));
    if (mimeHeaders.length) {
      const metadata = await simpleParser(Buffer.from(`${mimeHeaders.map(item => `${item.name}: ${item.value}`).join("\r\n")}\r\n\r\n`), options);
      const structured = z.object({ params: z.record(z.string(), z.unknown()) });
      const contentType = structured.safeParse(metadata.headers.get("content-type"));
      const contentDisposition = structured.safeParse(metadata.headers.get("content-disposition"));
      if (contentType.success && Object.hasOwn(contentType.data.params, "name") || contentDisposition.success && Object.hasOwn(contentDisposition.data.params, "filename")) return;
    }
    if (node.mimeType === "text/plain" && plain === undefined) plain = node;
    if (node.mimeType === "text/html" && html === undefined) html = node;
    for (const child of node.parts) await visit(child, depth + 1);
  }
  await visit(input.payload, 0);
  async function body(node: Part | undefined): Promise<string | null> {
    if (!node) return null;
    if (node.body.attachmentId && node.body.data === undefined) throw new Error("gmail_reply_body_requires_attachment_fetch");
    const data = node.body.data ?? "";
    if (!/^[A-Za-z\d_-]*={0,2}$/u.test(data)) throw new Error("gmail_mime_invalid");
    const bytes = Buffer.from(data, "base64url");
    if (bytes.toString("base64url") !== data.replace(/=+$/u, "")) throw new Error("gmail_mime_invalid");
    const contentType = node.headers.find(item => item.name.toLowerCase() === "content-type")?.value ?? node.mimeType;
    const parsed = await simpleParser(Buffer.from(`MIME-Version: 1.0\r\nContent-Type: ${contentType}\r\nContent-Transfer-Encoding: base64\r\n\r\n${bytes.toString("base64")}\r\n`), options);
    return node.mimeType === "text/html" ? typeof parsed.html === "string" ? parsed.html : null : parsed.text ?? null;
  }
  const parsed = await simpleParser(Buffer.from(`${input.payload.headers.map(item => `${item.name}: ${item.value}`).join("\r\n")}\r\n\r\n`), options);
  const headers = Object.fromEntries(input.payload.headers.map(item => [item.name.toLowerCase(), item.value]));
  if (parsed.inReplyTo) headers["in-reply-to"] = parsed.inReplyTo;
  const refs = parsed.references === undefined ? [] : Array.isArray(parsed.references) ? parsed.references : [parsed.references];
  return gmailReceivedMessageSchema.parse({ providerMessageId: input.id, internetMessageId: parsed.messageId ?? null,
    from: parsed.from?.text ?? "", to: addresses(parsed.to), cc: addresses(parsed.cc), bcc: addresses(parsed.bcc), replyTo: addresses(parsed.replyTo),
    receivedFor: [recipient], subject: parsed.subject ?? "", bodyText: await body(plain), bodyHtml: await body(html),
    inReplyTo: parsed.inReplyTo ?? null, references: refs, headers, occurredAt: Math.floor(timestamp / 1000) });
}
