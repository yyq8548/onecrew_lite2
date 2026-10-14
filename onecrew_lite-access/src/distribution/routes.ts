import { Hono, type Context } from "hono";
import { attributionContext } from "../analytics/service";
import { z } from "zod";
import { resolveEmailLocale } from "../auth/email-locale";
import { authJsonResponse, loadSafeSession } from "../auth/session";
import type { AppEnv } from "../types/bindings";
import { IntegrationError } from "../devices/errors";
import { verifyDeviceCredential } from "../devices/credential-identity";
import { DistributionError, tokenPattern, versionPattern } from "./contracts";
import {
  applicationProjection,
  authorizeLink,
  createUserLink,
  entitlement,
  submitApplication,
} from "./service";
import { publisherAuthorized } from "./crypto";
import {
  compareVersions,
  getAsset,
  latestRelease,
  publishRelease,
  readRelease,
  streamAsset,
} from "./releases";
async function user(c: Context<AppEnv>) {
  const result = await loadSafeSession(c.get("auth"), c.req.raw.headers);
  if (!result.session)
    throw new DistributionError("authentication_required", 401);
  const current = await c.env.DB.prepare(
    'SELECT "emailVerified" AS verified FROM "user" WHERE id=?',
  )
    .bind(result.session.user.id)
    .first<{ verified: number }>();
  if (!current?.verified)
    throw new DistributionError("email_verification_required", 403);
  return result.session.user.id;
}
function requestId(c: Context<AppEnv>) {
  const parsed = z.string().uuid().safeParse(c.req.header("Idempotency-Key"));
  if (!parsed.success)
    throw new DistributionError("idempotency_key_required", 400);
  return parsed.data;
}
async function body(c: Context<AppEnv>) {
  if (c.req.header("Content-Type")?.split(";", 1)[0] !== "application/json")
    throw new DistributionError("invalid_request", 400);
  const declared = Number(c.req.header("Content-Length") ?? 0);
  if (declared > 65536) throw new DistributionError("invalid_request", 400);
  const reader = c.req.raw.body?.getReader();
  if (!reader) throw new DistributionError("invalid_request", 400);
  const parts: Uint8Array[] = [];
  let length = 0;
  try {
    while (true) {
      const part = await reader.read();
      if (part.done) break;
      length += part.value.length;
      if (length > 65536) throw new DistributionError("invalid_request", 400);
      parts.push(part.value);
    }
  } finally {
    await reader.cancel().catch(() => undefined);
  }
  const bytes = new Uint8Array(length);
  let offset = 0;
  for (const part of parts) {
    bytes.set(part, offset);
    offset += part.length;
  }
  try {
    return JSON.parse(new TextDecoder().decode(bytes));
  } catch {
    throw new DistributionError("invalid_request", 400);
  }
}
function browserWrite(c: Context<AppEnv>) {
  if (c.req.header("Origin") !== c.get("config").authOrigin)
    throw new DistributionError("origin_not_allowed", 403);
}
async function desktop(c: Context<AppEnv>) {
  const owner = await verifyDeviceCredential(c.env, c.env.DB, c.req.raw);
  if ((await entitlement(c.env.DB, owner.userId))?.status !== "active")
    throw new DistributionError("entitlement_revoked", 403);
  return owner;
}
export const distributionRoutes = new Hono<AppEnv>();
for (const prefix of ["/api/distribution/*", "/api/desktop/distribution/*"])
  distributionRoutes.use(prefix, async (c, next) => {
    await next();
    c.header("Cache-Control", "no-store");
    c.header("Referrer-Policy", "no-referrer");
  });
