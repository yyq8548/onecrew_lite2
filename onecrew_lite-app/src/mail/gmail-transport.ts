import { request } from "node:https";
import { setTimeout as delay } from "node:timers/promises";
import { z } from "zod";
import { compileGmailBulkMessage, GmailReadbackValidationError, verifyGmailBulkReadback } from "./gmail-mime.ts";
import type { GmailBulkMessage, VerifiedGmailSendReceipt } from "./gmail-mime.ts";
import type { GmailBulkRepositoryOwner } from "./gmail-bulk-account-binding.ts";

// 由 Access 验证接口提供；本模块不读取当前账号或持久保存 token。
export interface VerifiedGmailAuthorization {
  readonly owner: GmailBulkRepositoryOwner;
  readonly mailboxRefId: string;
  readonly emailAddress: string;
  readonly accessToken: string;
  readonly expiresAt: number;
}
export interface GmailAccessProvider {
  authorize(owner: GmailBulkRepositoryOwner, mailboxRefId: string): Promise<VerifiedGmailAuthorization>;
}
export type GmailBulkTransportResult =
  | { state: "not_sent"; code: string }
  | { state: "unknown"; code: string }
  | { state: "accepted_readback_pending"; code: string; providerMessageId: string }
  | { state: "sent"; providerMessageId: string; receipt: VerifiedGmailSendReceipt };
type Failure = { ok: false; status: number | null; reason: string | null; code: string };
type ResponseResult = Failure | { ok: true; value: unknown };
export class GmailAcceptedProcessingError extends Error {
  readonly providerMessageId: string;
  constructor(providerMessageId: string, cause: unknown) {
    super("Gmail accepted the message, but readback processing failed", { cause });
    this.providerMessageId = providerMessageId;
  }
}
const allowedReasons = new Set(["rateLimitExceeded", "userRateLimitExceeded", "dailyLimitExceeded", "domainPolicy", "insufficientPermissions", "forbidden"]);
let nextReadAt = 0;
let intervalMs = 250;

export function gmailReadRetryPolicy(status: number | null, reason: string | null, attempt: number): { retry: boolean; throttled: boolean } {
  const throttled = status === 429 && reason !== "retry_after" || status === 403 && (reason === "rateLimitExceeded" || reason === "userRateLimitExceeded");
  const transient = reason !== "retry_after" && (throttled || status !== null && [500, 502, 503, 504].includes(status));
  return { retry: transient && attempt < (throttled ? 3 : 2), throttled };
}

async function paceRead(signal?: AbortSignal): Promise<void> {
  while (performance.now() < nextReadAt) await delay(nextReadAt - performance.now(), undefined, signal ? { signal } : {});
  nextReadAt = performance.now() + intervalMs;
}

export async function decodeGmailResponse(bytes: Buffer, status: number, retryAfter: boolean): Promise<ResponseResult> {
  const decoded = await Promise.resolve().then(() => new TextDecoder("utf-8", { fatal: true, ignoreBOM: true }).decode(bytes)).then(
    value => ({ ok: true as const, value }), error => {
      if (error instanceof TypeError) return { ok: false as const };
      throw error;
    },
  );
  if (!decoded.ok) return { ok: false, status: status === 403 ? 403 : null, reason: null, code: "gmail_response_invalid_utf8" };
  if (status < 200 || status >= 300) {
    let reason: string | null = retryAfter ? "retry_after" : null;
    if (status === 403 && !retryAfter) {
      const parsed = await Promise.resolve().then(() => JSON.parse(decoded.value) as unknown).then(value => ({ value }), error => {
        if (error instanceof SyntaxError) return { value: null };
        throw error;
      });
      const reasons = z.object({ error: z.object({ errors: z.array(z.object({ reason: z.string() })).min(1) }) }).safeParse(parsed.value);
      if (reasons.success) {
        const first = reasons.data.error.errors[0]!.reason;
        if (allowedReasons.has(first) && reasons.data.error.errors.every(item => item.reason === first)) reason = first;
      }
    }
    return { ok: false, status, reason, code: `gmail_read_http_${status}${reason ? `_${reason}` : ""}` };
  }
  return Promise.resolve().then(() => JSON.parse(decoded.value) as unknown).then(
    value => ({ ok: true as const, value }),
    error => {
      if (error instanceof SyntaxError) return { ok: false as const, status: null, reason: null, code: "gmail_response_invalid_json" };
      throw error;
    },
  );
}

