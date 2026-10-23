import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";
import { DatabaseSync } from "node:sqlite";
import { test } from "node:test";
import { fileURLToPath } from "node:url";
import { z } from "zod";
import { parseApplicationJson, stringifyApplicationJson } from "../src/contracts/application-json.ts";
import { loadDiscoveryLibraryBasis } from "../src/discovery/library-basis.ts";
import { buildDiscoveryLibrarySnapshot } from "../src/discovery/library-material.ts";

test("四份真实SQLite保留来源和审计证据，粉丝量按完整数字显示且不写原库", () => {
  const root = new URL("./fixtures/discovery-identity/", import.meta.url);
  const oracle = z.record(z.string(), z.unknown()).parse(parseApplicationJson(readFileSync(new URL("oracle.json", root))));
  for (const [file, expectedKey] of [["baseline.sqlite", "before"], ["merged.sqlite", "merged"], ["stale.sqlite", "stale"], ["revoked.sqlite", "revoked"]]) {
    const path = fileURLToPath(new URL(file!, root)), originalHash = createHash("sha256").update(readFileSync(path)).digest("hex"), db = new DatabaseSync(path, { readOnly: true });
    try {
      assert.throws(() => loadDiscoveryLibraryBasis(db, "merge-review", 1), /requires_snapshot/);
      db.exec("BEGIN DEFERRED"); const snapshot = buildDiscoveryLibrarySnapshot(loadDiscoveryLibraryBasis(db, "merge-review", 1)); db.exec("COMMIT");
      const expected = z.object({ groups: z.array(z.object({ members: z.array(z.object({ sources: z.array(z.unknown()) })) })) }).parse(oracle[expectedKey!]);
      const sources = expected.groups.flatMap(group => group.members.flatMap(member => member.sources)).map(source => {
        const row = z.object({ followers: z.string().optional() }).passthrough().parse(source);
        return { ...row, ...(row.followers !== undefined ? { followers: row.followers.replace(/^~/u, "") } : {}) };
      });
      for (const row of snapshot.rows.filter(row => row.verdict === "pending" && row.candidate_id !== null)) assert.equal(row.reasons.filter(reason => /^(核实状态|下一步|最后核实时间)：/u.test(reason)).length, 3);
      const actual = snapshot.rows.map(({ hit, followers, candidate_id, native_author_id, verdict, reasons, evidence_version }) => ({ hit, ...(followers !== undefined ? { followers } : {}), candidate_id, native_author_id, verdict, reasons: reasons.filter(reason => !/^(核实状态|证据缺口|下一步|最后核实时间)：/u.test(reason)), evidence_version }));
      const sorted = (values: unknown[]) => values.map(value => z.object({ hit: z.object({ id: z.string() }) }).passthrough().parse(value)).sort((a, b) => a.hit.id.localeCompare(b.hit.id));
      assert.deepEqual(parseApplicationJson(Buffer.from(stringifyApplicationJson(sorted(actual)))), sorted(sources));
    } finally { db.close(); }
    assert.equal(createHash("sha256").update(readFileSync(path)).digest("hex"), originalHash);
  }
});
