import assert from "node:assert/strict";
import { DatabaseSync } from "node:sqlite";
import { fileURLToPath } from "node:url";
import { test } from "node:test";
import { getDiscoverySavedContent, getDiscoverySavedPublicPage, getDiscoverySavedTranscript } from "../src/discovery/saved-read-repository.ts";
import { parseApplicationJson } from "../src/contracts/application-json.ts";

for (const kind of ["native", "web"]) test(`原Rust ${kind} 内容历史逐字段读取，跨计划与缺失回执明确失败`, () => {
  using database = new DatabaseSync(fileURLToPath(new URL(`./fixtures/discovery-contacts/${kind}-before.sqlite`, import.meta.url)), { readOnly: true });
  const source = database.prepare("SELECT * FROM discovery_content_reads ORDER BY search_hit_id LIMIT 1").get()!;
  assert.equal(typeof source.discovery_campaign_id, "string"); assert.equal(typeof source.search_hit_id, "string");
  const campaign = source.discovery_campaign_id as string, hit = source.search_hit_id as string, read = getDiscoverySavedContent(database, campaign, hit);
  assert.deepEqual(read, { claim: { campaign_id: campaign, expected_revision: source.expected_revision, search_hit_id: hit, platform: source.platform, profile_ref: source.profile_ref, author_id: source.author_id, source_url: source.source_url, call_number: source.call_number }, status: source.status, native_content: source.native_content_json === null ? null : parseApplicationJson(Buffer.from(source.native_content_json as string)), stop_code: source.stop_code, captured_at: source.updated_at });
  assert.throws(() => getDiscoverySavedContent(database, "another-campaign", hit), /not_found/);
  assert.throws(() => getDiscoverySavedTranscript(database, campaign, hit), /not_found/);
  assert.throws(() => getDiscoverySavedPublicPage(database, campaign, "not-a-candidate", "https://example.com"), /not_found/);
  assert.equal(database.isTransaction, false);
});
