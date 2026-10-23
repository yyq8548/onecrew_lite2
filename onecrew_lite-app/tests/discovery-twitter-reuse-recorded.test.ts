import assert from "node:assert/strict";
import { test } from "node:test";
import { DatabaseSync } from "node:sqlite";
import { mkdtemp, rm } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import { join } from "node:path";
import { initializeNewCreatorDatabase } from "../src/storage/creator-schema.ts";
import { createPrivateDirectory } from "../src/storage/private-backup.ts";
import { DiscoveryRepository } from "../src/discovery/repository.ts";
import { loadDiscoveryReviewBasis } from "../src/discovery/review-basis.ts";
import { decodeCreatorSearch } from "../src/platforms/opencli/search-results.ts";
import { parseApplicationJson } from "../src/contracts/application-json.ts";

const source = process.env.ONECREW_QUERY_FEEDBACK_DATABASE;
test("真实X回执复用同次身份正文，主页缺失时仅补主页，保留长正文且不重复读取", { skip: !source }, async () => {
  const root = await mkdtemp(fileURLToPath(new URL("../../.scratch/twitter-reuse-", import.meta.url))), directory = await createPrivateDirectory(join(root, "private"));
  const path = join(directory, "creator.sqlite"); await initializeNewCreatorDatabase(path);
  const db = new DatabaseSync(path), original = new DatabaseSync(source!, { readOnly: true });
  const decode = (value: unknown) => parseApplicationJson(Buffer.from(String(value)));
  try {
    const row = original.prepare(`SELECT h.*,a.author_id,a.author_url,i.account_id,i.observed_source_url,i.post_id,p.native_profile_json,c.native_content_json
      FROM discovery_search_hits h JOIN discovery_search_authors a ON a.search_hit_id=h.id
      JOIN discovery_identity_reads i ON i.search_hit_id=h.id AND i.status='completed'
      JOIN discovery_profile_reads p ON p.search_hit_id=h.id AND p.status='completed'
      JOIN discovery_content_reads c ON c.search_hit_id=h.id AND c.status='completed'
      WHERE h.platform='twitter' AND i.account_id=json_extract(p.native_profile_json,'$.account_id')
        AND i.account_id=json_extract(c.native_content_json,'$.identity.account_id')
      ORDER BY length(json_extract(c.native_content_json,'$.text')) DESC LIMIT 1` ).get()!;
    assert.ok(row);
    const prior = new DiscoveryRepository(original).getPlan(String(row.discovery_campaign_id)), content = decode(row.native_content_json) as { text: string }, profile = decode(row.native_profile_json) as { bio: string };
    const repo = new DiscoveryRepository(db), campaign = prior.id;
    repo.create({ id: campaign, brief: prior.brief, opencli_version: prior.opencli_version, criteria: prior.criteria, scopes: [{ platform: "twitter", profile_ref: String(row.profile_ref), keywords: [String(row.search_keyword)] }], target_count: prior.target_count });
    repo.approve(campaign, 1);
    const search = repo.claimSearch(campaign, 1, "twitter", String(row.profile_ref));
    const hits = decodeCreatorSearch("twitter", [{ id: row.post_id, author: row.author_id, url: row.source_url, text: content.text, bio: profile.bio }], 1);
    assert.ok(content.text.length > 500);
    assert.equal(hits[0]!.material!.body_text, content.text);
    assert.equal(hits[0]!.material!.native_author_id, null, "搜索中缺失的原生ID保持未知");
    repo.finishSearch(search, "completed", null, hits);
    const hit = String(db.prepare("SELECT id FROM discovery_search_hits").get()!.id), identity = repo.claimPostAuthor(campaign, 1, hit);
    repo.finishPostAuthor(identity, { account_id: row.account_id, observed_source_url: row.observed_source_url, stop_code: null, native_content: content, native_profile: null });
    const profileClaim = repo.claimProfile(campaign, 1, hit);
    repo.finishProfile(profileClaim, profile, null);
    const used = repo.getPlan(campaign).used_calls, saved = repo.getContent(campaign, hit);
    assert.equal(used, 3, "搜索、合并身份正文、补主页各一次");
    assert.equal(saved.claim.call_number, identity.call_number, "复用原调用编号，不伪造新的平台请求");
    assert.equal(saved.status, "completed"); assert.deepEqual(saved.native_content, content);
    assert.equal(repo.nextContentHit(campaign, "twitter"), null);
    assert.equal(repo.nextProfileHit(campaign), null);
    db.exec("BEGIN"); const basis = loadDiscoveryReviewBasis(db, campaign, 1, hit); db.exec("COMMIT");
    assert.equal(basis.native_author_id, row.account_id);
    assert.equal(basis.native_content!.claim.call_number, identity.call_number);
    assert.equal(repo.getPlan(campaign).used_calls, used);
  } finally { original.close(); db.close(); await rm(root, { recursive: true, force: true }); }
});
