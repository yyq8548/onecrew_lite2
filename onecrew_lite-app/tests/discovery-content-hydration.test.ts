import assert from "node:assert/strict";
import { copyFile, mkdtemp } from "node:fs/promises";
import { join } from "node:path";
import { DatabaseSync } from "node:sqlite";
import { test } from "node:test";
import { fileURLToPath } from "node:url";
import { DiscoveryContentRepository } from "../src/discovery/content-repository.ts";
import { getDiscoverySavedContent } from "../src/discovery/saved-read-repository.ts";

async function copied(platform: string) {
  const root = await mkdtemp(fileURLToPath(new URL("../../.scratch/content-hydration-", import.meta.url))), path = join(root, "crm.sqlite");
  await copyFile(new URL(`./fixtures/discovery-content-hydration/${platform}-before.sqlite`, import.meta.url), path);
  const db = new DatabaseSync(path); db.exec("PRAGMA foreign_keys=ON"); return { db, path };
}
for (const platform of ["bilibili", "twitter"]) test(`原Rust ${platform}内容真实SQLite的完整claim/receipt和永久原生绑定一致`, async () => {
  const { db, path } = await copied(platform); using other = new DatabaseSync(path); using original = new DatabaseSync(fileURLToPath(new URL(`./fixtures/discovery-content-hydration/${platform}-after.sqlite`, import.meta.url)), { readOnly: true });
  try {
    const campaign = platform === "twitter" ? "x-content" : "content", hit = String(original.prepare("SELECT search_hit_id FROM discovery_content_reads").get()!.search_hit_id), repo = new DiscoveryContentRepository(db);
    assert.equal(repo.nextNative(campaign, platform), hit);
    const expected = getDiscoverySavedContent(original, campaign, hit), claim = repo.claim(campaign, 1, hit); assert.deepEqual(claim, expected.claim);
    assert.throws(() => new DiscoveryContentRepository(other).claim(campaign, 1, hit), /read_active/);
    const actual = repo.finish(claim, expected.native_content, null); assert.deepEqual({ ...actual, captured_at: 0 }, { ...expected, captured_at: 0 });
    assert.equal(repo.nextNative(campaign, platform), null); assert.throws(() => repo.finish(claim, expected.native_content, null), /claim_stale/);
    if (platform === "twitter") assert.deepEqual(db.prepare("SELECT search_hit_id,identity_search_hit_id,profile_search_hit_id,account_id FROM discovery_content_identity_bindings").all(), original.prepare("SELECT search_hit_id,identity_search_hit_id,profile_search_hit_id,account_id FROM discovery_content_identity_bindings").all());
  } finally { db.close(); }
});
test("内容事务拒写共同回滚run/receipt；中断新领取增加计费且旧回执不能覆盖", async () => {
  const { db } = await copied("twitter"); using original = new DatabaseSync(fileURLToPath(new URL("./fixtures/discovery-content-hydration/twitter-after.sqlite", import.meta.url)), { readOnly: true });
  try {
    const repo = new DiscoveryContentRepository(db), hit = repo.nextNative("x-content", "twitter")!, expected = getDiscoverySavedContent(original, "x-content", hit), first = repo.claim("x-content", 1, hit);
    db.exec("CREATE TRIGGER reject_content_receipt BEFORE UPDATE OF native_content_json ON discovery_content_reads BEGIN SELECT RAISE(ABORT,'receipt_rejected'); END");
    assert.throws(() => repo.finish(first, expected.native_content, null), /receipt_rejected/);
    assert.equal(db.prepare("SELECT status FROM discovery_platform_runs").get()!.status, "searching"); assert.equal(db.prepare("SELECT status FROM discovery_content_reads").get()!.status, "reading");
    db.exec("DROP TRIGGER reject_content_receipt"); repo.interrupt(first); assert.equal(repo.nextNative("x-content", "twitter"), hit);
    const second = repo.claim("x-content", 1, hit); assert.equal(second.call_number, first.call_number + 1); assert.throws(() => repo.finish(first, expected.native_content, null), /claim_stale/);
    repo.finish(second, null, "opencli_timeout"); assert.equal(db.prepare("SELECT status FROM discovery_platform_runs").get()!.status, "completed"); assert.equal(db.prepare("SELECT stop_code FROM discovery_content_reads").get()!.stop_code, "opencli_timeout");
  } finally { db.close(); }
});
