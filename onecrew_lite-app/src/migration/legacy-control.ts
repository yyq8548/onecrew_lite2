import { constants, lstatSync } from "node:fs";
import { createHash } from "node:crypto";
import { lstat, open, realpath } from "node:fs/promises";
import { createConnection } from "node:net";
import { join, relative, resolve, toNamespacedPath } from "node:path";
import { z } from "zod";
import { parseApplicationJson } from "../contracts/application-json.ts";
import { verifyPrivateDirectory, verifyPrivateFile } from "../storage/private-backup.ts";
import { verifyLegacyPrivateFile, verifyLegacyPrivateDirectory } from "../storage/legacy-private-source.ts";
import { verifyLegacyWindowsProcess, type LegacyWindowsRegistration } from "./windows-registration.ts";

const endpointSchema = z.object({ protocol: z.literal(1), version: z.literal("0.7.1"), binary_id: z.string().regex(/^[a-f0-9]{64}$/u),
  pid: z.number().int().min(1).max(2147483647), port: z.number().int().min(1).max(65535), web_port: z.number().int().min(1).max(65535),
  token: z.string().regex(/^[A-Za-z0-9_-]{43}$/u) }).strict();
type Endpoint = z.infer<typeof endpointSchema>;

export async function deriveLegacyWindowsWebPort(dataDirectory: string): Promise<number> {
  if (process.platform !== "win32") throw new Error("migration.windows_origin_platform_unsupported");
  const canonical = await realpath(dataDirectory);
  if (!canonical.isWellFormed()) throw new Error("migration.legacy_path_encoding_unsupported");
  const bytes = createHash("sha256").update(Buffer.from(toNamespacedPath(canonical), "utf8")).digest();
  return 22000 + bytes.readUInt16BE(0) % 10000;
}

async function readDescriptor(path: string, limit: number, nativeRust = false): Promise<unknown> {
  if (nativeRust) await verifyLegacyPrivateFile(path);
  else await verifyPrivateFile(path);
  const before = await lstat(path, { bigint: true });
  await using file = await open(path, constants.O_RDONLY | (constants.O_NOFOLLOW ?? 0));
  const opened = await file.stat({ bigint: true });
  if (opened.ino !== before.ino || opened.dev !== before.dev || opened.nlink !== 1n || opened.size > BigInt(limit)) throw new Error("migration.legacy_descriptor_changed");
  const bytes = await file.readFile();
  try {
    const after = await file.stat({ bigint: true }), current = await lstat(path, { bigint: true });
    if (current.isSymbolicLink() || current.ino !== opened.ino || current.dev !== opened.dev || current.nlink !== 1n || after.size !== opened.size || after.mtimeNs !== opened.mtimeNs || after.ctimeNs !== opened.ctimeNs) throw new Error("migration.legacy_descriptor_changed");
    return parseApplicationJson(bytes);
  } finally { bytes.fill(0); }
}

async function request(endpoint: Endpoint, command: "ping" | "stop", signal: AbortSignal): Promise<unknown> {
  signal.throwIfAborted();
  return new Promise((resolveReply, rejectReply) => {
    const socket = createConnection({ host: "127.0.0.1", port: endpoint.port });
    const chunks: Buffer[] = [];
    let size = 0, settled = false;
    function finish(error?: Error, value?: unknown) {
      if (settled) return; settled = true;
      signal.removeEventListener("abort", aborted); socket.destroy();
      for (const chunk of chunks) chunk.fill(0);
      if (error) rejectReply(error); else resolveReply(value);
    }
    const aborted = () => finish(new Error("migration.legacy_control_cancelled"));
    signal.addEventListener("abort", aborted, { once: true });
    socket.setTimeout(120_000, () => finish(new Error("migration.legacy_control_timeout")));
    socket.once("error", () => finish(new Error("migration.legacy_control_unavailable")));
    socket.once("end", () => finish(new Error("migration.legacy_control_incomplete")));
    socket.once("connect", () => socket.write(JSON.stringify({ protocol: 1, token: endpoint.token, mode: "control", command }) + "\n"));
    socket.on("data", bytes => {
      if (typeof bytes === "string") { finish(new Error("migration.legacy_control_encoding_invalid")); return; }
      if (settled) { bytes.fill(0); return; }
      size += bytes.length;
      if (size > 16 * 1024) { bytes.fill(0); finish(new Error("migration.legacy_control_limit")); return; }
      chunks.push(bytes);
      const newline = bytes.indexOf(10);
      if (newline < 0) return;
      const frame = Buffer.concat(chunks);
      void Promise.resolve().then(() => {
        const result = z.object({ ok: z.literal(true), result: z.unknown() }).strict().safeParse(parseApplicationJson(frame));
        if (!result.success) { finish(new Error("migration.legacy_control_rejected")); return; }
        finish(undefined, result.data.result);
      }).then(undefined, () => finish(new Error("migration.legacy_control_rejected"))).finally(() => frame.fill(0));
    });
  });
}

