import assert from "node:assert/strict";
import { readFile, mkdtemp } from "node:fs/promises";
import { join } from "node:path";
import { DatabaseSync } from "node:sqlite";
import { test } from "node:test";
import { fileURLToPath } from "node:url";
import { parseApplicationJson } from "../src/contracts/application-json.ts";
import { initializeNewCreatorDatabase } from "../src/storage/creator-schema.ts";
import { DiscoveryRepository } from "../src/discovery/repository.ts";
import { discoveryMatchProposal } from "../src/discovery/semantic-match.ts";
import { applyDiscoveryProfessionalReview } from "../src/discovery/professional-review.ts";
import { SELECTION_QUALITY_REVIEWER } from "../src/discovery/quality-policy.ts";

const fixture = JSON.parse(await readFile(new URL("./fixtures/linkedin-occupational-headlines-20261021.json", import.meta.url), "utf8"));
const results = JSON.parse(await readFile(new URL("./fixtures/linkedin-occupational-reviews-20261021.json", import.meta.url), "utf8"));
const snapshot = JSON.parse(await readFile(new URL("./fixtures/linkedin-occupational-alicia-rows-20261021.json", import.meta.url), "utf8"));

test("真实Affiliate Manager主页缺少简介和帖子时可独立通过，85分不触发帖子或主页重读", async t => {
  const { material, previous_assessment } = fixture.records.find((record: { name: string }) => record.name === "Alicia Foo");
  const actual = results.results.find((record: { name: string }) => record.name === "Alicia Foo");
  assert.equal(previous_assessment.selection.state, "unverified");
  assert.equal(material.native_content, undefined);
  const directory = await mkdtemp(fileURLToPath(new URL("../../.scratch/occupational-profile-", import.meta.url)));
  const path = join(directory, "creator.sqlite");
  await initializeNewCreatorDatabase(path);
  const db = new DatabaseSync(path); t.after(() => db.close());
  const repository = new DiscoveryRepository(db), campaign = material.campaign_id;
  repository.create({ id: campaign, brief: "在LinkedIn搜索币安、OKX、Bitget、Bybit或Gate.io的商务、BD或Marketing人员。成功的账号不重复读取。币安（Binance）、OKX、Bitget、Bybit、Gate.io 的商务、BD 或 Marketing 人员", opencli_version: "1.8.8", criteria: material.criteria, scopes: material.approved_scopes, target_count: 30 });
  repository.approve(campaign, 1);
  db.prepare("UPDATE discovery_campaign_revisions SET approved_at=? WHERE discovery_campaign_id=?").run(material.approved_at, campaign);
  db.prepare("UPDATE discovery_platform_runs SET status='completed' WHERE discovery_campaign_id=?").run(campaign);
  for (const [table, row] of Object.entries({ discovery_search_hits: snapshot.rows.hit, discovery_search_authors: snapshot.rows.author, discovery_profile_reads: snapshot.rows.profile, discovery_candidates: snapshot.rows.candidate })) {
    const values = row as Record<string, string | number | null>, columns = Object.keys(values);
    db.prepare(`INSERT INTO ${table}(${columns.join(",")}) VALUES(${columns.map(() => "?").join(",")})`).run(...Object.values(values));
  }
  const hit = material.hit.id, saved = repository.prepareInitialMatchMaterial(campaign, 1, hit).material;
  const checked = applyDiscoveryProfessionalReview(saved, discoveryMatchProposal.parse(actual.proposal), actual.audit);
  assert.equal(checked.selection.outcome, "supported");
  assert.equal(checked.quality_rating.match_level, "good");
  assert.ok(checked.requirement_findings.every(finding => finding.outcome === "supported"));
  const before = repository.getPlan(campaign).used_calls;
  const receipt = repository.submitReview("Ye7CHhx3OvrXP80JGxEJJVmabkhpM9Kn", { campaign_id: campaign, expected_revision: 1, search_hit_id: hit, material_hash: saved.material_hash, request_id: "occupational-profile-actual-20261021", reviewer: { kind: "agent", label: SELECTION_QUALITY_REVIEWER }, findings: [checked.selection, checked.quality_rating, ...checked.requirement_findings] });
  assert.equal(receipt.verdict, "qualified");
  assert.equal(repository.nextContentHit(campaign, "linkedin"), null);
  assert.equal(repository.nextProfileHit(campaign), null);
  assert.equal(repository.getPlan(campaign).used_calls, before);
  assert.equal(db.prepare("SELECT COUNT(*) AS n FROM discovery_content_reads").get()!.n, 0);
});

test("真实前任Bybit资料仍待核实，不能将职业经验替代当前雇主证据", () => {
  const { material } = fixture.records.find((record: { name: string }) => record.name === "Rachel Rekkab");
  const actual = results.results.find((record: { name: string }) => record.name === "Rachel Rekkab");
  const checked = applyDiscoveryProfessionalReview(material, discoveryMatchProposal.parse(actual.proposal), actual.audit);
  assert.ok(checked.requirement_findings.some(finding => finding.outcome !== "supported"));
  assert.equal(material.native_content, undefined);
  assert.equal(snapshot.rows.profile.status, "completed");
  assert.ok(parseApplicationJson(Buffer.from(snapshot.rows.profile.native_profile_json)));
});
