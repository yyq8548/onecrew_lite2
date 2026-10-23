import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { lstatSync } from "node:fs";
import { lstat, mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import test from "node:test";
import { parseApplicationJson } from "../src/contracts/application-json.ts";
import { DiscoveryMetricsWriter, FileDiscoveryExecutionMetrics } from "../src/discovery/execution-metrics.ts";

test("真实文件后台统计与新增批次在结束时完整排空", async () => {
  const directory = await mkdtemp(fileURLToPath(new URL("../../.scratch/metrics-writer-", import.meta.url)));
  const identity = lstatSync(directory);
  const metrics = new FileDiscoveryExecutionMetrics(directory, () => { assert.equal(lstatSync(directory).ino, identity.ino); });
  const writer = new DiscoveryMetricsWriter(metrics), campaign = "file-stat-verification";
  try {
    for (let count = 0; count < 20; count++) {
      const start = performance.now(); await lstat(directory);
      metrics.enqueue(campaign, "file.stat", Math.floor(performance.now() - start));
      writer.reap();
    }
    await writer.close();
    const name = createHash("sha256").update(campaign).digest("hex");
    const saved = parseApplicationJson(await readFile(join(directory, "discovery-metrics", `${name}.json`))) as { phases: Record<string, { calls: number | bigint }> };
    assert.equal(BigInt(saved.phases["file.stat"]!.calls), 20n);
    assert.equal((await readFile(join(directory, "discovery-metrics", `${name}.events.jsonl`), "utf8")).trim().split("\n").length, 20);
  } finally { await rm(directory, { recursive: true }); }
});

test("真实统计目录冲突会在排空时报告失败", async () => {
  const directory = await mkdtemp(fileURLToPath(new URL("../../.scratch/metrics-writer-error-", import.meta.url)));
  try {
    await writeFile(join(directory, "discovery-metrics"), "blocked", { mode: 0o600 });
    const metrics = new FileDiscoveryExecutionMetrics(directory, () => { assert.ok(lstatSync(directory).isDirectory()); });
    metrics.enqueue("directory-conflict", "file.stat", 0);
    const writer = new DiscoveryMetricsWriter(metrics); writer.reap();
    await assert.rejects(writer.close(), /discovery_metrics_flush_failed/);
    assert.throws(() => writer.reap(), /discovery_metrics_flush_failed/);
  } finally { await rm(directory, { recursive: true }); }
});
