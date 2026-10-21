import { randomBytes, randomUUID } from "node:crypto";
import { lstatSync } from "node:fs";
import { open, readFile, realpath, rename, unlink } from "node:fs/promises";
import { join } from "node:path";
import { z } from "zod";
import type { OperationRegistry } from "../application/operation-registry.ts";
import { verifyPrivateDirectory, verifyPrivateFile } from "../storage/private-backup.ts";
import { listenLocalServer } from "./http.ts";
import { acquireInstanceLock, type InstanceLease } from "./instance-lock.ts";
import { loadOrCreateServiceProfile, type ServiceProfile } from "./profile.ts";
import type { BrowserSessions } from "./browser-sessions.ts";
import type { Hono } from "hono";
import { parseStoredJson } from "../contracts/stored-json.ts";
import type { WebAssets } from "./web-assets.ts";
import type { BootstrapSessions } from "./bootstrap-sessions.ts";

const descriptorSchema = z.object({
  service: z.literal("onecrew_lite"), protocol_version: z.literal(1),
  instance_id: z.uuid(), control_token: z.string().regex(/^[0-9a-f]{64}$/),
  pid: z.number().int().positive(), port: z.number().int().min(1).max(65535),
  data_dir: z.string().min(1), url: z.url(),
}).strict();
export type RuntimeDescriptor = Readonly<z.infer<typeof descriptorSchema>>;

export interface RuntimeResources {
  operations: OperationRegistry;
  browserSessions?: BrowserSessions;
  configureRoutes?: (app: Hono) => void;
  webAssets?: WebAssets;
  libraryWebAssets?: WebAssets;
  issueBrowserTicket?: () => Promise<string>;
  resourceIdentity?: Readonly<Record<string, unknown>>;
  bootstrapSessions?: BootstrapSessions;
  bootstrapCommands?: readonly string[];
  executeBootstrap?: (name: string, input: unknown) => Promise<unknown>;
  promoteBootstrap?: (flowId: string) => Promise<string | undefined>;
  stopAcceptingWork?: () => void;
  close?: () => Promise<void>;
}
export type RuntimeBootstrap = (lease: InstanceLease, profile: ServiceProfile) => Promise<RuntimeResources>;

