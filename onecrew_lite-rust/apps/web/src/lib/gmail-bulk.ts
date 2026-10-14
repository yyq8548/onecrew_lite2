import { creatorInvoke } from "./creator";

export type BulkRecipient = { email: string; source: string };
export type BulkItem = { id: string; recipient: string; source: string; state: "pending" | "attempting" | "accepted" | "sent" | "failed" | "unknown" | "cancelled"; provider_message_id: string | null; detail: string | null };
export type BulkBatch = { id: string; mailbox_ref_id: string; subject: string; body: string; state: "draft" | "running" | "paused" | "cancelled" | "completed"; created_at: number; items: BulkItem[] };
export const BULK_KEY = ["gmail-bulk"] as const;
export const listBulk = () => creatorInvoke<BulkBatch[]>("onecrew_gmail_bulk_list");
export const listBulkRecipients = () => creatorInvoke<BulkRecipient[]>("onecrew_gmail_bulk_recipients");
export const prepareBulk = (input: { mailbox_ref_id: string; subject: string; body: string; recipients: BulkRecipient[] }) => creatorInvoke<BulkBatch>("onecrew_gmail_bulk_prepare", { input });
export const controlBulk = (id: string, action: "confirm" | "pause" | "resume" | "cancel") => creatorInvoke<void>("onecrew_gmail_bulk_control", { input: { id, action } });

export function uniqueRecipients(recipients: BulkRecipient[]): BulkRecipient[] {
  const result = new Map<string, BulkRecipient>();
  for (const recipient of recipients) {
    const email = recipient.email.trim();
    const key = email.toLowerCase();
    const existing = result.get(key);
    result.set(key, { email, source: existing ? `${existing.source}\n${recipient.source}` : recipient.source });
  }
  return [...result.values()];
}
