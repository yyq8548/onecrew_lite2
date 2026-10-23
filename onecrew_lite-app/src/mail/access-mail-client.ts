import { request } from "node:https";
import { inspect } from "node:util";
import { z } from "zod";

export const ACCESS_MAIL_ORIGIN = "https://access.onecrewai.io";
export const ACCESS_MAIL_PREFIX = "/api/desktop/integrations/mail/v1";
const wireString = z.string().refine(value => value.isWellFormed(), "Invalid Unicode string");
export const gmailConnectionIdSchema = z.string().regex(/^conn_[a-f\d]{8}-[a-f\d]{4}-[a-f\d]{4}-[a-f\d]{4}-[a-f\d]{12}$/iu);
export const mailConnectionSchema = z.object({
  id: gmailConnectionIdSchema, provider: wireString, email: wireString, status: wireString, scopes: z.array(wireString),
  capabilities: z.object({ can_send: z.boolean(), can_read_thread: z.boolean(), can_read_attachment: z.boolean() }),
  created_at: wireString, updated_at: wireString,
});
export type MailConnection = z.infer<typeof mailConnectionSchema>;
export type MailAccessErrorCode = "mail_access_configuration_invalid" | "mail_access_network_failed" | "mail_access_login_required" | "mail_access_denied" | "mail_access_request_rejected" | "mail_access_response_invalid";
export class MailAccessError extends Error {
  readonly code: MailAccessErrorCode;
  constructor(code: MailAccessErrorCode) { super(code); this.code = code; }
}
export class MailAccessToken {
  readonly #secret: string;
  readonly expires_at: string;
  readonly scopes: readonly string[];
  readonly email: string;
  constructor(secret: string, expiresAt: string, scopes: readonly string[], email: string) {
    this.#secret = secret; this.expires_at = expiresAt; this.scopes = Object.freeze([...scopes]); this.email = email;
    Object.freeze(this);
  }
  exposeSecret(): string { return this.#secret; }
  toJSON(): never { throw new Error("Mail access tokens must not be serialized"); }
  [inspect.custom](): string { return "MailAccessToken [redacted]"; }
}

export async function decodeAccessMailEnvelope<T>(bytes: Buffer, schema: z.ZodType<T>): Promise<T> {
  if (bytes.length > 256 * 1024) throw new MailAccessError("mail_access_response_invalid");
  const json = await Promise.resolve().then(() => JSON.parse(new TextDecoder("utf-8", { fatal: true, ignoreBOM: true }).decode(bytes)) as unknown).then(value => value, error => {
    if (error instanceof SyntaxError || error instanceof TypeError) throw new MailAccessError("mail_access_response_invalid");
    throw error;
  });
  const envelope = z.object({ protocol: z.literal("onecrew.mail"), version: z.literal(1), data: schema }).safeParse(json);
  if (!envelope.success) throw new MailAccessError("mail_access_response_invalid");
  return envelope.data.data;
}

export function validateGoogleConfirmationUrl(value: string): string {
  if (!value.isWellFormed() || !URL.canParse(value)) throw new MailAccessError("mail_access_response_invalid");
  const url = new URL(value);
  const pairs = [...url.searchParams];
  if (url.origin !== ACCESS_MAIL_ORIGIN || url.username || url.password || url.hash || url.pathname !== "/integrations/mail/connect" || pairs.length !== 2 ||
    pairs.filter(([key, value]) => key === "provider" && value === "google").length !== 1 ||
    pairs.filter(([key, value]) => key === "ticket" && value.length > 0 && Buffer.byteLength(value) <= 2048).length !== 1) throw new MailAccessError("mail_access_response_invalid");
  return value;
}

export function accessMailStatusError(status: number): MailAccessError | null {
  return status === 200 ? null : new MailAccessError(status === 401 ? "mail_access_login_required" : status === 403 ? "mail_access_denied" : "mail_access_request_rejected");
}

async function machineRequest<T>(bearer: string, method: "GET" | "POST", path: string, schema: z.ZodType<T>, body?: object): Promise<T> {
  if (!/^[A-Za-z0-9\-._~+/]+=*$/u.test(bearer)) throw new MailAccessError("mail_access_configuration_invalid");
  const bytes = await new Promise<Buffer>((resolve, reject) => {
    let settled = false;
    let connectionTimer: ReturnType<typeof setTimeout>;
    let deadline: ReturnType<typeof setTimeout>;
    const finish = (value: Buffer | MailAccessError) => {
      if (settled) return;
      settled = true;
      clearTimeout(connectionTimer); clearTimeout(deadline);
      if (value instanceof MailAccessError) reject(value); else resolve(value);
    };
    const req = request({ hostname: "access.onecrewai.io", port: 443, path: ACCESS_MAIL_PREFIX + path, method, agent: false,
      headers: { authorization: `Bearer ${bearer}`, accept: "application/json", "cache-control": "no-store", ...(body ? { "content-type": "application/json" } : {}) } }, response => {
      const statusError = accessMailStatusError(response.statusCode ?? 0);
      if (statusError) { finish(statusError); response.destroy(); return; }
      const chunks: Buffer[] = [];
      let size = 0;
      response.on("data", (chunk: Buffer) => {
        size += chunk.length;
        if (size > 256 * 1024) { finish(new MailAccessError("mail_access_response_invalid")); response.destroy(); }
        else chunks.push(chunk);
      });
      response.once("end", () => finish(Buffer.concat(chunks)));
      response.once("aborted", () => finish(new MailAccessError("mail_access_network_failed")));
      response.once("error", () => finish(new MailAccessError("mail_access_network_failed")));
    });
    req.once("socket", socket => socket.once("secureConnect", () => clearTimeout(connectionTimer)));
    req.once("error", () => finish(new MailAccessError("mail_access_network_failed")));
    const timeout = () => { finish(new MailAccessError("mail_access_network_failed")); req.destroy(); };
    connectionTimer = setTimeout(timeout, 10000); deadline = setTimeout(timeout, 30000);
    req.end(body ? JSON.stringify(body) : undefined);
  });
  return decodeAccessMailEnvelope(bytes, schema);
}

export class AccessMailClient {
  async connectGoogle(bearer: string): Promise<string> {
    const data = await machineRequest(bearer, "POST", "/connect-url", z.object({ url: wireString }), { provider: "google" });
    return validateGoogleConfirmationUrl(data.url);
  }
  async connections(bearer: string): Promise<MailConnection[]> {
    return (await machineRequest(bearer, "GET", "/connections", z.object({ connections: z.array(mailConnectionSchema).max(100) }))).connections;
  }
  async accessToken(bearer: string, id: string): Promise<MailAccessToken> {
    if (!gmailConnectionIdSchema.safeParse(id).success) throw new MailAccessError("mail_access_configuration_invalid");
    const value = await machineRequest(bearer, "POST", `/connections/${id}/access-token`, z.object({ access_token: wireString.refine(value => value.length > 0 && Buffer.byteLength(value) <= 16384), expires_at: wireString, scopes: z.array(wireString), email: wireString }));
    return new MailAccessToken(value.access_token, value.expires_at, value.scopes, value.email);
  }
}
