import { request } from "node:https";
import { z } from "zod";
import type { VerifiedMailIdentityProvider } from "../mail/gmail-access.ts";
import type { CreatorRepositoryOwner } from "../creators/operations.ts";
import { parseApplicationJson, stringifyApplicationJson } from "../contracts/application-json.ts";

const jsonObject = z.record(z.string(), z.unknown());
export const tableSyncRequest = z.discriminatedUnion("operation", [
  z.object({ operation: z.literal("capabilities") }).strict(),
  z.object({ operation: z.literal("consent") }).strict(),
  z.object({ operation: z.literal("set_consent"), input: jsonObject, etag: z.string().nullable().optional().default(null) }).strict(),
  z.object({ operation: z.literal("begin"), input: jsonObject }).strict(),
  z.object({ operation: z.literal("status"), version_id: z.uuid() }).strict(),
  z.object({ operation: z.literal("part"), version_id: z.uuid(), attempt_id: z.uuid(), number: z.number().int().min(1).max(2048), md5_base64: z.string(), bytes: z.instanceof(Uint8Array).refine(value => value.byteLength > 0 && value.byteLength <= 8 * 1024 * 1024) }).strict(),
  z.object({ operation: z.literal("complete"), version_id: z.uuid(), input: jsonObject }).strict(),
  z.object({ operation: z.literal("abort"), version_id: z.uuid(), attempt_id: z.uuid() }).strict(),
]);
export type TableSyncRequest = z.infer<typeof tableSyncRequest>;
export interface TableSyncResponse { status: number; etag: string | null; retry_after: string | null; body: unknown }
export class TableSyncError extends Error {
  readonly code: string;
  readonly status = 503;
  constructor(code: string) { super(code); this.code = code; }
}

// 仅构造原固定端点。控制文本与原始分片分别限制，输入不能选择远端地址。
export function compileTableSyncRequest(raw: unknown) {
  const input = tableSyncRequest.parse(raw);
  let method = "GET", path: string, bytes: Buffer | undefined;
  const headers = new Headers({ "Cache-Control": "no-store" });
  switch (input.operation) {
    case "capabilities": path = "capabilities"; break;
    case "consent": path = "consent"; break;
    case "set_consent": method = "PUT"; path = "consent"; headers.set(input.etag === null ? "If-None-Match" : "If-Match", input.etag ?? "*"); break;
    case "begin": method = "POST"; path = "uploads"; break;
    case "status": path = `uploads/${input.version_id}`; break;
    case "part":
      method = "PUT"; path = `uploads/${input.version_id}/parts/${input.number}`; bytes = Buffer.from(input.bytes);
      headers.set("Content-Type", "application/octet-stream"); headers.set("Content-Length", String(bytes.length)); headers.set("Content-MD5", input.md5_base64); headers.set("X-OneCrew-Upload-Attempt", input.attempt_id); break;
    case "complete": method = "POST"; path = `uploads/${input.version_id}/complete`; break;
    case "abort": method = "POST"; path = `uploads/${input.version_id}/abort`; bytes = Buffer.from(stringifyApplicationJson({ attempt_id: input.attempt_id })); headers.set("Content-Type", "application/json"); break;
  }
  if ("input" in input) {
    bytes = Buffer.from(stringifyApplicationJson(input.input));
    if (bytes.length > (input.operation === "complete" ? 512 * 1024 : 16 * 1024)) throw new TableSyncError("table_sync_control_too_large");
    headers.set("Content-Type", "application/json");
  }
  return { method, url: new URL(`/dynamic-table-sync/${path}`, "https://api.onecrewai.io"), headers, body: bytes };
}

export class TableSyncClient {
  readonly #identities: VerifiedMailIdentityProvider;
  constructor(identities: VerifiedMailIdentityProvider) { this.#identities = identities; }
  async send(owner: CreatorRepositoryOwner, raw: unknown, signal: AbortSignal): Promise<TableSyncResponse> {
    signal.throwIfAborted();
    const compiled = compileTableSyncRequest(raw), identity = await this.#identities.snapshot(owner);
    if (identity.owner.accountId !== owner.accountId || identity.owner.tenantId !== owner.tenantId || identity.owner.baseId !== owner.baseId || identity.accessUserId !== owner.accountId) throw new TableSyncError("table_sync_account_changed");
    await this.#identities.assertCurrent(identity); signal.throwIfAborted();
    compiled.headers.set("Authorization", `Bearer ${identity.exposeBearer()}`);
    const combined = AbortSignal.any([signal, AbortSignal.timeout(45_000)]);
    const reply = await new Promise<TableSyncResponse>((resolve, reject) => {
      const outgoing = request(compiled.url, { method: compiled.method, headers: Object.fromEntries(compiled.headers), signal: combined }, response => {
        clearTimeout(connect);
        const read = async () => {
          const chunks: Buffer[] = []; let length = 0;
          for await (const chunk of response) {
            const bytes = Buffer.from(chunk); length += bytes.length;
            if (length > 1024 * 1024) { response.destroy(); throw new TableSyncError("table_sync_response_too_large"); }
            chunks.push(bytes);
          }
          const body = parseApplicationJson(Buffer.concat(chunks));
          if (response.statusCode === undefined) throw new TableSyncError("table_sync_invalid_response");
          return { status: response.statusCode, etag: response.headers.etag ?? null, retry_after: typeof response.headers["retry-after"] === "string" ? response.headers["retry-after"] : null, body };
        };
        void read().then(resolve, error => reject(combined.aborted ? combined.reason : error instanceof TableSyncError ? error : new TableSyncError(error instanceof SyntaxError ? "table_sync_invalid_response" : "table_sync_transport_unknown")));
      });
      const connect = setTimeout(() => outgoing.destroy(new TableSyncError("table_sync_transport_unknown")), 5000);
      outgoing.once("socket", socket => { if ("secureConnecting" in socket && !socket.secureConnecting) clearTimeout(connect); else socket.once("secureConnect", () => clearTimeout(connect)); });
      outgoing.once("error", () => { clearTimeout(connect); reject(combined.aborted ? combined.reason : new TableSyncError("table_sync_transport_unknown")); });
      outgoing.end(compiled.body);
    });
    await this.#identities.assertCurrent(identity); signal.throwIfAborted();
    return reply;
  }
}
