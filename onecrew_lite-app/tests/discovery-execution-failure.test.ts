import assert from "node:assert/strict";
import { test } from "node:test";
import { DatabaseSync } from "node:sqlite";
import { copyFile, chmod, mkdir, mkdtemp, rm } from "node:fs/promises";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { DiscoveryRepository } from "../src/discovery/repository.ts";
import { discoveryExecutionFailureCode } from "../src/discovery/execution-failure.ts";

test("真实 SQLite 搜索仓储区分锁冲突、计划缺失、连接关闭和读取失败", async () => {
  const scratch = fileURLToPath(new URL("../../.scratch/", import.meta.url));
  await mkdir(scratch, { recursive: true });
  const directory = await mkdtemp(join(scratch, "discovery-failure-"));
  const path = join(directory, "creator.sqlite");
  await copyFile(new URL("./fixtures/discovery-identity/baseline.sqlite", import.meta.url), path);
  await chmod(path, 0o600);
  const writer = new DatabaseSync(path), reader = new DatabaseSync(path);
  const repository = new DiscoveryRepository(reader);
  try {
    assert.throws(() => repository.getPlan("missing"), error => discoveryExecutionFailureCode(error) === "discovery_plan_missing");
    writer.exec("PRAGMA journal_mode=DELETE; BEGIN EXCLUSIVE");
    assert.throws(() => repository.getPlan("merge-review"), error => discoveryExecutionFailureCode(error) === "discovery_repository_busy");
    writer.exec("ROLLBACK");
    assert.equal(repository.getPlan("merge-review").id, "merge-review");
    reader.close();
    assert.throws(() => repository.getPlan("merge-review"), error => discoveryExecutionFailureCode(error) === "discovery_repository_stopped");
    const broken = new DiscoveryRepository(writer);
    writer.exec("DROP TABLE discovery_campaign_revisions");
    assert.throws(() => broken.getPlan("merge-review"), error => discoveryExecutionFailureCode(error) === "discovery_repository_read_failed");
  } finally {
    if (reader.isOpen) reader.close();
    writer.close();
    await rm(directory, { recursive: true });
  }
});
