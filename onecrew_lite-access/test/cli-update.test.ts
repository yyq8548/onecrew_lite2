import { env } from "cloudflare:workers";
import { applyD1Migrations, type D1Migration } from "cloudflare:test";
import { beforeEach, describe, expect, it, vi } from "vitest";
import app from "../src/index";
import { targets } from "../src/cli-distribution/contracts";

const base = env as unknown as CloudflareBindings & { TEST_MIGRATIONS: D1Migration[] };
const version = "0.5.22", userId = "cli-update-user";
const credentialId = "11111111-1111-4111-8111-111111111111";
const deviceId = `dev_${"u".repeat(24)}`, pairingId = `pair_${"u".repeat(24)}`;
const bearer = `${credentialId}_${"a".repeat(43)}`;
const projection = {
  protocol: "reacher.creator_sync", protocol_version: 1,
  installation_id: "22222222-2222-4222-8222-222222222222", credential_id: credentialId,
  access_user_id: userId, access_device_id: deviceId, access_pairing_id: pairingId,
  source_instance_id: "33333333-3333-4333-8333-333333333333",
  capabilities: ["creator_snapshots:write_chunked_quarantine"],
  accepted_bundle: {format: "reacher.creator_bundle", format_version: 2, profile: "portable_public",
    max_snapshot_bytes: 100, max_chunk_bytes: 100, max_chunks: 1,
    max_namespace_staged_bytes: 100, max_access_user_staged_bytes: 100},
};
const manifest = {schema_version: 1, product: "onecrew-cli", version,
  assets: targets.map(target => ({target, key: `releases/${version}/onecrew-${version}-${target}${target.endsWith("windows-msvc") ? ".exe" : ""}`, size: 3, sha256: "a".repeat(64)}))};
const verify = vi.fn(async (_request: Request) => Response.json(projection));
const get = vi.fn(async (key: string) => key.endsWith("cli-manifest.json")
  ? {size: 1000, json: async () => structuredClone(manifest)}
  : {size: 3, customMetadata: {sha256: "a".repeat(64)}, body: new Blob([new Uint8Array([1,2,3])]).stream()});
const head = vi.fn(async () => ({size: 3, customMetadata: {sha256: "a".repeat(64)}}));
const bindings = {...base, ACCESS_ORIGIN: "https://access.example", BETTER_AUTH_URL: "https://access.example", WEB_ORIGIN: "https://access.example",
  CLI_RELEASE_VERSION: version, RELEASES: {get, head} as unknown as R2Bucket,
  CREATOR_SYNC_API_ORIGIN: "https://sync.example", SYNC_CREDENTIAL_VERIFIER: {fetch: verify} as unknown as Fetcher};
const request = (path = "/api/cli/update", init: RequestInit = {}) => app.request(`https://access.example${path}`, {headers: {Authorization: `Bearer ${bearer}`}, ...init}, bindings);
const binary = `/api/cli/update/${version}/files/${targets[0]}`;
beforeEach(async () => {
  vi.clearAllMocks();
  await applyD1Migrations(base.DB, base.TEST_MIGRATIONS);
  await base.DB.prepare('DELETE FROM "user" WHERE id=?').bind(userId).run();
  await base.DB.prepare('INSERT INTO "user"(id,name,email,emailVerified,createdAt,updatedAt) VALUES(?,?,?,1,?,?)').bind(userId,"Update","update@example.com","2026-01-01","2026-01-01").run();
  await base.DB.prepare("INSERT INTO distribution_entitlement VALUES(?,'active',1,0)").bind(userId).run();
  await base.DB.prepare("INSERT INTO access_device(id,user_id,label,created_at) VALUES (?, ?, 'Update device', 1000)").bind(deviceId,userId).run();
  await base.DB.prepare(`INSERT INTO desktop_pairing_grant (id,device_id,request_hmac,code_hmac,code_challenge,platform,app_version,created_at,expires_at,consumed_at)
    VALUES (?, ?, ?, ?, ?, 'macos', '0.5.21', 1000, 91000, 2000)`).bind(pairingId,deviceId,"u".repeat(43),"v".repeat(43),"w".repeat(43)).run();
});