export async function startLocalRuntime(dataDirectory: string, bootstrap: RuntimeBootstrap, legacyPort?: number) {
  const lease = await acquireInstanceLock(dataDirectory);
  let server: Awaited<ReturnType<typeof listenLocalServer>> | undefined;
  let resources: RuntimeResources | undefined;
  let started = false;
  let closeRuntime: (() => Promise<void>) | undefined;
  let finishRuntime!: () => void;
  const stopped = new Promise<void>(resolve => { finishRuntime = resolve; });
  try {
    const profile = await loadOrCreateServiceProfile(lease, legacyPort);
    resources = await bootstrap(lease, profile);
    const instanceId = randomUUID();
    const controlToken = randomBytes(32).toString("hex");
    server = await listenLocalServer({ port: profile.port, instanceId, controlToken, operations: resources.operations,
      ...(resources.browserSessions ? { browserSessions: resources.browserSessions } : {}),
      ...(resources.configureRoutes ? { configureRoutes: resources.configureRoutes } : {}),
      ...(resources.webAssets ? { webAssets: resources.webAssets } : {}),
      ...(resources.libraryWebAssets ? { libraryWebAssets: resources.libraryWebAssets } : {}),
      ...(resources.issueBrowserTicket ? { issueBrowserTicket: resources.issueBrowserTicket } : {}),
      ...(resources.resourceIdentity ? { resourceIdentity: resources.resourceIdentity } : {}),
      ...(resources.bootstrapSessions ? { bootstrapSessions: resources.bootstrapSessions } : {}),
      ...(resources.bootstrapCommands ? { bootstrapCommands: resources.bootstrapCommands } : {}),
      ...(resources.executeBootstrap ? { executeBootstrap: resources.executeBootstrap } : {}),
      ...(resources.promoteBootstrap ? { promoteBootstrap: resources.promoteBootstrap } : {}),
      requestShutdown: () => {
        if (!closeRuntime) return false;
        setImmediate(() => { void closeRuntime!(); });
        return true;
      },
    });
    const descriptor: RuntimeDescriptor = Object.freeze({ service: "onecrew_lite", protocol_version: 1,
      instance_id: instanceId, control_token: controlToken, pid: process.pid, port: profile.port,
      data_dir: profile.dataDirectory, url: server.origin });
    const path = join(profile.dataDirectory, "server.json");
    if (lstatSync(path, { throwIfNoEntry: false })) await verifyPrivateFile(path);
    const pending = join(profile.dataDirectory, `server-${instanceId}.pending`);
    const handle = await open(pending, "wx", 0o600);
    try {
      await verifyPrivateFile(pending);
      await handle.writeFile(JSON.stringify(descriptor));
      await handle.sync();
    } finally {
      await handle.close();
    }
    await rename(pending, path);
    await verifyPrivateFile(path);
    const runningServer = server;
    let closing: Promise<void> | undefined;
    started = true;
    closeRuntime = (): Promise<void> => {
      closing ??= (async () => {
        const failures: unknown[] = [];
        const attempt = async (action: () => void | Promise<void>) => {
          await Promise.resolve().then(action).then(undefined, error => { failures.push(error); });
        };
        try {
          await attempt(() => resources!.stopAcceptingWork?.());
          await attempt(() => runningServer.close());
          await attempt(() => resources!.close?.());
          await attempt(async () => {
            await verifyPrivateFile(path);
            const current = descriptorSchema.parse(parseStoredJson(new TextDecoder("utf-8", { fatal: true }).decode(await readFile(path)), true));
            if (current.instance_id !== instanceId) throw new Error("Runtime descriptor owner changed during shutdown");
            await unlink(path);
          });
        } finally {
          await attempt(() => lease.close());
          finishRuntime();
        }
        if (failures.length) throw new AggregateError(failures, "Unified runtime shutdown failed");
      })();
      return closing;
    };
    return { descriptor, stopped, close: closeRuntime };
  } finally {
    if (!started) {
      try { if (server) await server.close(); }
      finally {
        try { await resources?.close?.(); }
        finally { await lease.close(); }
      }
    }
  }
}

// 描述文件仅作为发现入口，状态必须回读并核对实例身份。
export async function readRuntimeDescriptor(dataDirectory: string): Promise<RuntimeDescriptor> {
  const directory = await realpath(await verifyPrivateDirectory(dataDirectory));
  const path = join(directory, "server.json");
  await verifyPrivateFile(path);
  const descriptor = descriptorSchema.parse(parseStoredJson(new TextDecoder("utf-8", { fatal: true }).decode(await readFile(path)), true));
  if (descriptor.data_dir !== directory || descriptor.url !== `http://127.0.0.1:${descriptor.port}`) {
    throw new Error("Runtime descriptor does not match this data directory and loopback port");
  }
  return Object.freeze(descriptor);
}

export async function readVerifiedRuntime(dataDirectory: string): Promise<RuntimeDescriptor> {
  const descriptor = await readRuntimeDescriptor(dataDirectory);
  const probe = async (timeout: number) => {
    const signal = AbortSignal.timeout(timeout);
    return fetch(`${descriptor.url}/_local/status`, { headers: { authorization: `Bearer ${descriptor.control_token}` }, redirect: "error", signal })
      .then(response => ({ ok: true as const, response }), error => ({ ok: false as const, error: error as unknown, timedOut: signal.aborted }));
  };
  let observed = await probe(2000);
  if (!observed.ok && observed.timedOut) {
    const current = await readRuntimeDescriptor(dataDirectory);
    if (current.instance_id !== descriptor.instance_id || current.url !== descriptor.url || current.control_token !== descriptor.control_token) throw new Error("Runtime instance identity mismatch");
    observed = await probe(8000);
  }
  if (!observed.ok) throw observed.error;
  const response = observed.response;
  if (!response.ok) throw new Error(`Runtime identity verification returned HTTP ${response.status}`);
  const status = z.object({ service: z.literal("onecrew_lite"), protocol_version: z.literal(1), instance_id: z.uuid() }).parse(await response.json());
  if (status.instance_id !== descriptor.instance_id) throw new Error("Runtime instance identity mismatch");
  return Object.freeze(descriptor);
}