distributionRoutes.onError((error) => {
  if (error instanceof DistributionError || error instanceof IntegrationError)
    return authJsonResponse({ error: error.code }, error.status);
  if (error instanceof z.ZodError)
    return authJsonResponse({ error: "invalid_request" }, 400);
  return authJsonResponse({ error: "unavailable" }, 503);
});
distributionRoutes.get("/api/distribution/application", async (c) =>
  authJsonResponse(await applicationProjection(c.env, await user(c)), 200),
);
distributionRoutes.post("/api/distribution/application", async (c) => {
  browserWrite(c);
  return authJsonResponse(
    await submitApplication(
      c.env,
      await user(c),
      await body(c),
      requestId(c),
      resolveEmailLocale(c.req.raw),
      attributionContext(c.req.raw.headers),
    ),
    200,
  );
});
distributionRoutes.post("/api/distribution/download-links", async (c) => {
  browserWrite(c);
  z.object({})
    .strict()
    .parse(await body(c));
  return authJsonResponse(
    await createUserLink(c.env, await user(c), requestId(c)),
    200,
  );
});
distributionRoutes.get("/api/distribution/download-links/:token", async (c) => {
  const uid = await user(c),
    token = c.req.param("token");
  if (!tokenPattern.test(token))
    throw new DistributionError("link_not_found", 404);
  const link = await authorizeLink(c.env, uid, token);
  const release = await latestRelease(c.env);
  if (!release) throw new DistributionError("release_unavailable", 409);
  const assets = await c.env.DB.prepare(
    "SELECT id,filename,platform,arch,kind,size FROM distribution_asset WHERE version=? AND kind='installer' ORDER BY platform,arch",
  )
    .bind(release.version)
    .all();
  return authJsonResponse(
    {
      expiresAt: link.expires_at,
      release,
      assets: assets.results.map((a) => ({
        ...a,
        downloadUrl: `/api/distribution/download-links/${token}/assets/${a.id}`,
      })),
    },
    200,
  );
});
distributionRoutes.on(
  ["GET", "HEAD"],
  "/api/distribution/download-links/:token/assets/:assetId",
  async (c) => {
    const uid = await user(c);
    await authorizeLink(c.env, uid, c.req.param("token"));
    const asset = await getAsset(c.env, c.req.param("assetId"));
    if (!asset || asset.kind !== "installer")
      throw new DistributionError("not_found", 404);
    return streamAsset(c.env, c.req.raw, asset);
  },
);
distributionRoutes.post("/api/distribution/publisher/releases", async (c) => {
  await publisherAuthorized(c.env, c.req.raw);
  return authJsonResponse(
    await publishRelease(c.env, await body(c), requestId(c)),
    200,
  );
});
distributionRoutes.get(
  "/api/distribution/publisher/releases/:version",
  async (c) => {
    await publisherAuthorized(c.env, c.req.raw);
    return authJsonResponse(
      { ok: true, release: await readRelease(c.env, c.req.param("version")) },
      200,
    );
  },
);
distributionRoutes.get(
  "/api/desktop/distribution/update/:target/:arch/:currentVersion",
  async (c) => {
    await desktop(c);
    const { target, arch, currentVersion } = c.req.param();
    if (
      !["darwin", "windows", "linux"].includes(target) ||
      !["aarch64", "x86_64"].includes(arch) ||
      !versionPattern.test(currentVersion)
    )
      throw new DistributionError("invalid_request", 400);
    const release = await latestRelease(c.env, target, arch);
    if (!release || compareVersions(release.version, currentVersion) <= 0)
      return new Response(null, {
        status: 204,
        headers: { "Cache-Control": "no-store" },
      });
    const row = await c.env.DB.prepare(
      "SELECT id,signature FROM distribution_asset WHERE version=? AND platform=? AND arch=? AND kind='updater'",
    )
      .bind(release.version, target, arch)
      .first<{ id: string; signature: string }>();
    if (!row?.signature)
      throw new DistributionError("release_unavailable", 503);
    return authJsonResponse(
      {
        version: release.version,
        notes: release.notes,
        pub_date: release.pubDate,
        url: `${c.get("config").authOrigin}/api/desktop/distribution/assets/${row.id}`,
        signature: row.signature,
      },
      200,
    );
  },
);
distributionRoutes.on(
  ["GET", "HEAD"],
  "/api/desktop/distribution/assets/:assetId",
  async (c) => {
    await desktop(c);
    const asset = await getAsset(c.env, c.req.param("assetId"));
    if (!asset || asset.kind !== "updater")
      throw new DistributionError("not_found", 404);
    return streamAsset(c.env, c.req.raw, asset);
  },
);
export function distributionApiMethods(path: string): readonly string[] | null {
  if (path === "/api/distribution/application") return ["GET", "POST"];
  if (
    path === "/api/distribution/download-links" ||
    path === "/api/distribution/publisher/releases"
  )
    return ["POST"];
  if (/^\/api\/distribution\/download-links\/[A-Za-z0-9_-]{43}$/.test(path))
    return ["GET"];
  if (
    /^\/api\/distribution\/download-links\/[A-Za-z0-9_-]{43}\/assets\/[a-f0-9-]{36}$/.test(
      path,
    )
  )
    return ["GET", "HEAD"];
  if (/^\/api\/distribution\/publisher\/releases\/[0-9A-Za-z.-]+$/.test(path))
    return ["GET"];
  if (
    /^\/api\/desktop\/distribution\/update\/(darwin|windows|linux)\/(aarch64|x86_64)\/[0-9A-Za-z.-]+$/.test(
      path,
    )
  )
    return ["GET"];
  if (/^\/api\/desktop\/distribution\/assets\/[a-f0-9-]{36}$/.test(path))
    return ["GET", "HEAD"];
  return null;
}