// 固定 Gmail HTTPS 地址；不接受 URL、重定向、代理或网络函数注入，POST 没有重试路径。
function requestOnce(token: string, method: "GET" | "POST", path: string, raw?: string, resource: "messages" | "threads" | "profile" = "messages", signal?: AbortSignal): Promise<ResponseResult> {
  return new Promise((resolve, reject) => {
    if (signal?.aborted) { resolve({ ok: false, status: null, reason: null, code: "gmail_read_cancelled" }); return; }
    let settled = false;
    let connectionTimer: ReturnType<typeof setTimeout>;
    let deadlineTimer: ReturnType<typeof setTimeout>;
    const finish = (result: ResponseResult) => {
      if (settled) return;
      settled = true;
      clearTimeout(connectionTimer);
      clearTimeout(deadlineTimer);
      signal?.removeEventListener("abort", abort);
      resolve(result);
    };
    const req = request({ hostname: "gmail.googleapis.com", port: 443, path: `/gmail/v1/users/me/${resource}${path}`, method, agent: false,
      headers: { authorization: `Bearer ${token}`, ...(method === "POST" ? { "content-type": "application/json" } : {}) } }, res => {
      const chunks: Buffer[] = [];
      let size = 0;
      const status = res.statusCode ?? 0;
      if ((status < 200 || status >= 300) && status !== 403) {
        finish({ ok: false, status, reason: res.headers["retry-after"] !== undefined ? "retry_after" : null, code: `gmail_read_http_${status}` });
        res.destroy();
        return;
      }
      const limit = status >= 200 && status < 300 ? 8 * 1024 * 1024 : 16 * 1024;
      const fail = (code: string) => finish({ ok: false, status: status === 403 ? status : null, reason: null, code: status === 403 ? "gmail_read_http_403" : code });
      res.on("data", (chunk: Buffer) => {
        size += chunk.length;
        if (size > limit) {
          if (status >= 200 && status < 300) fail("gmail_read_response_too_large");
          else finish({ ok: false, status, reason: res.headers["retry-after"] !== undefined ? "retry_after" : null, code: `gmail_read_http_${status}` });
          res.destroy();
        } else chunks.push(chunk);
      });
      res.once("error", () => fail("gmail_read_transport_failed"));
      res.once("aborted", () => fail("gmail_read_transport_failed"));
      res.once("end", () => {
        void decodeGmailResponse(Buffer.concat(chunks), status, res.headers["retry-after"] !== undefined).then(finish, error => {
          if (settled) return;
          settled = true;
          clearTimeout(connectionTimer);
          clearTimeout(deadlineTimer);
          signal?.removeEventListener("abort", abort);
          reject(error);
        });
      });
    });
    const abort = () => { finish({ ok: false, status: null, reason: null, code: "gmail_read_cancelled" }); req.destroy(); };
    signal?.addEventListener("abort", abort, { once: true });
    req.once("socket", socket => socket.once("secureConnect", () => clearTimeout(connectionTimer)));
    req.once("error", () => finish({ ok: false, status: null, reason: null, code: "gmail_read_transport_failed" }));
    const timeout = () => { finish({ ok: false, status: null, reason: null, code: "gmail_read_timeout" }); req.destroy(); };
    connectionTimer = setTimeout(timeout, 10_000);
    deadlineTimer = setTimeout(timeout, 30_000);
    req.end(raw === undefined ? undefined : JSON.stringify({ raw }));
  });
}

