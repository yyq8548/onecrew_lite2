import assert from "node:assert/strict";
import { copyFile, mkdtemp } from "node:fs/promises";
import { join } from "node:path";
import { DatabaseSync } from "node:sqlite";
import { test } from "node:test";
import { fileURLToPath } from "node:url";
import { createPrivateDirectory } from "../src/storage/private-backup.ts";
import { DiscoveryLibraryRepository } from "../src/discovery/library-repository.ts";
import { buildDiscoveryLibrarySnapshot } from "../src/discovery/library-material.ts";

test("真实原SQLite双连接Library版本CAS拒绝旧来源，相同hash幂等且耗尽不写入", async () => {
  const directory = await mkdtemp(fileURLToPath(new URL("../../.scratch/discovery-library-cas-", import.meta.url))), path = join(await createPrivateDirectory(join(directory, "private")), "creator.sqlite");
  await copyFile(new URL("./fixtures/discovery-identity/baseline.sqlite", import.meta.url), path);
  const database = new DatabaseSync(path), other = new DatabaseSync(path), first = new DiscoveryLibraryRepository(database), second = new DiscoveryLibraryRepository(other);
  try {
    database.exec("PRAGMA foreign_keys=ON"); other.exec("PRAGMA foreign_keys=ON");
    const basis = first.readBasis("merge-review", 1), snapshot = buildDiscoveryLibrarySnapshot(basis);
    const initial = first.reserveVersion(basis, snapshot.snapshot_hash);
    assert.equal(second.reserveVersion(basis, snapshot.snapshot_hash), initial);
    const hit = basis.rows[0]!.hit.id;
    other.prepare("UPDATE discovery_search_hits SET title=title||? WHERE id=?").run(" Unicode 测试", hit);
    assert.throws(() => first.reserveVersion(basis, snapshot.snapshot_hash), /library_source_changed/);
    const updated = second.prepareSnapshot("merge-review", 1);
    assert.equal(updated.source_revision, initial + 1); assert.notEqual(updated.snapshot_hash, snapshot.snapshot_hash);
    assert.equal(first.prepareSnapshot("merge-review", 1).source_revision, updated.source_revision);
    database.prepare("UPDATE discovery_library_source_versions SET source_revision=? WHERE discovery_campaign_id=?").run(Number.MAX_SAFE_INTEGER, "merge-review");
    other.prepare("UPDATE discovery_search_hits SET title=title||? WHERE id=?").run(" next", hit);
    assert.throws(() => first.prepareSnapshot("merge-review", 1), /library_source_version_exhausted/);
    assert.equal(database.prepare("SELECT source_revision FROM discovery_library_source_versions WHERE discovery_campaign_id=?").get("merge-review")?.source_revision, Number.MAX_SAFE_INTEGER);
    assert.throws(() => first.reserveVersion(basis, "bad"), /library_source_hash_invalid/);
  } finally { other.close(); database.close(); }
});
