import assert from "node:assert/strict";
import { mkdtemp, copyFile } from "node:fs/promises";
import { join } from "node:path";
import { DatabaseSync } from "node:sqlite";
import { test } from "node:test";
import { fileURLToPath } from "node:url";
import { createPrivateDirectory } from "../src/storage/private-backup.ts";
import { DiscoveryRepository } from "../src/discovery/repository.ts";
import { parseApplicationJson, stringifyApplicationJson } from "../src/contracts/application-json.ts";
import { stripUnauditedDiscoveryProofs } from "../src/discovery/review-proof-audit.ts";
import { INITIAL_MATCH_REVIEWER } from "../src/discovery/initial-match.ts";
import { discoveryReviewReceipt } from "@onecrew/runtime-contracts/discovery";

test("真实作者资料经审核事务提交额外条件，伪造证明和跨版本证明被剔除", async () => {
  const directory = await mkdtemp(fileURLToPath(new URL("../../.scratch/discovery-requirement-ledger-", import.meta.url)));
  const path = join(await createPrivateDirectory(join(directory, "private")), "creator.sqlite");
  await copyFile(fileURLToPath(new URL("./fixtures/discovery-identity/baseline.sqlite", import.meta.url)), path);
  const db = new DatabaseSync(path), repository = new DiscoveryRepository(db);
  try {
    db.exec("PRAGMA foreign_keys=ON");
    const hit = db.prepare("SELECT id FROM discovery_search_hits ORDER BY id LIMIT 1").get()!.id as string;
    const original = repository.prepareReviewMaterial("merge-review", 1, hit);
    const quote = original.video_description!;
    const requirement = { description: "核实已观察的作者内容", excerpt: quote };
    db.prepare("UPDATE discovery_campaign_revisions SET criteria_json=? WHERE discovery_campaign_id=? AND revision=1").run(stringifyApplicationJson({ requires_ai_tool_demo: false, requires_public_tool_url: false, requirements: [requirement] }), "merge-review");
    const material = repository.prepareReviewMaterial("merge-review", 1, hit);
    const receipt = repository.submitReview("owner-requirements", { campaign_id: "merge-review", expected_revision: 1, search_hit_id: hit, material_hash: material.material_hash, request_id: "additional-condition", reviewer: { kind: "agent", label: INITIAL_MATCH_REVIEWER }, findings: [
      { criterion: "match_fit", outcome: "supported", match_level: "good", rationale: quote, citations: [{ kind: "description", quote }] },
      { criterion: { requirement: 0 }, outcome: "supported", rationale: quote, citations: [{ kind: "description", quote }] },
    ] });
    assert.deepEqual(receipt.judgments[1]?.criterion, { requirement: 0 });
    assert.equal(discoveryReviewReceipt.safeParse(receipt).success, true);
    const evidence = parseApplicationJson(db.prepare("SELECT evidence_json FROM discovery_candidates WHERE id=?").get(material.candidate_id)!.evidence_json as string) as Record<string, unknown>;
    db.exec("BEGIN DEFERRED");
    const audited = stripUnauditedDiscoveryProofs(db, evidence, material.candidate_id, "merge-review", 1, material.native_author_id);
    assert.deepEqual(audited.reviewed_requirements, evidence.reviewed_requirements);
    assert.equal((audited.reviewed_requirements as unknown[]).length, 1);
    const forged = structuredClone(evidence);
    (forged.reviewed_requirements as Record<string, unknown>[])[0]!.material_hash = "0".repeat(64);
    assert.equal(stripUnauditedDiscoveryProofs(db, forged, material.candidate_id, "merge-review", 1, material.native_author_id).reviewed_requirements, undefined);
    assert.equal(stripUnauditedDiscoveryProofs(db, evidence, material.candidate_id, "merge-review", 2, material.native_author_id).reviewed_requirements, undefined);
    db.exec("COMMIT");
  } finally { db.close(); }
});
