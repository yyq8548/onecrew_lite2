type Availability = "available" | "unavailable";
interface OperatorSummary { operator_account_id: string; username: string; label: string; availability: Availability }
export interface PlatformConnectionInventory {
  x: OperatorSummary[]; instagram: OperatorSummary[]; telegram: OperatorSummary[];
  youtube: { credential_id: string; label: string; availability: Availability }[];
  mailboxes: { mailbox_ref_id: string; email_address: string; display_label: string; provider: "gmail" | "resend"; availability: Availability }[];
}
export function projectMcpConnections(inventory: PlatformConnectionInventory, toolkits: readonly string[]) {
  const includes = (toolkit: string) => toolkits.length === 0 || toolkits.includes(toolkit);
  let truncated = false;
  function limit<T>(toolkit: string, values: T[]): T[] {
    if (!includes(toolkit)) return [];
    truncated ||= values.length > 20; return values.slice(0, 20);
  }
  const operators = (toolkit: "x" | "instagram" | "telegram") => limit(toolkit, inventory[toolkit]).map(({ operator_account_id, ...value }) => ({ connection: { kind: "operator_account", toolkit, operator_account_id }, ...value }));
  const x = operators("x"), instagram = operators("instagram"), telegram = operators("telegram");
  const youtube = limit("youtube", inventory.youtube).map(({ credential_id, ...value }) => ({ connection: { kind: "service_credential", toolkit: "youtube", credential_id }, ...value }));
  const email = limit("email", inventory.mailboxes).map(({ mailbox_ref_id, provider: _, ...value }) => ({ connection: { kind: "mailbox", toolkit: "email", mailbox_ref_id }, ...value }));
  return { x, instagram, telegram, youtube, email, truncated };
}
