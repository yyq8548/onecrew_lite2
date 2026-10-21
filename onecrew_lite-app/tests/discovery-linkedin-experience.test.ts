import assert from "node:assert/strict";
import { readFile, mkdtemp } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import { join } from "node:path";
import { DatabaseSync } from "node:sqlite";
import { test } from "node:test";
import { initializeNewCreatorDatabase } from "../src/storage/creator-schema.ts";
import { DiscoveryRepository } from "../src/discovery/repository.ts";
import { DiscoveryLinkedinExperienceRepository } from "../src/discovery/linkedin-experience.ts";
import { SELECTION_QUALITY_REVIEWER } from "../src/discovery/quality-policy.ts";
import { discoveryMatchProposal } from "../src/discovery/semantic-match.ts";
import { applyDiscoveryProfessionalReview } from "../src/discovery/professional-review.ts";
import { linkedinExperiencePlan } from "../src/platforms/opencli/linkedin.ts";
import { discoveryScoreHit } from "../src/discovery/execution-material.ts";

const materials = JSON.parse(await readFile(new URL("./fixtures/linkedin-occupational-headlines-20261021.json",import.meta.url),"utf8"));
const model = JSON.parse(await readFile(new URL("./fixtures/linkedin-experience-model-20261021.json",import.meta.url),"utf8"));
const examples: Array<{name:string;fixture:string;live:string;expected:"qualified"|"rejected"|"pending";stop?:string}> = [
  {name:"Liu Zhengyang",fixture:"linkedin-experience-failure-20261021.json",live:"linkedin-experience-live-liu-20261021.json",expected:"qualified"},
  {name:"Dylan X",fixture:"linkedin-experience-dylan-rows-20261021.json",live:"linkedin-experience-live-dylan-20261021.json",expected:"rejected"},
  {name:"Dylan X",fixture:"linkedin-experience-dylan-rows-20261021.json",live:"linkedin-experience-live-dylan-20261021.json",expected:"rejected",stop:"opencli_timeout"},
  {name:"Dylan X",fixture:"linkedin-experience-dylan-rows-20261021.json",live:"linkedin-experience-live-dylan-20261021.json",expected:"pending",stop:"interrupted_experience_read"},
];
for (const example of examples) {
const fixture=JSON.parse(await readFile(new URL(`./fixtures/${example.fixture}`,import.meta.url),"utf8"));
const live=JSON.parse(await readFile(new URL(`./fixtures/${example.live}`,import.meta.url),"utf8"));

test(`真实${example.name}只补查一次Experience：${example.stop??example.expected}，保留主页和失败历史`,async t=>{
  const material = materials.records[0].material;
  const directory = await mkdtemp(fileURLToPath(new URL("../../.scratch/experience-test-",import.meta.url)));
  const path = join(directory,"creator.sqlite");
  await initializeNewCreatorDatabase(path);
  const db = new DatabaseSync(path); t.after(()=>db.close());
  const repository = new DiscoveryRepository(db), campaign = material.campaign_id;
  repository.create({id:campaign,brief:"在LinkedIn搜索币安、OKX、Bitget、Bybit或Gate.io的商务、BD或Marketing人员。成功的账号不重复读取。币安（Binance）、OKX、Bitget、Bybit、Gate.io 的商务、BD 或 Marketing 人员",opencli_version:"1.8.8",criteria:material.criteria,scopes:material.approved_scopes,target_count:30,max_calls:null});
  repository.approve(campaign,1);
  db.prepare("UPDATE discovery_platform_runs SET status='completed' WHERE discovery_campaign_id=?").run(campaign);
  db.prepare("UPDATE discovery_campaigns SET used_calls=70 WHERE id=?").run(campaign);
  for (const [table,row] of Object.entries(fixture.rows)) {
    const value = row as Record<string,string|number|null>, columns = Object.keys(value);
    db.prepare(`INSERT INTO ${table}(${columns.join(",")}) VALUES(${columns.map(()=>"?").join(",")})`).run(...Object.values(value));
  }
  const hit = fixture.rows.discovery_search_hits.id;
  assert.equal(repository.nextContentHit(campaign,"linkedin"),hit);
  const profile = db.prepare("SELECT * FROM discovery_profile_reads WHERE search_hit_id=?").get(hit);
  const failure = db.prepare("SELECT * FROM discovery_content_reads WHERE search_hit_id=?").get(hit);
  const experience = new DiscoveryLinkedinExperienceRepository(db);
  const claim = experience.claim(campaign,1,hit);
  assert.equal(claim.call_number,71);
  assert.equal(repository.nextLinkedinExperience(campaign),null);
  const read=experience.finish(claim,example.stop?null:live.native,example.stop??null);
  assert.throws(()=>experience.claim(campaign,1,hit),/not_eligible/);
  assert.deepEqual(db.prepare("SELECT * FROM discovery_profile_reads WHERE search_hit_id=?").get(hit),profile);
  assert.deepEqual(db.prepare("SELECT * FROM discovery_content_reads WHERE search_hit_id=?").get(hit),failure);
  assert.equal(discoveryScoreHit({step:{kind:"read_content",search_hit_id:hit},campaign:repository.getPlan(campaign),counts:repository.countCandidates(campaign),content:read,profile:null,identity:null,transcript:null}),example.stop||live.native.entries.length===0?null:hit);
  if(example.stop) {
    assert.equal(db.prepare("SELECT verdict FROM discovery_candidates WHERE id=?").get(fixture.rows.discovery_candidates.id)!.verdict,example.expected);
    assert.equal(repository.getPlan(campaign).platform_runs[0]?.status,example.stop==="opencli_timeout"?"completed":"technical_stop");
    assert.equal(repository.getPlan(campaign).used_calls,71);
    return;
  }
  if(live.native.entries.length===0) {
    const candidate=db.prepare("SELECT verdict,evidence_json FROM discovery_candidates WHERE id=?").get(fixture.rows.discovery_candidates.id)!;
    assert.equal(candidate.verdict,"rejected");
    assert.ok(JSON.parse(String(candidate.evidence_json)).assessment.reasons.includes("linkedin_current_employment_evidence_exhausted"));
    assert.equal(repository.nextContentHit(campaign,"linkedin"),null);
    assert.equal(repository.getPlan(campaign).used_calls,71);
    return;
  }
  const saved = repository.prepareInitialMatchMaterial(campaign,1,hit).material;
  assert.equal(saved.native_profile?.experience_checked,true);
  const actual=model.results.find((record:{name:string})=>record.name===example.name);
  const checked=applyDiscoveryProfessionalReview(saved,discoveryMatchProposal.parse(actual.proposal),actual.audit);
  const findings=[checked.selection,checked.quality_rating,...checked.requirement_findings];
  const receipt = repository.submitReview("Ye7CHhx3OvrXP80JGxEJJVmabkhpM9Kn",{campaign_id:campaign,expected_revision:1,search_hit_id:hit,material_hash:saved.material_hash,request_id:"actual-experience-exhausted",reviewer:{kind:"agent",label:SELECTION_QUALITY_REVIEWER},findings});
  assert.equal(receipt.verdict,example.expected);
  if(example.expected==="rejected") assert.ok(receipt.reasons.includes("linkedin_current_employment_evidence_exhausted"));
  else assert.ok(saved.native_profile?.web_text?.includes("Binance"));
  assert.equal(repository.nextContentHit(campaign,"linkedin"),null);
  assert.equal(repository.getPlan(campaign).used_calls,71);
});
}

test("真实同公司职位分组保存公司与Present，永久账号不一致时拒绝接受",async()=>{
  const live=JSON.parse(await readFile(new URL("./fixtures/linkedin-experience-live-leah-20261021.json",import.meta.url),"utf8"));
  const native=live.native;
  const decoded=linkedinExperiencePlan(native.author_handle,native.account_id).decode([native]);
  assert.match(decoded.entries[0]!.text,/Binance.US[\s\S]*Chief of Staff[\s\S]*Present/);
  assert.throws(()=>linkedinExperiencePlan(native.author_handle,"ACoAAEFedrMBtRLS16uhO63agJY8BvgV9m0V7GM").decode([native]),/opencli_invalid_output/);
});