// 仅连接已经存在的旧服务；从不启动、重试启动或结束未经验证的 PID。
export async function stopLegacyWindowsService(input: LegacyWindowsRegistration, signal: AbortSignal) {
  signal.throwIfAborted(); await verifyLegacyPrivateDirectory(input.dataDirectory);
  const path = join(input.dataDirectory, ".onecrew_lite-service.json");
  if (!lstatSync(path, { throwIfNoEntry: false })) return Object.freeze({ state: "not_discovered" as const });
  const endpoint = endpointSchema.parse(await readDescriptor(path, 8192, true));
  if (endpoint.binary_id !== input.executableSha256) throw new Error("migration.legacy_binary_identity_changed");
  if (endpoint.web_port !== await deriveLegacyWindowsWebPort(input.dataDirectory)) throw new Error("migration.legacy_origin_changed");
  await verifyLegacyWindowsProcess(input, endpoint.pid);
  const ping = z.object({ version: z.literal("0.7.1"), binary_id: z.literal(endpoint.binary_id), pid: z.literal(endpoint.pid) }).strict().parse(await request(endpoint, "ping", signal));
  const current = endpointSchema.parse(await readDescriptor(path, 8192, true));
  if (JSON.stringify(current) !== JSON.stringify(endpoint)) throw new Error("migration.legacy_descriptor_changed");
  await verifyLegacyWindowsProcess(input, ping.pid);
  const stopped = z.object({ stopping: z.literal(true), data_preserved: z.literal(true) }).strict().parse(await request(endpoint, "stop", signal));
  return Object.freeze({ state: "stop_requested" as const, pid: ping.pid, binarySha256: ping.binary_id, webPort: endpoint.web_port, ...stopped });
}

const librarySchema = z.object({ service: z.literal("onecrew_lite"), protocol_version: z.literal(1), pid: z.number().int().positive(), port: z.number().int().min(1).max(65535),
  url: z.string(), instance_id: z.uuid(), control_token: z.string().regex(/^[a-f0-9]{64}$/u), data_dir: z.string() }).strict();

export async function stopLegacyLibrary(libraryDirectory: string, signal: AbortSignal) {
  signal.throwIfAborted(); await verifyLegacyPrivateDirectory(libraryDirectory);
  const path = join(libraryDirectory, "run", "server.json");
  if (!lstatSync(path, { throwIfNoEntry: false })) return Object.freeze({ state: "not_discovered" as const });
  await verifyLegacyPrivateDirectory(join(libraryDirectory, "run"));
  const endpoint = librarySchema.parse(await readDescriptor(path, 8192));
  const origin = `http://127.0.0.1:${endpoint.port}`;
  if (endpoint.url !== origin || relative(resolve(endpoint.data_dir), resolve(libraryDirectory)) !== "") throw new Error("migration.legacy_library_identity_changed");
  async function call(route: string, method: "GET" | "POST") {
    const response = await fetch(`${origin}/_local/${route}`, { method, headers: { authorization: `Bearer ${endpoint.control_token}` }, redirect: "error",
      signal: AbortSignal.any([signal, AbortSignal.timeout(30_000)]) });
    if (!response.ok || !response.body) { await response.body?.cancel(); throw new Error("migration.legacy_library_control_rejected"); }
    const reader = response.body.getReader(), chunks: Uint8Array[] = [];
    let size = 0;
    try {
      while (true) {
        const chunk = await reader.read(); if (chunk.done) break;
        size += chunk.value.byteLength;
        if (size > 8192) { await reader.cancel(); throw new Error("migration.legacy_library_control_rejected"); }
        chunks.push(chunk.value);
      }
      return { status: response.status, result: parseApplicationJson(Buffer.concat(chunks)) };
    } finally { reader.releaseLock(); for (const bytes of chunks) bytes.fill(0); }
  }
  z.object({ instance_id: z.literal(endpoint.instance_id) }).strict().parse((await call("status", "GET")).result);
  if (JSON.stringify(librarySchema.parse(await readDescriptor(path, 8192))) !== JSON.stringify(endpoint)) throw new Error("migration.legacy_library_descriptor_changed");
  const shutdown = await call("shutdown", "POST");
  if (shutdown.status !== 202) throw new Error("migration.legacy_library_control_rejected");
  z.object({ stopping: z.literal(true) }).strict().parse(shutdown.result);
  return Object.freeze({ state: "stop_requested" as const, instanceId: endpoint.instance_id, pid: endpoint.pid });
}
