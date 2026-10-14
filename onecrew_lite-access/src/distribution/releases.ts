import type { Bindings } from "../types/bindings";
import { DistributionError, releaseInput } from "./contracts";
import { hash } from "./crypto";
export type Asset = {
  id: string;
  key: string;
  filename: string;
  platform: string;
  arch: string;
  kind: string;
  size: number;
  sha256: string;
  signature: string | null;
  etag: string;
  version: string;
};
export async function readRelease(env: Bindings, version: string) {
  const r = await env.DB.prepare(
    "SELECT version,notes,pub_date AS pubDate FROM distribution_release WHERE version=?",
  )
    .bind(version)
    .first<{ version: string; notes: string; pubDate: string }>();
  if (!r) throw new DistributionError("not_found", 404);
  const assets = await env.DB.prepare(
    "SELECT id,object_key AS key,filename,platform,arch,kind,size,sha256,signature FROM distribution_asset WHERE version=? ORDER BY id",
  )
    .bind(version)
    .all();
  return {
    ...r,
    assets: assets.results.map((a) => {
      if (a.signature === null) {
        const { signature: _, ...rest } = a;
        return rest;
      }
      return a;
    }),
  };
}
export async function publishRelease(
  env: Bindings,
  input: unknown,
  requestId: string,
) {
  const data = releaseInput.parse(input);
  if (!env.RELEASES) throw new DistributionError("configuration_required", 503);
  data.assets.sort((a, b) => a.id.localeCompare(b.id));
  const fingerprint = await hash(JSON.stringify(data));
  const prior = await env.DB.prepare(
    "SELECT version,fingerprint FROM distribution_release WHERE request_id=?",
  )
    .bind(requestId)
    .first<{ version: string; fingerprint: string }>();
  if (prior) {
    if (prior.fingerprint !== fingerprint)
      throw new DistributionError("idempotency_conflict", 409);
    return { ok: true, release: await readRelease(env, prior.version) };
  }
  if (
    await env.DB.prepare("SELECT 1 FROM distribution_release WHERE version=?")
      .bind(data.version)
      .first()
  )
    throw new DistributionError("version_exists", 409);
  const objects = await Promise.all(
    data.assets.map((a) => env.RELEASES!.head(a.key)),
  );
  for (let i = 0; i < data.assets.length; i++) {
    const a = data.assets[i]!,
      obj = objects[i];
    if (!obj || obj.size !== a.size || obj.customMetadata?.sha256 !== a.sha256)
      throw new DistributionError("asset_not_ready", 422);
  }
  try {
    await env.DB.batch([
      env.DB.prepare(
        "INSERT INTO distribution_release VALUES(?,?,?,?,?,?)",
      ).bind(
        data.version,
        data.notes,
        data.pubDate,
        Date.now(),
        requestId,
        fingerprint,
      ),
      ...data.assets.map((a, i) =>
        env.DB.prepare(
          "INSERT INTO distribution_asset VALUES(?,?,?,?,?,?,?,?,?,?,?)",
        ).bind(
          a.id,
          data.version,
          a.key,
          a.filename,
          a.platform,
          a.arch,
          a.kind,
          a.size,
          a.sha256,
          a.signature ?? null,
          objects[i]!.etag,
        ),
      ),
    ]);
  } catch {
    const concurrent = await env.DB.prepare(
      "SELECT version,fingerprint FROM distribution_release WHERE request_id=?",
    )
      .bind(requestId)
      .first<{ version: string; fingerprint: string }>();
    if (concurrent?.fingerprint === fingerprint)
      return { ok: true, release: await readRelease(env, concurrent.version) };
    throw new DistributionError("version_exists", 409);
  }
  return { ok: true, release: await readRelease(env, data.version) };
}
export function compareVersions(a: string, b: string) {
  // A hyphen inside a prerelease identifier is data, not another separator.
  const separate = (version: string): [string, string | undefined] => {
    const separator = version.indexOf("-");
    return separator < 0
      ? [version, undefined]
      : [version.slice(0, separator), version.slice(separator + 1)];
  };
  const [ac, ap] = separate(a),
    [bc, bp] = separate(b);
  const an = ac!.split(".").map(BigInt),
    bn = bc!.split(".").map(BigInt);
  for (let i = 0; i < 3; i++) {
    if (an[i] !== bn[i]) return an[i]! > bn[i]! ? 1 : -1;
  }
  if (ap === undefined || bp === undefined)
    return ap === bp ? 0 : ap === undefined ? 1 : -1;
  const aa = ap.split("."),
    ba = bp.split(".");
  for (let i = 0; i < Math.max(aa.length, ba.length); i++) {
    const x = aa[i],
      y = ba[i];
    if (x === y) continue;
    if (x === undefined) return -1;
    if (y === undefined) return 1;
    const xn = /^\d+$/.test(x),
      yn = /^\d+$/.test(y);
    if (xn && yn) {
      const xx = BigInt(x),
        yy = BigInt(y);
      if (xx !== yy) return xx > yy ? 1 : -1;
      continue;
    }
    if (xn !== yn) return xn ? -1 : 1;
    return x > y ? 1 : -1;
  }
  return 0;
}
export async function latestRelease(
  env: Pick<Bindings, "DB">,
  platform?: string,
  arch?: string,
) {
  const rows = await env.DB.prepare(
    "SELECT DISTINCT r.version,r.notes,r.pub_date AS pubDate FROM distribution_release r JOIN distribution_asset a ON a.version=r.version WHERE (? IS NULL OR a.platform=?) AND (? IS NULL OR a.arch=?) AND (? IS NULL OR a.kind='updater')",
  )
    .bind(
      platform ?? null,
      platform ?? null,
      arch ?? null,
      arch ?? null,
      platform ?? null,
    )
    .all<{ version: string; notes: string; pubDate: string }>();
  return (
    rows.results.sort((a, b) => compareVersions(b.version, a.version))[0] ??
    null
  );
}
export async function getAsset(env: Bindings, id: string) {
  return env.DB.prepare(
    "SELECT id,version,object_key AS key,filename,platform,arch,kind,size,sha256,signature,etag FROM distribution_asset WHERE id=?",
  )
    .bind(id)
    .first<Asset>();
}
export function parseRange(range: string | null, size: number) {
  if (range === null) return null;
  const match = /^bytes=(\d*)-(\d*)$/.exec(range);
  if (!match || (!match[1] && !match[2]))
    throw new DistributionError("invalid_range", 416);
  let start: number, end: number;
  if (!match[1]) {
    const suffix = Number(match[2]);
    if (!Number.isSafeInteger(suffix) || suffix <= 0)
      throw new DistributionError("invalid_range", 416);
    start = Math.max(0, size - suffix);
    end = size - 1;
  } else {
    start = Number(match[1]);
    end = match[2] ? Math.min(Number(match[2]), size - 1) : size - 1;
  }
  if (
    !Number.isSafeInteger(start) ||
    !Number.isSafeInteger(end) ||
    start < 0 ||
    start >= size ||
    end < start
  )
    throw new DistributionError("invalid_range", 416);
  return { offset: start, length: end - start + 1 };
}
export async function streamAsset(
  env: Bindings,
  request: Request,
  asset: Asset,
) {
  if (!env.RELEASES) throw new DistributionError("configuration_required", 503);
  let range: ReturnType<typeof parseRange>;
  try {
    range = parseRange(request.headers.get("Range"), asset.size);
  } catch {
    return new Response(null, {
      status: 416,
      headers: {
        "Content-Range": `bytes */${asset.size}`,
        "Cache-Control": "no-store",
        "Referrer-Policy": "no-referrer",
      },
    });
  }
  const head = await env.RELEASES.head(asset.key);
  if (!head || head.etag !== asset.etag || head.size !== asset.size)
    throw new DistributionError("not_found", 404);
  const headers = new Headers({
    "Cache-Control": "no-store",
    "Referrer-Policy": "no-referrer",
    "Content-Type": "application/octet-stream",
    "X-Content-Type-Options": "nosniff",
    "Cross-Origin-Resource-Policy": "same-origin",
    "Content-Disposition": `attachment; filename="${asset.filename}"`,
    "Accept-Ranges": "bytes",
    "Content-Length": String(range?.length ?? asset.size),
    ETag: head.httpEtag,
  });
  if (range)
    headers.set(
      "Content-Range",
      `bytes ${range.offset}-${range.offset + range.length - 1}/${asset.size}`,
    );
  if (request.method === "HEAD")
    return new Response(null, { status: range ? 206 : 200, headers });
  const body = await env.RELEASES.get(asset.key, {
    onlyIf: { etagMatches: asset.etag },
    ...(range ? { range } : {}),
  });
  if (!body || !("body" in body)) throw new DistributionError("not_found", 404);
  return new Response(body.body, { status: range ? 206 : 200, headers });
}