describe("Authenticated CLI updates", () => {
  it("requires an installation bearer on metadata and binaries even with browser cookies", async () => {
    for (const path of ["/api/cli/update", binary]) {
      const response = await request(path, {headers: {Cookie: "session=synthetic"}});
      expect(response.status).toBe(401);
      expect(response.headers.get("Cache-Control")).toBe("no-store");
    }
    expect(verify).not.toHaveBeenCalled(); expect(get).not.toHaveBeenCalled();
  });
  it("returns only current release assets and verifies every authenticated download", async () => {
    const response = await request();
    expect(response.status).toBe(200);
    expect(response.headers.get("Cache-Control")).toBe("no-store");
    expect(response.headers.get("Vary")).toContain("Authorization");
    const value = await response.json();
    expect(value).toEqual({schema_version:1, product:"onecrew-cli", version,
      assets: manifest.assets.map(({target,size,sha256})=>({target,size,sha256,url:`https://access.example/api/cli/update/${version}/files/${target}`}))});
    expect(JSON.stringify(value)).not.toContain(bearer);
    expect(JSON.stringify(value)).not.toContain('"key"');
    const downloaded = await request(binary);
    expect(downloaded.status).toBe(200);
    expect(downloaded.headers.get("Cache-Control")).toBe("no-store");
    expect(downloaded.headers.get("Vary")).toContain("Authorization");
    expect(new Uint8Array(await downloaded.arrayBuffer())).toEqual(new Uint8Array([1,2,3]));
    const metadata = await request(binary,{method:"HEAD"});
    expect(metadata.status).toBe(200); expect(await metadata.text()).toBe("");
    expect(metadata.headers.get("Content-Length")).toBe("3");
    expect(verify).toHaveBeenCalledTimes(3);
    for (const [probe] of verify.mock.calls) {
      expect(probe.url).toBe("https://sync.example/creator-sync/credential");
      expect(probe.headers.get("Authorization")).toBe(`Bearer ${bearer}`);
      expect(probe.headers.has("Cookie")).toBe(false);
      expect(probe.redirect).toBe("manual");
    }
  });
  it("fails closed for expired credentials and unavailable identity verification", async () => {
    verify.mockResolvedValueOnce(new Response(null,{status:401}));
    expect((await request()).status).toBe(401);
    verify.mockResolvedValueOnce(new Response(null,{status:503}));
    expect((await request(binary)).status).toBe(503);
    expect(get).not.toHaveBeenCalled();
  });
  it("rechecks entitlement and email verification on binary requests", async () => {
    expect((await request()).status).toBe(200);
    await base.DB.prepare("UPDATE distribution_entitlement SET status='revoked' WHERE user_id=?").bind(userId).run();
    expect((await request(binary)).status).toBe(403);
    await base.DB.prepare("UPDATE distribution_entitlement SET status='active' WHERE user_id=?").bind(userId).run();
    await base.DB.prepare('UPDATE "user" SET emailVerified=0 WHERE id=?').bind(userId).run();
    expect((await request()).status).toBe(403);
  });
  it("rejects mismatched device pairing and stale release paths", async () => {
    verify.mockResolvedValueOnce(Response.json({...projection,access_pairing_id:`pair_${"x".repeat(24)}`}));
    expect((await request()).status).toBe(403);
    expect((await request(binary.replace(version,"0.5.21"))).status).toBe(409);
    expect((await request("/api/cli/update",{method:"POST"})).status).toBe(405);
    expect((await request(binary.replace(targets[0],"linux"))).status).toBe(404);
  });
  it("rejects changed binary metadata before streaming", async () => {
    get.mockImplementationOnce(async () => ({size:1000,json:async()=>structuredClone(manifest)}));
    get.mockImplementationOnce(async () => ({size:4,customMetadata:{sha256:"a".repeat(64)},body:new Blob([]).stream()}));
    expect((await request(binary)).status).toBe(409);
  });
});
