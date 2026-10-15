import { creatorInvoke } from "./creator";

export type GmailAuthorization = {
  status: string;
  scopes: string[];
  capabilities: { can_send: boolean; can_read_thread: boolean };
};

export type MailAccessConnection = GmailAuthorization & {
  id: string;
  provider: string;
  email: string;
};

export const MAIL_ACCESS_CONNECTIONS_KEY = ["mail-access-connections"] as const;
export const listMailAccessConnections = () => creatorInvoke<MailAccessConnection[]>("onecrew_mail_access_connections");
export const createGmailConnectUrl = () => creatorInvoke<string>("onecrew_gmail_connect_url");
export const refreshGmailMailboxes = () => creatorInvoke<MailAccessConnection[]>("onecrew_gmail_refresh");

export function gmailAuthorizationComplete(connection: GmailAuthorization): boolean {
  return connection.status === "connected"
    && connection.capabilities.can_send
    && connection.capabilities.can_read_thread
    && connection.scopes.includes("https://www.googleapis.com/auth/gmail.send")
    && connection.scopes.includes("https://www.googleapis.com/auth/gmail.readonly");
}
