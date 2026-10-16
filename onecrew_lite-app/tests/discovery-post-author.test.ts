import assert from "node:assert/strict";
import { copyFile, mkdtemp } from "node:fs/promises";
import { join } from "node:path";
import { DatabaseSync } from "node:sqlite";
import { fileURLToPath } from "node:url";
import { test } from "node:test";
import { DiscoveryPostAuthorRepository } from "../src/discovery/post-author-repository.ts";
import { getDiscoverySavedIdentity } from "../src/discovery/saved-read-repository.ts";
import { discoveryPostIdForSource } from "../src/discovery/post-author-contracts.ts";

async function before() {
  const root = await mkdtemp(fileURLToPath(new URL("../../.scratch/post-author-", import.meta.url))), path = join(root, "crm.sqlite");
  await copyFile(new URL("./fixtures/discovery-post-author/before.sqlite", import.meta.url), path); const db = new DatabaseSync(path); db.exec("PRAGMA foreign_keys=ON"); return { db, path };
}
test("原Rust X原帖作者claim/receipt逐字段相同，竞争不重复计费，中断不自动重试", async () => {
  const { db, path } = await before(); using other = new DatabaseSync(path); using original = new DatabaseSync(fileURLToPath(new URL("./fixtures/discovery-post-author/after.sqlite", import.meta.url)), { readOnly: true });
  try {
    const repo = new DiscoveryPostAuthorRepository(db), hit = String(original.prepare("SELECT search_hit_id FROM discovery_identity_reads").get()!.search_hit_id), expected = getDiscoverySavedIdentity(original, "identity", hit);
    assert.equal(repo.needs("identity", hit), true); const claim = repo.claim("identity", 1, hit); assert.deepEqual(claim, expected.claim);
    assert.throws(() => new DiscoveryPostAuthorRepository(other).claim("identity", 1, hit), /approved_idle_or_budget/);
    const actual = repo.finish(claim, { account_id: expected.account_id, observed_source_url: expected.observed_source_url, stop_code: null }); assert.deepEqual({ ...actual, captured_at: 0 }, { ...expected, captured_at: 0 });
    assert.equal(repo.needs("identity", hit), false); assert.throws(() => repo.claim("identity", 1, hit), /source_unavailable_or_attempted/); assert.throws(() => repo.finish(claim, { account_id: expected.account_id, observed_source_url: expected.observed_source_url, stop_code: null }), /claim_stale/);
  } finally { db.close(); }
  const second = await before();
  try { const repo = new DiscoveryPostAuthorRepository(second.db), hit = String(second.db.prepare("SELECT id FROM discovery_search_hits").get()!.id), claim = repo.claim("identity", 1, hit); repo.interrupt(claim); assert.equal(repo.needs("identity", hit), false); assert.throws(() => repo.claim("identity", 1, hit), /source_unavailable_or_attempted/); assert.equal(getDiscoverySavedIdentity(second.db, "identity", hit).stop_code, "interrupted_identity_read"); }
  finally { second.db.close(); }
});
test("X原帖定位符仅原x.com精确作者或i路径，无查询、重定向路径与端口", () => {
  assert.equal(discoveryPostIdForSource("creator", "https://x.com/creator/status/123"), "123"); assert.equal(discoveryPostIdForSource("creator", "https://x.com/i/status/123"), "123");
  for (const source of ["https://twitter.com/creator/status/123", "https://x.com/other/status/123", "https://x.com/creator/status/123/", "https://x.com:443/creator/status/123", "https://x.com/creator/status/123?x=1"]) assert.throws(() => discoveryPostIdForSource("creator", source));
});