async function readGmail(auth: VerifiedGmailAuthorization, path: string, resource: "messages" | "threads" | "profile" = "messages", assertCurrent?: () => Promise<void>, signal?: AbortSignal): Promise<ResponseResult> {
  for (let attempt = 0; ; attempt++) {
    signal?.throwIfAborted();
    await paceRead(signal);
    if (assertCurrent) await assertCurrent();
    if (assertCurrent && (!auth.accessToken || auth.expiresAt <= Math.floor(Date.now() / 1000) + 30)) throw new Error("gmail_authorization_required");
    const result = await requestOnce(auth.accessToken, "GET", path, undefined, resource, signal);
    if (assertCurrent) await assertCurrent();
    if (result.ok) return result;
    const policy = gmailReadRetryPolicy(result.status, result.reason, attempt);
    if (policy.throttled) {
      intervalMs = Math.min(intervalMs * 2, 4000);
      nextReadAt = Math.max(nextReadAt, performance.now() + 5000 * 2 ** Math.min(attempt, 3));
    }
    if (!policy.retry) return result;
    if (!policy.throttled) await delay(1000 * 2 ** attempt, undefined, signal ? { signal } : {});
  }
}
export async function readGmailProfile(auth: VerifiedGmailAuthorization, assertCurrent: () => Promise<void>, signal?: AbortSignal): Promise<string> {
  await assertCurrent(); signal?.throwIfAborted();
  const response = await readGmail(auth, "?fields=emailAddress", "profile", assertCurrent, signal);
  await assertCurrent(); signal?.throwIfAborted();
  if (!response.ok) throw new Error(response.code.startsWith("gmail_response_") ? response.code.replace("gmail_response_", "gmail_read_response_") : response.code);
  const parsed = z.object({ emailAddress: z.email() }).safeParse(response.value);
  if (!parsed.success) throw new Error("gmail_read_profile_invalid");
  const email = parsed.data.emailAddress;
  const lower = (value: string) => value.replace(/[A-Z]/gu, character => character.toLowerCase());
  if (lower(email) !== lower(auth.emailAddress)) throw new Error("gmail_profile_identity_mismatch");
  return email;
}

async function readSent(auth: VerifiedGmailAuthorization, id: string): Promise<ResponseResult> {
  return readGmail(auth, `/${id}?format=raw`);
}

const gmailId = z.string().regex(/^[a-f\d]{1,128}$/iu);
export function gmailReplyThreadId(value: unknown, sentId: string): string {
  gmailId.parse(sentId);
  const seed = z.object({ id: gmailId, threadId: gmailId, labelIds: z.array(z.string()) }).parse(value);
  if (seed.id !== sentId || !seed.labelIds.includes("SENT")) throw new Error("gmail_sent_message_mismatch");
  return seed.threadId;
}
export function gmailReplyMessageIds(value: unknown, threadId: string, sentId: string): string[] {
  gmailId.parse(threadId); gmailId.parse(sentId);
  const thread = z.object({ id: gmailId, messages: z.array(z.object({ id: gmailId, labelIds: z.array(z.string()).default([]) })) }).parse(value);
  if (thread.id !== threadId) throw new Error("gmail_thread_id_mismatch");
  if (!thread.messages.some(message => message.id === sentId)) throw new Error("gmail_thread_seed_missing");
  return [...new Set(thread.messages.filter(message => !message.labelIds.some(label => ["SENT", "DRAFT", "TRASH", "SPAM"].includes(label))).map(message => message.id))];
}

// 只读取本地已确认发送所属会话；不暴露任意 URL 或整个收件箱扫描。
export async function readGmailOutreachReplyIds(auth: VerifiedGmailAuthorization, sentId: string, assertCurrent: () => Promise<void>, signal?: AbortSignal): Promise<string[]> {
  gmailId.parse(sentId);
  const read = async (path: string, resource: "messages" | "threads") => {
    await assertCurrent();
    if (!auth.accessToken || auth.expiresAt <= Math.floor(Date.now() / 1000) + 30) throw new Error("gmail_authorization_required");
    const response = await readGmail(auth, path, resource, assertCurrent, signal);
    await assertCurrent();
    if (!response.ok) throw new Error(response.code);
    return response.value;
  };
  const seed = await read(`/${sentId}?format=minimal&fields=id,threadId,labelIds`, "messages");
  const threadId = gmailReplyThreadId(seed, sentId);
  const thread = await read(`/${threadId}?format=minimal&fields=id,messages(id,labelIds)`, "threads");
  return gmailReplyMessageIds(thread, threadId, sentId);
}

