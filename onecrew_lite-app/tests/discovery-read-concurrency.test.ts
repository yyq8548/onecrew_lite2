import assert from "node:assert/strict";
import { test } from "node:test";
import { DatabaseSync } from "node:sqlite";
import { mkdtemp, rm } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import { join } from "node:path";
import { backupLegacyDatabase } from "../src/storage/legacy-sqlite.ts";
import { upgradeCreatorStagingCopy } from "../src/storage/creator-upgrade.ts";
import { DiscoveryRepository } from "../src/discovery/repository.ts";
import { DiscoveryReadGate } from "../src/discovery/read-capacity.ts";

const source = process.env.ONECREW_QUERY_FEEDBACK_DATABASE;
const campaign = "b29b55fa-1131-4d00-9d92-ecad85d3f8f7";
test("已暂停任务的离线副本支持两平台领取，第三路和同平台不计费，恢复保留两个中断", { skip: !source }, async () => {
  const root = await mkdtemp(fileURLToPath(new URL("../../.scratch/read-concurrency-", import.meta.url)));
  const stagingPath = join(root, "staging", "creator.sqlite");
  await backupLegacyDatabase(source!, stagingPath, "creator");
  await upgradeCreatorStagingCopy({ sourcePath: source!, stagingPath });
  const db = new DatabaseSync(stagingPath), second = new DatabaseSync(stagingPath);
  const first = new DiscoveryRepository(db), other = new DiscoveryRepository(second);
  try {
    const plan = first.getPlan(campaign), before = plan.used_calls;
    const preserved = db.prepare("SELECT id,verdict,evidence_json FROM discovery_candidates WHERE discovery_campaign_id=? ORDER BY id").all(campaign);
    const scopes = plan.scopes.filter(scope => ["tiktok", "instagram", "youtube"].includes(scope.platform));
    assert.equal(scopes.length, 3);
    const [one, two, three] = ["tiktok", "instagram", "youtube"].map(platform => scopes.find(scope => scope.platform === platform)!);
    first.claimSearch(campaign, plan.revision, one!.platform, one!.profile_ref);
    assert.throws(() => other.claimSearch(campaign, plan.revision, one!.platform, one!.profile_ref), /read_active/);
    other.claimSearch(campaign, plan.revision, two!.platform, two!.profile_ref);
    assert.throws(() => first.claimSearch(campaign, plan.revision, three!.platform, three!.profile_ref), /read_active/);
    assert.equal(first.getPlan(campaign).used_calls, before + 2);
    assert.equal(first.planStep(campaign, plan.revision).kind, "busy");
    first.recoverInterruptedReads();
    const stopped = first.getPlan(campaign).platform_runs.filter(run => [one!.platform, two!.platform].includes(run.platform));
    assert.ok(stopped.every(run => run.status === "technical_stop" && run.stop_code === "interrupted_read"));
    assert.deepEqual(db.prepare("SELECT id,verdict,evidence_json FROM discovery_candidates WHERE discovery_campaign_id=? ORDER BY id").all(campaign), preserved);
    const gate = new DiscoveryReadGate(), permit = gate.acquire([one!.platform]);
    assert.throws(() => gate.acquire([one!.platform]), /read_active/);
    const another = gate.acquire([two!.platform]);
    assert.throws(() => gate.acquire([three!.platform]), /read_active/);
    permit.release(); permit.release();
    gate.acquire([three!.platform]).release(); another.release();
    assert.deepEqual(gate.occupied, []);
  } finally { second.close(); db.close(); await rm(root, { recursive: true, force: true }); }
});
