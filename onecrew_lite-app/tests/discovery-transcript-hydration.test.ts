import assert from "node:assert/strict";
import { copyFile, mkdtemp } from "node:fs/promises";
import { join } from "node:path";
import { DatabaseSync } from "node:sqlite";
import { fileURLToPath } from "node:url";
import { test } from "node:test";
import { DiscoveryTranscriptRepository, validateDiscoveryTranscriptResult } from "../src/discovery/transcript-repository.ts";
import { getDiscoverySavedTranscript } from "../src/discovery/saved-read-repository.ts";
import { boundedDiscoveryTranscript } from "../src/discovery/transcript-execution.ts";

async function before() {
  const root = await mkdtemp(fileURLToPath(new URL("../../.scratch/transcript-hydration-", import.meta.url))), path = join(root, "crm.sqlite");
  await copyFile(new URL("./fixtures/discovery-transcript-hydration/before.sqlite", import.meta.url), path);
  const database = new DatabaseSync(path); database.exec("PRAGMA foreign_keys=ON"); return { database, path };
}
test("原Rust字幕真实三阶段SQLite：计费领取、竞争连接和完成回执逐字段相同", async () => {
  const { database, path } = await before(); using original = new DatabaseSync(fileURLToPath(new URL("./fixtures/discovery-transcript-hydration/after.sqlite", import.meta.url)), { readOnly: true }); using other = new DatabaseSync(path);
  try {
    const repo = new DiscoveryTranscriptRepository(database), expected = original.prepare("SELECT search_hit_id FROM discovery_transcript_reads").get()!, hit = String(expected.search_hit_id);
    assert.deepEqual(repo.next("transcript"), { id: hit, profile_ref: "dedicated" });
    const claim = repo.claim("transcript", 1, "dedicated", hit); assert.equal(claim.call_number, 3);
    assert.throws(() => new DiscoveryTranscriptRepository(other).claim("transcript", 1, "dedicated", hit), /read_active/);
    const saved = getDiscoverySavedTranscript(original, "transcript", hit), actual = repo.finish(claim, { segments: saved.segments, truncated: saved.truncated, stop_code: saved.stop_code });
    assert.deepEqual({ ...actual, captured_at: 0 }, { ...saved, captured_at: 0 }); assert.equal(repo.next("transcript"), null);
    assert.throws(() => repo.finish(claim, { segments: [], truncated: false, stop_code: null }), /claim_stale/);
    assert.equal(database.prepare("SELECT used_calls FROM discovery_campaigns").get()!.used_calls, 3);
  } finally { database.close(); }
});
test("字幕中断保留原计费并允许明确重新领取，空字幕是unavailable，错误回执不能伪装完成", async () => {
  const { database } = await before();
  try {
    const repo = new DiscoveryTranscriptRepository(database), hit = repo.next("transcript")!.id, first = repo.claim("transcript", 1, "dedicated", hit);
    repo.interrupt(first); assert.equal(repo.next("transcript")!.id, hit); const second = repo.claim("transcript", 1, "dedicated", hit); assert.equal(second.call_number, 4);
    assert.throws(() => repo.finish(first, { segments: [], truncated: false, stop_code: null }), /claim_stale/);
    assert.equal(repo.finish(second, { segments: [], truncated: false, stop_code: null }).status, "unavailable");
    assert.throws(() => validateDiscoveryTranscriptResult({ segments: [{ timestamp: "0:10", speaker: "", text: "original" }], truncated: false, stop_code: "opencli_timeout" }));
    assert.throws(() => validateDiscoveryTranscriptResult({ segments: [], truncated: true, stop_code: null }));
    const excerpt = boundedDiscoveryTranscript(Array.from({ length: 201 }, () => ({ timestamp: "0:10", speaker: "", text: "original" })));
    assert.equal(excerpt.segments.length, 200); assert.equal(excerpt.truncated, true);
  } finally { database.close(); }
});
