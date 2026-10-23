import { createHash, randomBytes, randomUUID } from "node:crypto";
import { AccountContextService, type AccountContext } from "../identity/account-context.ts";
import { OwnerReviewContext, PeerResumePayload } from "../results/owner-context.ts";
import { stringifyApplicationJson } from "../contracts/application-json.ts";
import { z } from "zod";

export const mcpSurface = z.enum(["full", "compact", "accounts", "creators", "outreach", "mail", "skills"]);
export type McpSurface = z.infer<typeof mcpSurface>;
interface Session { account: AccountContext; origin: string; surface: McpSurface; expires: number }
const digest = (token: string) => createHash("sha256").update(token).digest("hex");
export interface McpPeerCall { call_id: string; operation_id: string; schema_revision: string; connection: unknown; arguments: Record<string, unknown> }

// 每个适配器连接独立绑定账号与来源；请求不能指定结果所属的 origin。
export class McpSessions {
  readonly #accounts: AccountContextService;
  readonly #sessions = new Map<string, Session>();
  #closed = false;
  constructor(accounts: AccountContextService) { this.#accounts = accounts; }
  activeSessionCount(): number { this.#prune(); return this.#sessions.size; }
  #prune(): void { for (const [key, value] of this.#sessions) if (value.expires <= performance.now() || value.account.signal.aborted) this.#sessions.delete(key); }
  create(account: AccountContext, rawSurface: unknown) {
    if (this.#closed) throw new Error("mcp_session_closed");
    this.#accounts.assertCurrent(account); this.#prune();
    const surface = mcpSurface.parse(rawSurface);
    if (this.#sessions.size >= 128) throw new Error("mcp_session_capacity");
    const token = randomBytes(32).toString("base64url"), origin = `mcp:${surface}:${randomUUID()}`;
    this.#sessions.set(digest(token), { account, origin, surface, expires: performance.now() + 8 * 60 * 60_000 });
    return { session_id: token, surface, account_id: account.principal.user.id, auth_epoch: account.epoch };
  }
  authorize(token: string): Readonly<Session> {
    if (this.#closed || !/^[A-Za-z0-9_-]{43}$/u.test(token)) throw new Error("mcp_session_invalid");
    this.#prune();
    const session = this.#sessions.get(digest(token));
    if (!session) throw new Error("mcp_session_invalid");
    this.#accounts.assertCurrent(session.account);
    return Object.freeze({ ...session });
  }
  reviewContexts(token: string, calls: readonly McpPeerCall[]) {
    const session = this.authorize(token);
    if (calls.length < 1 || calls.length > 8 || new Set(calls.map(call => call.call_id)).size !== calls.length) throw new Error("mcp_execution_batch_invalid");
    const bytes = Buffer.from(stringifyApplicationJson(calls));
    if (bytes.length > 1024 * 1024) throw new Error("mcp_execution_batch_too_large");
    const peer = calls.length > 1 ? new PeerResumePayload(bytes) : undefined;
    const contexts = calls.map(call => new OwnerReviewContext(session.origin, call.call_id, call.operation_id, peer ? Buffer.from(stringifyApplicationJson(call)) : undefined));
    return { account: session.account, surface: session.surface, contexts, peer };
  }
  assertResultOrigin(token: string, origin: string): AccountContext {
    const session = this.authorize(token);
    if (origin !== session.origin) throw new Error("result_origin_mismatch");
    return session.account;
  }
  revoke(token: string): void { this.authorize(token); this.#sessions.delete(digest(token)); }
  close(): void { this.#closed = true; this.#sessions.clear(); }
}
