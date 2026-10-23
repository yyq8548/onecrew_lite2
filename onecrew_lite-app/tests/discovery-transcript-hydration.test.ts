import assert from "node:assert/strict";
import { copyFile, mkdtemp, rm } from "node:fs/promises";
import { join } from "node:path";
import { DatabaseSync } from "node:sqlite";
import { fileURLToPath } from "node:url";
import { test } from "node:test";
import { DiscoveryRepository } from "../src/discovery/repository.ts";
import { getDiscoverySavedTranscript } from "../src/discovery/saved-read-repository.ts";

async function before() {
  const root = await mkdtemp(fileURLToPath(new URL("../../.scratch/transcript-hydration-", import.meta.url))), path = join(root, "crm.sqlite");
  await copyFile(new URL("./fixtures/discovery-transcript-hydration/before.sqlite", import.meta.url), path);
  const database = new DatabaseSync(path); database.exec("PRAGMA foreign_keys=ON"); return { database, path, root };
}
test("真实SQLite候选不再调度字幕，历史资料保持不变", async () => {
  const { database, path, root } = await before(); using original = new DatabaseSync(fileURLToPath(new URL("./fixtures/discovery-transcript-hydration/after.sqlite", import.meta.url)), { readOnly: true }); using other = new DatabaseSync(path);
  try {
    const repo = new DiscoveryRepository(database), expected = original.prepare("SELECT search_hit_id FROM discovery_transcript_reads").get()!, hit = String(expected.search_hit_id);
    const before = database.prepare("SELECT * FROM discovery_campaigns").all();
    assert.equal("nextTranscriptHit" in repo, false);
    assert.equal("claimTranscript" in repo, false);
    assert.equal("claimTranscript" in new DiscoveryRepository(other), false);
    assert.deepEqual(database.prepare("SELECT * FROM discovery_campaigns").all(), before);
    assert.equal(database.prepare("SELECT count(*) count FROM discovery_transcript_reads").get()!.count, 0);
    const saved = getDiscoverySavedTranscript(original, "transcript", hit);
    assert.equal(saved.status, "completed");
    assert.ok(saved.segments.length > 0);
  } finally { other.close(); database.close(); await rm(root, { recursive: true }); }
});