export async function readGmailReplyMessage(auth: VerifiedGmailAuthorization, id: string, assertCurrent: () => Promise<void>, signal?: AbortSignal): Promise<unknown> {
  gmailId.parse(id);
  const response = await readGmail(auth, `/${id}?format=full`, "messages", assertCurrent, signal);
  if (!response.ok) throw new Error(response.code);
  return response.value;
}

function authorizationValid(auth: VerifiedGmailAuthorization, message: GmailBulkMessage): boolean {
  return auth.emailAddress === message.from && auth.mailboxRefId.length > 0 &&
    auth.owner.accountId.length > 0 && auth.owner.tenantId.length > 0 && auth.owner.baseId.length > 0 &&
    /^[A-Za-z0-9\-._~+/]+=*$/u.test(auth.accessToken) && Buffer.byteLength(auth.accessToken) <= 16384 && Number.isFinite(auth.expiresAt) && auth.expiresAt > Date.now() / 1000 + 30;
}

// 仅供 GmailBulk：调用者先冻结原账号仓储并独立提交永久尝试标记。
// signal 只在 POST 开始前检查；开始后的结果必须提交回原账号仓储。
export async function sendGmailBulkMessage(auth: VerifiedGmailAuthorization, message: GmailBulkMessage, signal: AbortSignal, beforePost?: () => void, onAccepted?: (providerMessageId: string) => void): Promise<GmailBulkTransportResult> {
  if (signal.aborted) return { state: "not_sent", code: "gmail_send_cancelled_before_attempt" };
  const authorization = { owner: { ...auth.owner }, mailboxRefId: auth.mailboxRefId, emailAddress: auth.emailAddress, accessToken: auth.accessToken, expiresAt: auth.expiresAt };
  const frozenMessage = { ...message };
  if (!authorizationValid(authorization, frozenMessage)) return { state: "not_sent", code: "gmail_authorization_required" };
  const compiled = await compileGmailBulkMessage(frozenMessage).then(value => ({ ok: true as const, value }), error => {
    if (error instanceof z.ZodError) return { ok: false as const, code: `gmail_input_invalid:${error.issues.map(issue => `${issue.path.join(".")}:${issue.code}`).join(",")}` };
    throw new Error("Gmail MIME compilation failed before provider send", { cause: error });
  });
  if (!compiled.ok) return { state: "not_sent", code: compiled.code };
  if (signal.aborted) return { state: "not_sent", code: "gmail_send_cancelled_before_attempt" };
  if (!authorizationValid(authorization, frozenMessage)) return { state: "not_sent", code: "gmail_authorization_required" };
  beforePost?.();
  if (!authorizationValid(authorization, frozenMessage)) return { state: "not_sent", code: "gmail_authorization_required" };
  const result = await requestOnce(authorization.accessToken, "POST", "/send", compiled.value.raw);
  if (!result.ok) return result.status !== null && result.status >= 400 && result.status < 500
    ? { state: "not_sent", code: `gmail_send_rejected:${result.code}` } : { state: "unknown", code: result.code };
  const accepted = z.object({ id: z.string().regex(/^[a-f\d]{1,128}$/iu) }).safeParse(result.value);
  if (!accepted.success) return { state: "unknown", code: "gmail_send_response_invalid" };
  const id = accepted.data.id;
  await Promise.resolve().then(() => onAccepted?.(id)).then(undefined, error => { throw new GmailAcceptedProcessingError(id, error); });
  const readback = await readSent(authorization, id).then(value => value, error => { throw new GmailAcceptedProcessingError(id, error); });
  if (!readback.ok) return { state: "accepted_readback_pending", providerMessageId: id, code: readback.code };
  return verifyGmailBulkReadback(readback.value, id, frozenMessage).then(
    receipt => ({ state: "sent" as const, providerMessageId: id, receipt }),
    error => {
      if (error instanceof GmailReadbackValidationError) return { state: "accepted_readback_pending" as const, providerMessageId: id, code: error.code };
      if (error instanceof z.ZodError) return { state: "accepted_readback_pending" as const, providerMessageId: id, code: "gmail_readback_invalid_contract" };
      throw new GmailAcceptedProcessingError(id, error);
    },
  );
}
