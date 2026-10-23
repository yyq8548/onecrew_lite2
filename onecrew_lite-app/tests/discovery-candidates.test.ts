import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { createHash } from "node:crypto";
import { DatabaseSync } from "node:sqlite";
import { test } from "node:test";
import { fileURLToPath } from "node:url";
import { DiscoveryCandidateRepository } from "../src/discovery/candidate-repository.ts";

test("原真实SQLite按审核排序分页，legacy计数保持原历史政策且排除来源不新增权限", () => {
  const root = new URL("./fixtures/discovery-identity/", import.meta.url);
  for (const file of ["baseline", "merged", "stale", "revoked"]) {
    const path = fileURLToPath(new URL(`${file}.sqlite`, root)), hash = createHash("sha256").update(readFileSync(path)).digest("hex"), db = new DatabaseSync(path, { readOnly: true });
    try {
      const repository = new DiscoveryCandidateRepository(db), all = repository.list({ campaign_id: "merge-review", after_id: null, limit: 100 });
      assert.equal(all.length, 2);
      const first = repository.list({ campaign_id: "merge-review", after_id: null, limit: 1 });
      const second = repository.list({ campaign_id: "merge-review", after_id: first[0]!.id, limit: 1 });
      assert.deepEqual([...first, ...second], all);
      assert.deepEqual(repository.list({ campaign_id: "merge-review", after_id: "unknown-cursor", limit: 1 }), []);
      assert.equal(repository.count("merge-review").qualified, 2);
      assert.deepEqual(repository.count("merge-review", ["youtube"]), { public_page_errors: 0, qualified: 0, pending: 0, rejected: 0 });
      assert.throws(() => repository.list({ campaign_id: "merge-review", after_id: null, limit: 101 }));
      assert.throws(() => repository.count("missing"), /plan_missing/);
    } finally { db.close(); }
    assert.equal(createHash("sha256").update(readFileSync(path)).digest("hex"), hash);
  }
});
