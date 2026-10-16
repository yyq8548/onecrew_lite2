import { createHash, randomBytes, timingSafeEqual } from "node:crypto";
import { BrowserSessionError } from "./browser-sessions.ts";

const digest = (value: string) => createHash("sha256").update(value).digest("hex");
const token = () => randomBytes(32).toString("base64url");
const valid = (value: string | undefined): value is string => typeof value === "string" && /^[A-Za-z0-9_-]{43}$/u.test(value);
interface BootstrapSession { csrf: string; expires: number; flowId: string | null }

// 未登录页面只拥有独立配对权限；不能借此取得任何账号、表格或平台数据。
export class BootstrapSessions {
  readonly #tickets = new Map<string, number>();
  readonly #sessions = new Map<string, BootstrapSession>();
  #prune(): void {
    const now = performance.now();
    for (const [key, expires] of this.#tickets) if (expires <= now) this.#tickets.delete(key);
    for (const [key, session] of this.#sessions) if (session.expires <= now) this.#sessions.delete(key);
  }
  issueTicket(): string {
    this.#prune();
    if (this.#tickets.size >= 32) throw new Error("Too many bootstrap tickets");
    const value = token(); this.#tickets.set(digest(value), performance.now() + 60_000); return value;
  }
  acceptsTicket(value: string): boolean { this.#prune(); return valid(value) && this.#tickets.has(digest(value)); }
  exchange(value: string) {
    this.#prune();
    const key = valid(value) ? digest(value) : "";
    const exists = this.#tickets.delete(key);
    if (!exists) throw new BrowserSessionError();
    if (this.#sessions.size >= 128) throw new Error("Too many bootstrap sessions");
    const cookie = token(), csrf = token();
    this.#sessions.set(digest(cookie), { csrf, expires: performance.now() + 20 * 60_000, flowId: null });
    return { cookie, csrf, scope: "bootstrap" as const };
  }
  has(cookie: string | undefined): boolean { this.#prune(); return valid(cookie) && this.#sessions.has(digest(cookie)); }
  authorize(cookie: string | undefined, csrf: string | undefined, write: boolean): Readonly<BootstrapSession> {
    this.#prune();
    const session = valid(cookie) ? this.#sessions.get(digest(cookie)) : undefined;
    if (!session || write && (!valid(csrf) || !timingSafeEqual(Buffer.from(csrf), Buffer.from(session.csrf)))) throw new BrowserSessionError();
    return Object.freeze({ ...session });
  }
  bindPairing(cookie: string | undefined, flowId: string): void {
    this.authorize(cookie, undefined, false);
    if (!flowId) throw new Error("Bootstrap pairing identity is missing");
    this.#sessions.get(digest(cookie!))!.flowId = flowId;
  }
  revoke(cookie: string | undefined): void { if (valid(cookie)) this.#sessions.delete(digest(cookie)); }
  clear(): void { this.#tickets.clear(); this.#sessions.clear(); }
}
