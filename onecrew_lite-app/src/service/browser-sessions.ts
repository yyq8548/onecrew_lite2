import { createHash, randomBytes, timingSafeEqual } from "node:crypto";
import type { AccountContext, AccountContextService } from "../identity/account-context.ts";

export class BrowserSessionError extends Error {
  readonly code = "local.invalid_session";
  readonly status = 401;
  constructor() { super("Browser session is missing, expired or invalid"); }
}
const digest = (value: string) => createHash("sha256").update(value).digest("hex");
const token = () => randomBytes(32).toString("base64url");
const validToken = (value: string) => /^[A-Za-z0-9_-]{43}$/u.test(value);
export function browserCookieName(origin: string): string {
  const url = new URL(origin);
  if (url.protocol !== "http:" || url.hostname !== "127.0.0.1" || !url.port) throw new Error("Browser cookie requires the bound loopback origin");
  return `onecrew_session_${url.port}`;
}
interface BoundContext { context: AccountContext; expires: number }
interface Session extends BoundContext { csrf: string }

// 登录票据只使用一次；内存中的浏览器会话随服务退出失效，不把服务控制令牌交给网页。
export class BrowserSessions {
  readonly #accounts: AccountContextService;
  readonly #tickets = new Map<string, BoundContext>();
  readonly #sessions = new Map<string, Session>();
  constructor(accounts: AccountContextService) { this.#accounts = accounts; }
  #prune(): void {
    const now = performance.now();
    for (const map of [this.#tickets, this.#sessions]) {
      for (const [key, entry] of map) if (entry.context.signal.aborted || entry.expires <= now) map.delete(key);
    }
  }
  issueTicket(context: AccountContext): string {
    this.#accounts.assertCurrent(context);
    this.#prune();
    if (this.#tickets.size >= 32) throw new Error("Too many outstanding browser login tickets");
    const value = token();
    this.#tickets.set(digest(value), { context, expires: performance.now() + 60_000 });
    return value;
  }
  exchange(value: string, existingCookie?: string) {
    this.#prune();
    if (!validToken(value)) throw new BrowserSessionError();
    const key = digest(value);
    const ticket = this.#tickets.get(key);
    this.#tickets.delete(key);
    if (!ticket) throw new BrowserSessionError();
    this.#accounts.assertCurrent(ticket.context);
    const existing = existingCookie && validToken(existingCookie) ? this.#sessions.get(digest(existingCookie)) : undefined;
    if (existing && existing.context.principal.user.id === ticket.context.principal.user.id && existing.context.epoch === ticket.context.epoch) {
      this.#accounts.assertCurrent(existing.context);
      existing.expires = performance.now() + 8 * 60 * 60_000;
      return { cookie: existingCookie!, csrf: existing.csrf, accountId: existing.context.principal.user.id, epoch: existing.context.epoch };
    }
    if (this.#sessions.size >= 128) throw new Error("Too many active browser sessions");
    const cookie = token();
    const csrf = token();
    this.#sessions.set(digest(cookie), { context: ticket.context, csrf, expires: performance.now() + 8 * 60 * 60_000 });
    return { cookie, csrf, accountId: ticket.context.principal.user.id, epoch: ticket.context.epoch };
  }
  authorize(cookie: string | undefined, csrf: string | undefined, write: boolean): AccountContext {
    this.#prune();
    if (!cookie || !validToken(cookie)) throw new BrowserSessionError();
    const session = this.#sessions.get(digest(cookie));
    if (!session) throw new BrowserSessionError();
    this.#accounts.assertCurrent(session.context);
    if (write) {
      if (!csrf || !validToken(csrf) || !timingSafeEqual(Buffer.from(csrf), Buffer.from(session.csrf))) throw new BrowserSessionError();
    }
    return session.context;
  }
  readSession(cookie: string | undefined) {
    const context = this.authorize(cookie, undefined, false);
    const session = this.#sessions.get(digest(cookie!))!;
    session.expires = performance.now() + 8 * 60 * 60_000;
    return Object.freeze({ csrf: session.csrf, accountId: context.principal.user.id, epoch: context.epoch });
  }
  clear(): void { this.#tickets.clear(); this.#sessions.clear(); }
}
