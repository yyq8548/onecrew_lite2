import { resolveEmailLocale } from "../auth/email-locale";
import { installInstructions, resolveInstallLocale } from "./prompts";
import { Hono, type Context } from "hono";
import { getCookie, setCookie, deleteCookie } from "hono/cookie";
import { bodyLimit } from "hono/body-limit";
import type { AppEnv } from "../types/bindings";
import { authJsonResponse, loadSafeSession } from "../auth/session";
import { DistributionError } from "../distribution/contracts";
import { parseAccessOrigin } from "../config/env";
import { authorizeLegacyTicket, authorizeUpdate, authorizeInstallEntry, authorizeDownload, beginDownload, issueInstallLink, restoreInstallLink, streamBinary } from "./service";
import { INSTALL_COOKIE } from "./credentials";
import { IntegrationError } from "../devices/errors";
import { manifestSchema, legacyTicketPattern, type CliManifest } from "./contracts";
import macTemplate from "./install.sh.txt";
import windowsTemplate from "./install.ps1.txt";

function supportsBootstrapUpdate(version: string) {
  const [major, minor, patch] = version.split(".").map(BigInt);
  return major! > 0n || minor! > 5n || (minor === 5n && patch! >= 22n);
}

export function renderInstaller(template: string, manifest: CliManifest, baseUrl: string) {
  // Substitutions enter shell/PowerShell single-quoted literals. Accept only the
  // canonical HTTPS gateway path, never arbitrary or request-controlled URLs.
  if (!/^https:\/\/[A-Za-z0-9.-]+(?::[0-9]+)?\/cli\/(?:install\/[A-Za-z0-9_-]{43}|download\/v1\.[A-Za-z0-9_-]{60,1536})$/.test(baseUrl) || !manifestSchema.safeParse(manifest).success) throw new DistributionError("release_unavailable", 409);
  const names = { "aarch64-apple-darwin": "MAC_ARM", "x86_64-apple-darwin": "MAC_INTEL", "x86_64-pc-windows-msvc": "WINDOWS" } as const;
  const supportsBootstrap = supportsBootstrapUpdate(manifest.version);
  let result = template.replaceAll("__ONECREW_VERSION__", manifest.version)
    .replaceAll("__ONECREW_BOOTSTRAP_UPDATE__", supportsBootstrap ? "1" : "0");
  for (const asset of manifest.assets) {
    const name = names[asset.target];
    result = result.replaceAll(`__ONECREW_${name}_URL__`, `${baseUrl}/files/${asset.target}`).replaceAll(`__ONECREW_${name}_SHA256__`, asset.sha256).replaceAll(`__ONECREW_${name}_SIZE__`, String(asset.size));
  }
  if (/__ONECREW_[A-Z0-9_]+__/.test(result)) throw new DistributionError("release_unavailable", 409);
  return result;
}
export const cliRoutes = new Hono<AppEnv>();
for (const path of ["/cli/*", "/api/cli/*"]) cliRoutes.use(path, async (c, next) => {
  await next();
  c.header("Cache-Control", "no-store"); c.header("Referrer-Policy", "no-referrer"); c.header("X-Content-Type-Options", "nosniff");
});
cliRoutes.use("/api/cli/update*", async (c, next) => {
  await next();
  c.header("Vary", "Authorization");
});
cliRoutes.use("/api/cli/install-links", async (c, next) => {
  await next();
  c.header("Vary", "Cookie, Accept-Language", { append: true });
});
cliRoutes.onError(error => {
  const response = error instanceof DistributionError || error instanceof IntegrationError ? authJsonResponse({error: error.code}, error.status) : authJsonResponse({error: "unavailable"}, 503);
  if (error instanceof DistributionError && error.code === "rate_limited") response.headers.set("Retry-After", "60");
  return response;
});
cliRoutes.get("/api/cli/update", async c => {
  const manifest = await authorizeUpdate(c.env, c.req.raw);
  const origin = parseAccessOrigin(c.env);
  return authJsonResponse({
    schema_version: manifest.schema_version, product: manifest.product, version: manifest.version,
    assets: manifest.assets.map(({target, size, sha256}) => ({target, size, sha256,
      url: `${origin}/api/cli/update/${manifest.version}/files/${target}`})),
  }, 200);
});
cliRoutes.on(["GET", "HEAD"], "/api/cli/update/:version/files/:target", async c => {
  const manifest = await authorizeUpdate(c.env, c.req.raw);
  // Re-select metadata when the release pin changes between check and download.
  if (manifest.version !== c.req.param("version")) throw new DistributionError("release_unavailable", 409);
  return streamBinary(c.env, manifest, c.req.param("target"), c.req.method === "HEAD");
});
async function browserUser(c: Context<AppEnv>) {
  const session = await loadSafeSession(c.get("auth"), c.req.raw.headers);
  if (!session.session) throw new DistributionError("authentication_required", 401);
  return session.session.user.id;
}
const cookieOptions = { path: "/", secure: true, httpOnly: true, sameSite: "Strict" as const };
cliRoutes.get("/api/cli/install-links", async c => {
  const userId = await browserUser(c);
  const token = getCookie(c, INSTALL_COOKIE);
  const link = await restoreInstallLink(c.env, userId, token, resolveEmailLocale(c.req.raw));
  if (token && !link) deleteCookie(c, INSTALL_COOKIE, cookieOptions);
  return authJsonResponse({ link }, 200);
});
cliRoutes.post("/api/cli/install-links", bodyLimit({ maxSize: 1024 }), async c => {
  if (c.req.header("Origin") !== c.get("config").authOrigin) throw new DistributionError("origin_not_allowed", 403);
  if (c.req.header("Content-Type")?.split(";")[0] !== "application/json") throw new DistributionError("invalid_request", 400);
  const body: unknown = await c.req.json().catch(() => null);
  if (!body || Array.isArray(body) || typeof body !== "object" || Object.keys(body).length) throw new DistributionError("invalid_request", 400);
  const userId = await browserUser(c);
  const link = await issueInstallLink(c.env, userId, getCookie(c, INSTALL_COOKIE), resolveEmailLocale(c.req.raw));
  setCookie(c, INSTALL_COOKIE, link.url.split("/").at(-1)!, { ...cookieOptions,
    maxAge: Math.max(0, Math.floor((link.expiresAt - Date.now()) / 1000)) });
  return authJsonResponse(link, 200);
});
cliRoutes.on(["GET", "HEAD"], "/cli/install/:token", async c => {
  const token = c.req.param("token");
  const legacy = legacyTicketPattern.test(token);
  const selection = legacy
    ? { ...await authorizeLegacyTicket(c.env, token), token }
    : await beginDownload(c.env, await authorizeInstallEntry(c.env, token));
  const { manifest, expiresAt } = selection;
  const base = `${parseAccessOrigin(c.env)}/cli/${legacy ? "install" : "download"}/${selection.token}`;
  const locale = resolveInstallLocale(c.req.raw);
  const text = installInstructions(locale, manifest.version, expiresAt, base, supportsBootstrapUpdate(manifest.version));
  return new Response(c.req.method === "HEAD" ? null : text, {headers: {
    "Content-Type": "text/markdown; charset=utf-8", "Content-Language": locale, "Vary": "Accept-Language",
  }});
});
for (const [filename, template] of [["install.sh", macTemplate], ["install.ps1", windowsTemplate]] as const) {
  cliRoutes.on(["GET", "HEAD"], `/cli/install/:token/${filename}`, async c => {
    const {manifest} = await authorizeLegacyTicket(c.env, c.req.param("token"));
    const text = renderInstaller(template, manifest, `${parseAccessOrigin(c.env)}/cli/install/${c.req.param("token")}`);
    return new Response(c.req.method === "HEAD" ? null : text, {headers: {"Content-Type": "text/plain; charset=utf-8"}});
  });
  cliRoutes.on(["GET", "HEAD"], `/cli/download/:token/${filename}`, async c => {
    const {manifest} = await authorizeDownload(c.env, c.req.param("token"));
    const text = renderInstaller(template, manifest, `${parseAccessOrigin(c.env)}/cli/download/${c.req.param("token")}`);
    return new Response(c.req.method === "HEAD" ? null : text, {headers: {"Content-Type": "text/plain; charset=utf-8"}});
  });
}
cliRoutes.on(["GET", "HEAD"], "/cli/install/:token/files/:target", async c => {
  const {manifest} = await authorizeLegacyTicket(c.env, c.req.param("token"));
  return streamBinary(c.env, manifest, c.req.param("target"), c.req.method === "HEAD");
});
cliRoutes.on(["GET", "HEAD"], "/cli/download/:token/files/:target", async c => {
  const {manifest} = await authorizeDownload(c.env, c.req.param("token"));
  return streamBinary(c.env, manifest, c.req.param("target"), c.req.method === "HEAD");
});
