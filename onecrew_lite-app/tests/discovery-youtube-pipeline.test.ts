import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { mkdir, mkdtemp, rm } from "node:fs/promises";
import { join } from "node:path";
import { DatabaseSync, type SQLInputValue } from "node:sqlite";
import { test, type TestContext } from "node:test";
import { fileURLToPath } from "node:url";
import { parseApplicationJson } from "../src/contracts/application-json.ts";
import { DiscoveryRepository } from "../src/discovery/repository.ts";
import { DiscoveryVideoRepository, type DiscoveryVideoCandidate } from "../src/discovery/video-repository.ts";
import { SELECTION_QUALITY_REVIEWER } from "../src/discovery/quality-policy.ts";
import { loadDiscoveryContactObservations } from "../src/discovery/contact-repository.ts";
import { discoveryObservesLink, discoveryPromotional, discoveryThirdPartyLink } from "../src/discovery/contact-navigation.ts";
import { extractDiscoveryContacts } from "../src/discovery/contacts.ts";
import { containsDiscoveryCitation } from "../src/discovery/review-material.ts";
import { initializeNewCreatorDatabase } from "../src/storage/creator-schema.ts";

type Row = Record<string, SQLInputValue>;
interface CapturedCampaign { campaign: Row; revision: Row; run: Row }
const captured = parseApplicationJson(readFileSync(new URL("./fixtures/youtube-regression-capture-20261028.json", import.meta.url))) as {
  historical: CapturedCampaign & { hit: Row; author?: Row; candidate: Row; completion: Row; binding: Row };
  current: CapturedCampaign & { hits: Row[]; authors: Row[]; candidates: Row[] };
};
function insert(db: DatabaseSync, table: string, row: Row) {
  const keys = Object.keys(row);
  db.prepare(`INSERT INTO ${table}(${keys.join(",")}) VALUES(${keys.map(() => "?").join(",")})`).run(...Object.values(row));
}
async function fixture(t: TestContext) {
  const scratch = fileURLToPath(new URL("../../.scratch/youtube-pipeline-tests/", import.meta.url));
  await mkdir(scratch, { recursive: true });
  const directory = await mkdtemp(join(scratch, "recorded-")), path = join(directory, "creator.sqlite");
  await initializeNewCreatorDatabase(path);
  const db = new DatabaseSync(path);
  t.after(async () => { db.close(); await rm(directory, { recursive: true }); });
  db.exec("PRAGMA foreign_keys=ON; BEGIN IMMEDIATE; PRAGMA defer_foreign_keys=ON");
  for (const source of [captured.historical, captured.current]) {
    insert(db, "discovery_campaigns", source.campaign);
    insert(db, "discovery_campaign_revisions", source.revision);
    // 回放已完成搜索后的队列，不恢复本机暂停时的联系页领取。
    insert(db, "discovery_platform_runs", { ...source.run, status: "completed", active_query: null, stop_code: null });
    insert(db, "discovery_recommendation_policies", { discovery_campaign_id: source.campaign.id!, criteria_revision: 1, policy: "score_and_contact_v1" });
  }
  const old = captured.historical;
  for (const [table, row] of Object.entries({ discovery_search_hits: old.hit, ...(old.author ? { discovery_search_authors: old.author } : {}), discovery_candidates: old.candidate, discovery_video_completions: old.completion, discovery_video_identity_bindings: old.binding })) insert(db, table, row);
  for (const row of captured.current.hits) insert(db, "discovery_search_hits", row);
  for (const row of captured.current.authors) insert(db, "discovery_search_authors", row);
  db.exec("COMMIT");
  return { db, repository: new DiscoveryRepository(db), videos: new DiscoveryVideoRepository(db), campaign: String(captured.current.campaign.id), profile: String(captured.current.run.profile_ref) };
}
function nativeCandidate(row: Row, campaign: string): DiscoveryVideoCandidate {
  return { campaign_id: campaign, expected_revision: 1, platform: "youtube", source_profile_ref: String(row.source_profile_ref), platform_account_id: String(row.platform_account_id), display_name: String(row.display_name), profile_url: String(row.profile_url), evidence: parseApplicationJson(String(row.evidence_json)) as Record<string, unknown> };
}

