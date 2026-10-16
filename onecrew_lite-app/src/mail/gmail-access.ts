import { inspect } from "node:util";
import { z } from "zod";
import { AccessMailClient, MailAccessToken, gmailConnectionIdSchema } from "./access-mail-client.ts";
import type { MailConnection } from "./access-mail-client.ts";
import { asciiLower, GMAIL_READ_SCOPE, GMAIL_SEND_SCOPE, gmailMailboxId } from "./gmail-mailbox-repository.ts";
import type { GmailMailboxRepository } from "./gmail-mailbox-repository.ts";
import type { GmailBulkRepositoryOwner } from "./gmail-bulk-account-binding.ts";
import type { GmailAccessProvider, VerifiedGmailAuthorization } from "./gmail-transport.ts";
import { readGmailProfile } from "./gmail-transport.ts";

export interface VerifiedMailIdentity {
  readonly owner: GmailBulkRepositoryOwner;
  readonly accessUserId: string;
  exposeBearer(): string;
}
// 必须由唯一的真实 Access 身份验证方实现。assertCurrent 还需比较安装凭据及 bearer。
// 请求中的 accountId/tenantId/baseId 只用于比对，不能据此创建已验证身份。
export interface VerifiedMailIdentityProvider {
  snapshot(owner: GmailBulkRepositoryOwner): Promise<VerifiedMailIdentity>;
  assertCurrent(snapshot: VerifiedMailIdentity): Promise<void>;
}
export interface GmailMailboxRepositoryResolver {
  resolve(identity: VerifiedMailIdentity): Promise<Readonly<{ owner: GmailBulkRepositoryOwner; repository: GmailMailboxRepository }>>;
}

function sameOwner(a: GmailBulkRepositoryOwner, b: GmailBulkRepositoryOwner): boolean {
  return a.accountId === b.accountId && a.tenantId === b.tenantId && a.baseId === b.baseId;
}
export function validateGmailAccessToken(token: MailAccessToken, email: string, nowSeconds: number): number {
  const parsedDate = z.iso.datetime({ offset: true }).safeParse(token.expires_at);
  if (!parsedDate.success) throw new Error("gmail_token_invalid");
  const expires = Math.floor(Date.parse(parsedDate.data) / 1000);
  if (asciiLower(token.email) !== asciiLower(email) || !token.exposeSecret() || !token.scopes.includes(GMAIL_SEND_SCOPE) || !token.scopes.includes(GMAIL_READ_SCOPE) || !Number.isFinite(expires) || expires <= Math.floor(nowSeconds) + 30) throw new Error("gmail_authorization_required");
  return expires;
}

