import { timingSafeEqual } from "node:crypto";
import { createServer, type ServerResponse } from "node:http";
import type { Socket } from "node:net";
import { getRequestListener } from "@hono/node-server";
import { Hono } from "hono";
import { bodyLimit } from "hono/body-limit";
import { getCookie, setCookie } from "hono/cookie";
import { z } from "zod";
import type { OperationRegistry } from "../application/operation-registry.ts";
import { browserCookieName, type BrowserSessions } from "./browser-sessions.ts";
import { parseApplicationJson, stringifyApplicationJson } from "../contracts/application-json.ts";
import type { WebAssets } from "./web-assets.ts";
import type { BootstrapSessions } from "./bootstrap-sessions.ts";
import { getPackagedRuntime } from "../packaging/runtime-context.ts";

interface LocalServerOptions {
  port: number;
  instanceId: string;
  controlToken: string;
  operations: OperationRegistry;
  requestShutdown?: () => boolean;
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
}

export async function listenLocalServer(options: LocalServerOptions) {
  if (!Number.isInteger(options.port) || options.port < 0 || options.port > 65535) throw new Error("Invalid loopback port");
  if (Buffer.byteLength(options.controlToken) < 32 || !options.instanceId) throw new Error("Local instance identity and strong control token are required");
  const token = Buffer.from(`Bearer ${options.controlToken}`);
  const app = new Hono();
  const packaged = getPackagedRuntime();
  const productVersion = packaged ? packaged.manifest.productVersion : "0.9.0";
  const libraryAssetPath = (path: string): string | null => {
    if (!options.libraryWebAssets) return null;
    if (["/library", "/library/", "/connections", "/login", "/auth/complete"].includes(path)) return "/";
    if (!path.startsWith("/library/")) return null;
    const asset = path.slice("/library".length);
    if (options.libraryWebAssets.has(asset)) return asset;
    return /^\/library\/[^/.]+$/u.test(path) ? "/" : null;
  };
  const eventResponses = new Set<ServerResponse>();
  const connections = new Map<Socket, number>();
  let closing = false;
  const listener = getRequestListener(app.fetch);
  const server = createServer((request, response) => {
    const socket = request.socket;
    connections.set(socket, (connections.get(socket) ?? 0) + 1);
    response.once("close", () => {
      const remaining = (connections.get(socket) ?? 1) - 1;
      if (connections.has(socket)) connections.set(socket, remaining);
      if (closing && remaining === 0) socket.destroy();
    });
    const path = new URL(request.url!, "http://127.0.0.1").pathname;
    if (path === "/api/events" || /^\/api\/v1\/dynamic-tables\/[^/]+\/creator-updates$/u.test(path)) {
      eventResponses.add(response);
      response.once("close", () => eventResponses.delete(response));
    }
    listener(request, response);
  });
  server.on("connection", socket => {
    connections.set(socket, 0);
    socket.once("close", () => connections.delete(socket));
    if (closing) socket.destroy();
  });
  let origin: string;

  app.use("*", async (context, next) => {
    context.header("Cache-Control", "no-store");
    context.header("X-Content-Type-Options", "nosniff");
    if (!origin) throw new Error("Local server has not bound its origin");
    const expected = new URL(origin);
    if (context.req.header("host") !== expected.host || new URL(context.req.url).origin !== origin) {
      return context.json({ error: { code: "local.invalid_host" } }, 403);
    }
    const requestOrigin = context.req.header("origin");
    if (requestOrigin !== undefined && requestOrigin !== origin) return context.json({ error: { code: "local.invalid_origin" } }, 403);
    if (["GET", "HEAD"].includes(context.req.method) && options.webAssets?.has(context.req.path)) return next();
    if (["GET", "HEAD"].includes(context.req.method) && libraryAssetPath(context.req.path) !== null) return next();
    const authorization = context.req.header("authorization");
    if (authorization !== undefined) {
      const presented = Buffer.from(authorization);
      if (presented.length !== token.length || !timingSafeEqual(presented, token)) return context.json({ error: { code: "local.unauthorized" } }, 401);
    } else if (options.browserSessions && !context.req.path.startsWith("/_local/")) {
      if (context.req.path !== "/api/local/session") {
        const write = !["GET", "HEAD"].includes(context.req.method);
        if (write && requestOrigin !== origin) return context.json({ error: { code: "local.invalid_origin" } }, 403);
        const cookie = getCookie(context, browserCookieName(origin));
        if (options.bootstrapSessions?.has(cookie)) {
          options.bootstrapSessions.authorize(cookie, context.req.header("x-onecrew-csrf"), write);
          const allowed = context.req.path === "/api/session" && context.req.method === "GET" || context.req.method === "POST" && context.req.path.startsWith("/api/commands/") && options.bootstrapCommands?.includes(context.req.path.slice("/api/commands/".length));
          if (!allowed) return context.json({ error: { code: "local.bootstrap_scope" } }, 403);
          return next();
        }
        const session = options.browserSessions.authorize(getCookie(context, browserCookieName(origin)), context.req.header("x-onecrew-csrf"), write);
        const cookieRead = context.req.method === "GET" && ["/api/events", "/api/session"].includes(context.req.path);
        if (!cookieRead && (context.req.header("x-onecrew-account-id") !== session.principal.user.id || context.req.header("x-onecrew-auth-epoch") !== session.epoch)) return context.json({ error: { code: "access.account_changed" } }, 409);
      } else if (requestOrigin !== origin) return context.json({ error: { code: "local.invalid_origin" } }, 403);
    } else {
      return context.json({ error: { code: "local.unauthorized" } }, 401);
    }
    return next();
  });
  const commandLimit = bodyLimit({ maxSize: 1024 * 1024, onError: context => context.json({ error: { code: "request.too_large" } }, 413) });
  const tableLimit = bodyLimit({ maxSize: 2 * 1024 * 1024, onError: context => context.json({ error: { code: "request.too_large" } }, 413) });
  const importLimit = bodyLimit({ maxSize: 10 * 1024 * 1024 + 64 * 1024, onError: context => context.json({ error: { code: "request.too_large" } }, 413) });
  const creatorImportLimit = bodyLimit({ maxSize: 32 * 1024 * 1024, onError: context => context.json({ ok: false, error: "request.too_large" }, 413) });
  app.use("*", (context, next) => {
    if (context.req.path === "/api/creator-import") return creatorImportLimit(context, next);
    if (context.req.path === "/api/v1/dynamic-tables/import") return importLimit(context, next);
    if (context.req.path === "/api/v1/dynamic-tables" || context.req.path.startsWith("/api/v1/dynamic-tables/")) return tableLimit(context, next);
    return commandLimit(context, next);
  });
  if (options.browserSessions) app.post("/api/local/session", async context => {
    if (context.req.header("content-type")?.split(";")[0]?.trim() !== "application/json") return context.json({ error: { code: "request.json_required" } }, 415);
    const { ticket } = z.object({ ticket: z.string() }).strict().parse(await context.req.json());
    const existingCookie = getCookie(context, browserCookieName(origin));
    const session = options.bootstrapSessions?.acceptsTicket(ticket) ? options.bootstrapSessions.exchange(ticket, existingCookie) : options.browserSessions!.exchange(ticket, existingCookie);
    setCookie(context, browserCookieName(origin), session.cookie, { httpOnly: true, sameSite: "Strict", path: "/", maxAge: 8 * 60 * 60 });
    return context.json("scope" in session ? { csrf: session.csrf, scope: session.scope } : { csrf: session.csrf, accountId: session.accountId, epoch: session.epoch });
  });
  if (options.browserSessions) app.get("/api/session", async context => {
    const cookie = getCookie(context, browserCookieName(origin));
    if (options.bootstrapSessions?.has(cookie)) {
      const bootstrap = options.bootstrapSessions.authorize(cookie, undefined, false);
      const ticket = bootstrap.flowId && options.promoteBootstrap ? await options.promoteBootstrap(bootstrap.flowId) : undefined;
      if (!ticket) return context.json({ ok: true, result: { csrf: bootstrap.csrf, version: productVersion, transport: "local_http", protocol_version: 1, scope: "bootstrap" } });
      const promoted = options.browserSessions!.exchange(ticket);
      options.bootstrapSessions.revoke(cookie);
      setCookie(context, browserCookieName(origin), promoted.cookie, { httpOnly: true, sameSite: "Strict", path: "/", maxAge: 8 * 60 * 60 });
      return context.json({ ok: true, result: { csrf: promoted.csrf, accountId: promoted.accountId, epoch: promoted.epoch, version: productVersion, transport: "local_http", protocol_version: 2 } });
    }
    const session = options.browserSessions!.readSession(cookie);
    setCookie(context, browserCookieName(origin), cookie!, { httpOnly: true, sameSite: "Strict", path: "/", maxAge: 8 * 60 * 60 });
    return context.json({ ok: true, result: { ...session, version: productVersion, transport: "local_http", protocol_version: 2 } });
  });
  app.get("/_local/status", context => context.json({
    service: "onecrew_lite", protocol_version: 1, instance_id: options.instanceId,
    migration_state: "development", production_ready: false,
    registered_operations: options.operations.names(),
    web_assets_sha256: options.webAssets?.fingerprint() ?? null,
    resource_identity: options.resourceIdentity ?? null,
  }));
  if (options.issueBrowserTicket) app.post("/_local/workspace", async context => {
    if (!options.webAssets) return context.json({ error: { code: "workspace.assets_unavailable" } }, 503);
    const ticket = await options.issueBrowserTicket!();
    return context.json({ url: `${origin}/#onecrew_ticket=${encodeURIComponent(ticket)}`, instance_id: options.instanceId });
  });
  if (options.issueBrowserTicket) app.post("/_local/library/workspace", async context => {
    if (!options.libraryWebAssets) return context.json({ error: { code: "workspace.library_assets_unavailable" } }, 503);
    const ticket = await options.issueBrowserTicket!();
    return context.json({ url: `${origin}/library#onecrew_ticket=${encodeURIComponent(ticket)}`, instance_id: options.instanceId });
  });
  app.get("/_local/library/status", context => context.json({ ready: options.libraryWebAssets !== undefined, web_assets_sha256: options.libraryWebAssets?.fingerprint() ?? null, instance_id: options.instanceId }));
  if (options.requestShutdown) app.post("/_local/shutdown", context => {
    if (!options.requestShutdown!()) return context.json({ error: { code: "runtime.not_ready" } }, 503);
    return context.json({ stopping: true, instance_id: options.instanceId }, 202);
  });
  for (const route of ["/api/operations/:name", "/api/commands/:name"]) {
    app.post(route, async context => {
      if (context.req.header("content-type")?.split(";")[0]?.trim() !== "application/json") {
        return context.json({ error: { code: "request.json_required" } }, 415);
      }
      const name = z.string().min(1).parse(context.req.param("name"));
      const cookie = getCookie(context, browserCookieName(origin));
      if (route === "/api/commands/:name" && options.bootstrapSessions?.has(cookie)) {
        if (!options.executeBootstrap || !options.bootstrapCommands?.includes(name)) return context.json({ error: { code: "local.bootstrap_scope" } }, 403);
        const input = parseApplicationJson(new Uint8Array(await context.req.arrayBuffer()));
        const result = await options.executeBootstrap(name, input);
        if (name === "creator_sync_pairing_begin") {
          const bound = z.object({ projection: z.object({ flow_id: z.string().min(1) }) }).parse(result);
          options.bootstrapSessions.bindPairing(cookie, bound.projection.flow_id);
        }
        return context.body(stringifyApplicationJson({ ok: true, result }), 200, { "Content-Type": "application/json; charset=UTF-8" });
      }
      const accountId = z.string().min(1).parse(context.req.header("x-onecrew-account-id"));
      const epoch = z.uuid().parse(context.req.header("x-onecrew-auth-epoch"));
      const input = parseApplicationJson(new Uint8Array(await context.req.arrayBuffer()));
      const result = await options.operations.execute(name, input, accountId, epoch);
      return context.body(stringifyApplicationJson(route === "/api/commands/:name" ? { ok: true, result } : result), 200, { "Content-Type": "application/json; charset=UTF-8" });
    });
  }
  app.post("/api/creator-import", async context => {
    const encodedName = context.req.header("x-onecrew-file-name");
    if (encodedName === undefined || Buffer.byteLength(encodedName) > 768) return context.json({ ok: false, error: "missing_workbook_file_name" });
    const fileName = new URLSearchParams(`name=${encodedName}`).get("name");
    if (fileName === null) return context.json({ ok: false, error: "invalid_file_name" });
    const accountId = z.string().min(1).parse(context.req.header("x-onecrew-account-id"));
    const epoch = z.uuid().parse(context.req.header("x-onecrew-auth-epoch"));
    const result = await options.operations.execute("onecrew_creator_import_file_prepare", {
      file_name: fileName, bytes: new Uint8Array(await context.req.arrayBuffer()),
    }, accountId, epoch);
    return context.body(stringifyApplicationJson({ ok: true, result }), 200, { "Content-Type": "application/json; charset=UTF-8" });
  });
  options.configureRoutes?.(app);
  if (options.libraryWebAssets) app.all("*", async (context, next) => {
    const asset = libraryAssetPath(context.req.path);
    if (asset === null || !["GET", "HEAD"].includes(context.req.method)) return next();
    return options.libraryWebAssets!.response(asset, context.req.method);
  });
  if (options.webAssets) app.on(["GET", "HEAD"], "*", context => {
    if (!options.webAssets!.has(context.req.path)) return context.notFound();
    return options.webAssets!.response(context.req.path, context.req.method);
  });
  app.onError((error, context) => {
    const failure = (code: string) => context.req.path.startsWith("/api/commands/") || context.req.path === "/api/creator-import" ? { ok: false, error: code } : { error: { code } };
    if (context.req.path.startsWith("/api/commands/onecrew_managed_browser_connection_") && ["browser.managed_challenge_tab_required", "browser.managed_challenge_mismatch", "browser.owned_process_not_running", "browser.connection_expired"].includes(error.message)) return context.json(failure(error.message), 422);
    if (error instanceof z.ZodError || error instanceof SyntaxError || error instanceof TypeError && "code" in error && error.code === "ERR_ENCODING_INVALID_ENCODED_DATA") return context.json(failure("request.invalid_input"), 400);
    if ("code" in error && "status" in error) {
      const status = error.status;
      if (status === 400 || status === 401 || status === 403 || status === 404 || status === 409 || status === 410 || status === 422 || status === 429 || status === 502 || status === 503 || status === 504) return context.json(failure(String(error.code)), status);
    }
    return context.json(failure("operation.failed"), 500);
  });

  await new Promise<void>((resolve, reject) => {
    server.once("error", reject);
    server.listen(options.port, "127.0.0.1", () => {
      server.off("error", reject);
      const address = server.address();
      if (address === null || typeof address === "string") throw new Error("Local server did not bind TCP");
      origin = `http://127.0.0.1:${address.port}`;
      resolve();
    });
  });
  return {
    origin: origin!,
    async close(): Promise<void> {
      closing = true;
      const closed = new Promise<void>((resolve, reject) => server.close(error => error ? reject(error) : resolve()));
      // 停机时取消只读事件流；业务请求和已经开始的回执仍由原所有者排空。
      for (const response of eventResponses) response.destroy();
      for (const [socket, active] of connections) if (active === 0) socket.destroy();
      await closed;
      token.fill(0);
    },
  };
}
