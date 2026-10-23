import assert from "node:assert/strict";
import { mkdtemp } from "node:fs/promises";
import { join } from "node:path";
import { DatabaseSync } from "node:sqlite";
import { test } from "node:test";
import { fileURLToPath } from "node:url";
import { initializeNewCreatorDatabase } from "../src/storage/creator-schema.ts";
import { createPrivateDirectory } from "../src/storage/private-backup.ts";
import { DiscoveryRepository } from "../src/discovery/repository.ts";

test("真实数据库恢复核对停止时间与代码，保留计费且禁止越过授权和风控停止", async () => {
  const directory = await mkdtemp(fileURLToPath(new URL("../../.scratch/discovery-repair-", import.meta.url)));
  const path = join(await createPrivateDirectory(join(directory, "private")), "creator.sqlite");
  await initializeNewCreatorDatabase(path);
  const db = new DatabaseSync(path), repository = new DiscoveryRepository(db);
  try {
    db.exec("PRAGMA foreign_keys=ON");
    for (const [id, status, code] of [
      ["repair", "technical_stop", "provider_response_invalid_json"],
      ["connection-daemon", "technical_stop", "opencli_daemon_unavailable"],
      ["connection-extension", "technical_stop", "opencli_extension_unavailable"],
      ["connection-profile", "technical_stop", "opencli_profile_unavailable"],
      ["auth", "unauthorized", "opencli_auth_required"],
      ["risk", "risk_control_stop", "opencli_risk_control"],
    ] as const) {
      repository.create({ id, brief: "查找作者", opencli_version: "1.8.8", criteria: {}, scopes: [{ platform: "youtube", profile_ref: "work", keywords: ["first", "second"] }], target_count: 2 });
      repository.approve(id, 1);
      const claim = repository.claimSearch(id, 1, "youtube", "work");
      repository.finishSearch(claim, status, code, []);
      const run = repository.getPlan(id).platform_runs[0]!;
      const checkpoint = { expected_stop_code: code, expected_stopped_at: run.updated_at };
      if (id === "repair" || id.startsWith("connection-")) {
        assert.throws(() => repository.claimSearch(id, 1, "youtube", "work", true, { ...checkpoint, expected_stopped_at: run.updated_at - 1 }), /checkpoint_changed/);
        assert.throws(() => repository.claimSearch(id, 1, "youtube", "other", true, checkpoint), /outside_scope/);
        assert.equal(repository.getPlan(id).used_calls, 1);
        const next = repository.claimSearch(id, 1, "youtube", "work", true, checkpoint);
        assert.equal(next.query, id.startsWith("connection-") ? "first" : "second");
        assert.equal(next.callNumber, 2);
        assert.throws(() => repository.claimSearch(id, 1, "youtube", "work", true, checkpoint), /read_active/);
        assert.equal(repository.getPlan(id).used_calls, 2);
        repository.finishSearch(next, "completed", null, []);
        assert.equal(db.prepare("SELECT COUNT(*) n FROM discovery_search_completions WHERE discovery_campaign_id=?").get(id)?.n, 1);
      } else {
        assert.throws(() => repository.claimSearch(id, 1, "youtube", "work", true, checkpoint), /checkpoint_changed/);
        assert.equal(repository.getPlan(id).used_calls, 1);
      }
    }
  } finally { db.close(); }
});