test("真实YouTube候选评分期间放行另一已核实作者，保持身份、浏览器互斥及待核实约束", async t => {
  const { repository, videos, campaign, profile } = await fixture(t);
  const held = captured.current.hits.find(row => row.author_label === "Jeff Su")!;
  const native = captured.current.candidates.find(row => row.display_name === "Jeff Su")!;
  videos.finish(videos.claim(campaign, 1, profile, String(held.id)), nativeCandidate(native, campaign), null);
  const used = repository.getPlan(campaign).used_calls;
  const expected = captured.current.hits.find(row => row.author_label === "Dan Martell")!;
  assert.deepEqual(repository.planStepScoring(campaign, 1, [], [], [String(held.id)]), { kind: "read_video", profile_ref: profile, search_hit_id: String(expected.id) });
  assert.equal(repository.nextIndependentRead(campaign, 1, String(held.id), [String(held.id)], { holdDiscovery: true }), null);
  assert.equal(repository.nextIndependentRead(campaign, 1, String(held.id), [String(held.id)], { busyPlatforms: ["youtube"] }), null);
  const others = captured.current.hits.filter(row => ["AI Master", "Dan Martell"].includes(String(row.author_label))).map(row => String(row.id));
  assert.equal(repository.nextIndependentRead(campaign, 1, String(held.id), [String(held.id)], { heldQueryHits: others }), null);
  assert.equal(repository.getPlan(campaign).used_calls, used);
});

test("旧材料缺少必需条件时只补读一次新视频，真实原生材料成功后保留两次来源", async t => {
  const { db, repository, videos, campaign, profile } = await fixture(t);
  const criteria = parseApplicationJson(String(captured.current.revision.criteria_json)) as Record<string, unknown>;
  criteria.requirements = [{ description: "视频在最近七天内发布", excerpt: "最近七天" }];
  db.prepare("UPDATE discovery_campaign_revisions SET criteria_json=? WHERE discovery_campaign_id=?").run(JSON.stringify(criteria), campaign);
  const hit = captured.current.hits.find(row => row.author_label === "AI Master")!;
  assert.equal(videos.reuse(videos.claim(campaign, 1, profile, String(hit.id))), true);
  const material = repository.prepareReviewMaterial(campaign, 1, String(hit.id));
  const missing = { outcome: "insufficient", match_level: null, rationale: "保存的标题和简介没有视频发布日期，不能确认最近七天条件", citations: [], tool: null, country_code: null };
  repository.submitReview("retained-youtube-regression", { campaign_id: campaign, expected_revision: 1, search_hit_id: hit.id, material_hash: material.material_hash, request_id: "saved-publication-date-missing", reviewer: { kind: "agent", label: SELECTION_QUALITY_REVIEWER }, findings: [{ ...missing, criterion: "selection_fit" }, { ...missing, criterion: "quality_rating" }, { ...missing, criterion: { requirement: 0 } }] });
  assert.deepEqual(videos.next(campaign), { id: hit.id, profile_ref: profile });
  const supplement = videos.claim(campaign, 1, profile, String(hit.id));
  assert.equal(videos.reuse(supplement), false);
  const fresh = parseApplicationJson(readFileSync(new URL("./fixtures/ai-master-fresh-native-20261028.json", import.meta.url))) as Row;
  videos.finish(supplement, nativeCandidate(fresh, campaign), null);
  const updated = repository.prepareReviewMaterial(campaign, 1, String(hit.id));
  assert.equal(updated.video_title, (parseApplicationJson(String(fresh.evidence_json)) as Record<string, unknown>).video_title);
  assert.equal(updated.native_content, undefined);
  assert.throws(() => videos.claim(campaign, 1, profile, String(hit.id)), /discovery_video_already_saved/);
  assert.equal(db.prepare("SELECT COUNT(*) AS n FROM discovery_video_completions WHERE search_hit_id IN(?,?)").get(hit.id!, captured.historical.hit.id!)!.n, 2);
});

