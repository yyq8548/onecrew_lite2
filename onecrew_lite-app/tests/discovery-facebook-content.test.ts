import assert from "node:assert/strict";
import { copyFile, mkdtemp } from "node:fs/promises";
import { join } from "node:path";
import { DatabaseSync } from "node:sqlite";
import { fileURLToPath } from "node:url";
import { test } from "node:test";
import { DiscoveryContentRepository } from "../src/discovery/content-repository.ts";
import { getDiscoverySavedContent } from "../src/discovery/saved-read-repository.ts";

async function before() {
  const root = await mkdtemp(fileURLToPath(new URL("../../.scratch/facebook-content-", import.meta.url))), path = join(root, "crm.sqlite");
  await copyFile(new URL("./fixtures/discovery-facebook-content/before.sqlite", import.meta.url), path); const db = new DatabaseSync(path); db.exec("PRAGMA foreign_keys=ON"); return db;
}
const campaign = "facebook-native-identity";
test("原Rust Facebook原文逐来源永久attempt：失败推进第二条、完成和重放保持同一回执", async () => {
  using db = await before(); using original = new DatabaseSync(fileURLToPath(new URL("./fixtures/discovery-facebook-content/after.sqlite", import.meta.url)), { readOnly: true });
  const repo = new DiscoveryContentRepository(db), row = original.prepare("SELECT search_hit_id FROM discovery_content_reads").get()!, hit = String(row.search_hit_id);
  assert.equal(repo.nextNative(campaign, "facebook"), hit);
  const first = repo.claim(campaign, 1, hit); assert.ok(first.source_url.endsWith("1075265928641482")); assert.equal(first.call_number, 5);
  const failed = repo.finish(first, null, "opencli_command_failed"); assert.equal(failed.status, "failed"); assert.equal(repo.nextNative(campaign, "facebook"), hit);
  const second = repo.claim(campaign, 1, hit), expected = getDiscoverySavedContent(original, campaign, hit); assert.deepEqual(second, expected.claim);
  assert.deepEqual(repo.finish(first, null, "opencli_command_failed"), failed); assert.throws(() => repo.finish(first, null, "opencli_timeout"), /outcome_changed/);
  const completed = repo.finish(second, expected.native_content, null); assert.deepEqual({ ...completed, captured_at: 0 }, { ...expected, captured_at: 0 }); assert.deepEqual(repo.finish(second, expected.native_content, null), completed);
  assert.deepEqual(db.prepare("SELECT search_hit_id,source_url,call_number,status,stop_code FROM discovery_facebook_original_attempts ORDER BY call_number").all(), original.prepare("SELECT search_hit_id,source_url,call_number,status,stop_code FROM discovery_facebook_original_attempts ORDER BY call_number").all());
  assert.throws(() => repo.claim(campaign, 1, hit), /targets_exhausted|already_attempted/);
});
test("Facebook原文取消保留永久失败来源，下一次仅领取尚未读取的第二来源", async () => {
  using db = await before(); const repo = new DiscoveryContentRepository(db), hit = repo.nextNative(campaign, "facebook")!, first = repo.claim(campaign, 1, hit); repo.interrupt(first);
  assert.equal(db.prepare("SELECT status FROM discovery_platform_runs WHERE platform='facebook'").get()!.status, "completed");
  assert.equal(db.prepare("SELECT stop_code FROM discovery_facebook_original_attempts").get()!.stop_code, "interrupted_content_read");
  const second = repo.claim(campaign, 1, hit); assert.notEqual(second.source_url, first.source_url); assert.equal(second.call_number, first.call_number + 1);
});
