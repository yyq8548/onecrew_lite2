import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { mkdtemp } from "node:fs/promises";
import { join } from "node:path";
import { DatabaseSync } from "node:sqlite";
import { test } from "node:test";
import { fileURLToPath } from "node:url";
import { z } from "zod";
import { parseApplicationJson, stringifyApplicationJson } from "../src/contracts/application-json.ts";
import { initializeNewCreatorDatabase } from "../src/storage/creator-schema.ts";
import { createPrivateDirectory } from "../src/storage/private-backup.ts";
import { DiscoveryRepository } from "../src/discovery/repository.ts";
import { loadDiscoveryReviewBasis } from "../src/discovery/review-basis.ts";
import { buildDiscoveryReviewMaterial, discoveryReviewBasis } from "../src/discovery/review-material.ts";

test("真实schema90按原Rust材料还原已绑定YouTube一致快照，丢失绑定明确失败", async () => {
  const original = z.array(z.object({ basis: discoveryReviewBasis, expected: z.unknown() })).parse(parseApplicationJson(readFileSync(new URL("./fixtures/discovery-review-rust.json", import.meta.url))))[0]!;
  const basis = original.basis, hit = basis.hit;
  const directory = await mkdtemp(fileURLToPath(new URL("../../.scratch/discovery-basis-", import.meta.url)));
  const path = join(await createPrivateDirectory(join(directory, "private")), "creator.sqlite"); await initializeNewCreatorDatabase(path);
  const db = new DatabaseSync(path), repository = new DiscoveryRepository(db);
  try {
    db.exec("PRAGMA foreign_keys=ON");
    repository.create({ id: basis.campaign_id, brief: "原Rust离线合同", opencli_version: "1.8.8", criteria: parseApplicationJson(Buffer.from(basis.criteria_json)), scopes: parseApplicationJson(Buffer.from(basis.scopes_json)), target_count: 30 });
    repository.approve(basis.campaign_id, basis.revision);
    db.prepare("UPDATE discovery_campaign_revisions SET approved_at=?,criteria_json=?,scope_json=? WHERE discovery_campaign_id=? AND revision=?").run(basis.approved_at, basis.criteria_json, basis.scopes_json, basis.campaign_id, basis.revision);
    db.prepare("INSERT INTO discovery_search_hits(id,discovery_campaign_id,platform,profile_ref,search_keyword,source_url,title,author_label,captured_at) VALUES(?,?,?,?,?,?,?,?,?)").run(hit.id, hit.campaign_id, hit.platform, hit.profile_ref, hit.search_keyword, hit.source_url, hit.title, hit.author_label, hit.captured_at);
    db.prepare("INSERT INTO discovery_candidates(id,discovery_campaign_id,criteria_revision,platform,source_platform,source_profile_ref,platform_account_id,display_name,profile_url,verdict,evidence_json,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?)").run(basis.candidate_id, basis.campaign_id, basis.revision, hit.platform, hit.platform, hit.profile_ref, basis.native_author_id, hit.author_label, `https://www.youtube.com/channel/${basis.native_author_id}`, "pending", basis.evidence_json, 1, 1);
    db.prepare("INSERT INTO discovery_video_completions(search_hit_id,completed_at) VALUES(?,?)").run(hit.id, 1);
    db.prepare("INSERT INTO discovery_video_identity_bindings(search_hit_id,discovery_campaign_id,criteria_revision,candidate_id,native_author_id,created_at) VALUES(?,?,?,?,?,?)").run(hit.id, basis.campaign_id, basis.revision, basis.candidate_id, basis.native_author_id, 1);
    assert.throws(() => loadDiscoveryReviewBasis(db, basis.campaign_id, basis.revision, hit.id), /requires_snapshot/);
    db.exec("BEGIN DEFERRED");
    const loaded = loadDiscoveryReviewBasis(db, basis.campaign_id, basis.revision, hit.id);
    assert.deepEqual(parseApplicationJson(Buffer.from(stringifyApplicationJson(buildDiscoveryReviewMaterial(loaded)))), original.expected);
    db.exec("COMMIT");
    db.prepare("DELETE FROM discovery_video_identity_bindings WHERE search_hit_id=?").run(hit.id);
    db.exec("BEGIN DEFERRED");
    assert.throws(() => loadDiscoveryReviewBasis(db, basis.campaign_id, basis.revision, hit.id), /author_binding_unavailable/);
    db.exec("ROLLBACK");
  } finally { db.close(); }
});