test("同一原生作者的新视频复用真实旧材料，保留原视频来源和采集时间", async t => {
  const { db, repository, videos, campaign, profile } = await fixture(t);
  const hit = captured.current.hits.find(row => row.author_label === "AI Master")!;
  const claim = videos.claim(campaign, 1, profile, String(hit.id));
  assert.equal(videos.reuse(claim), true);
  const material = repository.prepareReviewMaterial(campaign, 1, String(hit.id));
  const saved = parseApplicationJson(String(captured.historical.candidate.evidence_json)) as Record<string, unknown>;
  assert.equal(material.hit.source_url, hit.source_url);
  assert.equal(material.native_author_id, captured.historical.candidate.platform_account_id);
  assert.equal(material.native_content?.claim.source_url, captured.historical.hit.source_url);
  assert.equal(material.native_content?.captured_at, saved.captured_at);
  assert.equal((material.native_content?.native_content as Record<string, unknown>).text, `${saved.video_title}\n\n${saved.video_description}`);
  assert.equal(material.video_title, undefined);
  assert.equal(material.video_description, null);
  assert.equal(containsDiscoveryCitation(material, { kind: "native_content", quote: String(saved.video_title) }), true);
  assert.notEqual(material.native_follower_estimate, undefined);
  assert.equal(repository.getPlan(campaign).used_calls, Number(captured.current.campaign.used_calls) + 1);
  assert.throws(() => videos.claim(campaign, 1, profile, String(hit.id)), /discovery_video_already_saved/);
  const evidence = String(db.prepare("SELECT evidence_json FROM discovery_candidates WHERE discovery_campaign_id=? AND platform_account_id=?").get(campaign, material.native_author_id)!.evidence_json);
  assert.equal(evidence.includes('"reviewed_selection"'), false);
});

test("未取得原生作者ID的新搜索结果不能按相似名称复用历史账号", async t => {
  const { videos, campaign, profile } = await fixture(t);
  const hit = captured.current.hits.find(row => row.author_label === "Ed Hill | AI Automation")!;
  assert.equal(videos.reuse(videos.claim(campaign, 1, profile, String(hit.id))), false);
});

test("真实简介按完整段落排除嘉宾与推广资源，作者自己的社交联系继续保留", () => {
  const samples = parseApplicationJson(readFileSync(new URL("./fixtures/contact-native-samples-20261028.json", import.meta.url))) as { display_name: string; author: string; source: string; text: string }[];
  const greg = samples.find(row => row.display_name === "Greg Isenberg")!;
  assert.equal(discoveryThirdPartyLink("https://x.com/remy_gaskell", greg.text, greg.display_name), true);
  assert.equal(discoveryThirdPartyLink("https://www.instagram.com/aiwithremy/", greg.text, greg.display_name), true);
  assert.equal(discoveryThirdPartyLink("https://twitter.com/gregisenberg", greg.text, greg.display_name), false);
  const contacts = extractDiscoveryContacts(greg.text, greg.source, greg.author, 1793227103, "author_published", greg.display_name);
  assert.ok(contacts.some(contact => contact.normalized_value === "https://twitter.com/gregisenberg"));
  assert.equal(contacts.some(contact => /remy_gaskell|aiwithremy/.test(contact.normalized_value)), false);
  for (const row of samples) for (const match of row.text.matchAll(/https:\/\/(?:filmora\.go\.link|base44\.pxf\.io)\/[^\s]+/gu)) {
    assert.equal(discoveryObservesLink(row.text, match[0]), true);
    assert.equal(discoveryPromotional(match[0], row.text), true);
  }
});

test("历史联系投影从完整原文纠正嘉宾归属，原联系人记录保持不变", async t => {
  const { db, campaign } = await fixture(t);
  const saved = parseApplicationJson(readFileSync(new URL("./fixtures/contact-projection-capture-20261028.json", import.meta.url))) as { candidate: Row; contacts: Row[] };
  insert(db, "discovery_candidates", saved.candidate);
  for (const contact of saved.contacts) insert(db, "discovery_contact_observations", contact);
  db.exec("BEGIN");
  const social = loadDiscoveryContactObservations(db, campaign, 1, String(saved.candidate.id), String(saved.candidate.platform_account_id), true);
  const all = loadDiscoveryContactObservations(db, campaign, 1, String(saved.candidate.id), String(saved.candidate.platform_account_id));
  db.exec("COMMIT");
  assert.ok(social.some(contact => /gregisenberg/.test(contact.normalized_value)));
  assert.equal(social.some(contact => /remy_gaskell|aiwithremy/.test(contact.normalized_value)), false);
  assert.equal(all.some(contact => /remy_gaskell|aiwithremy/.test(contact.normalized_value)), false);
  assert.equal(db.prepare("SELECT COUNT(*) AS n FROM discovery_contact_observations WHERE candidate_id=?").get(saved.candidate.id!)!.n, saved.contacts.length);
});