class AuthorizedGmailSession implements VerifiedGmailAuthorization {
  readonly owner: GmailBulkRepositoryOwner;
  readonly mailboxRefId: string;
  readonly emailAddress: string;
  readonly expiresAt: number;
  readonly #token: MailAccessToken;
  constructor(owner: GmailBulkRepositoryOwner, mailboxRefId: string, email: string, expires: number, token: MailAccessToken) {
    this.owner = Object.freeze({ ...owner }); this.mailboxRefId = mailboxRefId; this.emailAddress = email; this.expiresAt = expires; this.#token = token;
    Object.freeze(this);
  }
  get accessToken(): string { return this.#token.exposeSecret(); }
  toJSON(): never { throw new Error("Gmail authorization must not be serialized"); }
  [inspect.custom](): string { return "AuthorizedGmailSession [redacted]"; }
}

// 生产 client 固定，身份和账号数据库解析方必须由应用的真实验证链装配。
export class AccessGmailProvider implements GmailAccessProvider {
  readonly #identities: VerifiedMailIdentityProvider;
  readonly #repositories: GmailMailboxRepositoryResolver;
  readonly #client = new AccessMailClient();
  constructor(identities: VerifiedMailIdentityProvider, repositories: GmailMailboxRepositoryResolver) {
    this.#identities = identities; this.#repositories = repositories;
  }
  async #identity(owner: GmailBulkRepositoryOwner): Promise<VerifiedMailIdentity> {
    const identity = await this.#identities.snapshot(owner);
    if (!sameOwner(owner, identity.owner) || !identity.accessUserId || identity.accessUserId !== owner.accountId) throw new Error("gmail_account_mismatch");
    await this.#identities.assertCurrent(identity);
    return identity;
  }
  async #repository(identity: VerifiedMailIdentity): Promise<GmailMailboxRepository> {
    const resolved = await this.#repositories.resolve(identity);
    await this.#identities.assertCurrent(identity);
    if (!sameOwner(identity.owner, resolved.owner)) throw new Error("gmail_account_mismatch");
    return resolved.repository;
  }
  async connectGoogle(owner: GmailBulkRepositoryOwner): Promise<string> {
    const identity = await this.#identity(owner);
    const url = await this.#client.connectGoogle(identity.exposeBearer());
    await this.#identities.assertCurrent(identity);
    return url;
  }
  async profile(owner: GmailBulkRepositoryOwner, mailboxRefId: string, signal: AbortSignal): Promise<string> {
    const identity = await this.#identity(owner), authorization = await this.authorize(owner, mailboxRefId);
    return readGmailProfile(authorization, () => this.#identities.assertCurrent(identity), signal);
  }
  async connections(owner: GmailBulkRepositoryOwner): Promise<MailConnection[]> {
    const identity = await this.#identity(owner);
    const connections = await this.#client.connections(identity.exposeBearer());
    await this.#identities.assertCurrent(identity);
    return connections;
  }
  async refreshMailboxes(owner: GmailBulkRepositoryOwner): Promise<MailConnection[]> {
    const identity = await this.#identity(owner);
    const repository = await this.#repository(identity);
    const connections = await this.#client.connections(identity.exposeBearer());
    await this.#identities.assertCurrent(identity);
    repository.reconcile(identity.accessUserId, connections);
    await this.#identities.assertCurrent(identity);
    return connections;
  }
  async authorize(owner: GmailBulkRepositoryOwner, mailboxRefId: string): Promise<VerifiedGmailAuthorization> {
    const identity = await this.#identity(owner);
    const repository = await this.#repository(identity);
    const binding = repository.binding(mailboxRefId);
    if (binding.provider !== "gmail" || binding.providerKind !== "remote" || !binding.available || !binding.authAvailable || !binding.capabilities.includes("send") || !binding.capabilities.includes("sent_readback")) throw new Error("gmail_mailbox_unavailable");
    const prefix = `gmail:${identity.accessUserId}:`;
    if (!binding.authConnectionId.startsWith(prefix)) throw new Error("gmail_account_mismatch");
    const connectionId = binding.authConnectionId.slice(prefix.length);
    if (!gmailConnectionIdSchema.safeParse(connectionId).success || gmailMailboxId(identity.accessUserId, connectionId) !== mailboxRefId || binding.secretRecordId !== `access:${identity.accessUserId}:${connectionId}`) throw new Error("gmail_account_mismatch");
    const connections = await this.#client.connections(identity.exposeBearer());
    await this.#identities.assertCurrent(identity);
    const connection = connections.find(connection => connection.id === connectionId);
    if (!connection) throw new Error("gmail_connection_missing");
    if (connection.provider !== "google" || connection.status !== "connected" || asciiLower(connection.email) !== asciiLower(binding.email) || !connection.capabilities.can_send || !connection.capabilities.can_read_thread) throw new Error("gmail_authorization_required");
    const token = await this.#client.accessToken(identity.exposeBearer(), connectionId);
    const expires = validateGmailAccessToken(token, binding.email, Date.now() / 1000);
    await this.#identities.assertCurrent(identity);
    return new AuthorizedGmailSession(owner, mailboxRefId, binding.email, expires, token);
  }
}
