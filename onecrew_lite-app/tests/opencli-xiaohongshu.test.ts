import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { copyFile, mkdtemp } from 'node:fs/promises';
import { join } from 'node:path';
import { DatabaseSync } from 'node:sqlite';
import { test } from 'node:test';
import { fileURLToPath } from 'node:url';
import { xiaohongshuReadPlan } from '../src/platforms/opencli/xiaohongshu.ts';
import { decodeCreatorSearch } from '../src/platforms/opencli/search-results.ts';
import { classifyOpenCliFailure } from '../src/platforms/opencli/process.ts';
import { creatorAuthProbe } from '../src/platforms/opencli/auth-status.ts';
import { discoveryContactPlatform, discoverySocialProfile } from '../src/discovery/contacts.ts';
import { initializeNewCreatorDatabase, validateCreatorSchemaVersion } from '../src/storage/creator-schema.ts';
import { migrateCreatorReceiptPlatformEdge } from '../src/storage/creator-receipt-platform-edge.ts';
import { backupLegacyDatabase } from '../src/storage/legacy-sqlite.ts';
import { upgradeCreatorStagingCopy } from '../src/storage/creator-upgrade.ts';
import { DiscoveryRepository } from '../src/discovery/repository.ts';
import { containsDiscoveryCitation } from '../src/discovery/review-material.ts';
import { stringifyApplicationJson } from '../src/contracts/application-json.ts';
import { SELECTION_QUALITY_REVIEWER } from '../src/discovery/quality-policy.ts';

const fixture = (name: string) => JSON.parse(readFileSync(new URL(`./fixtures/xiaohongshu-${name}-live-20261027.json`, import.meta.url), 'utf8'));
const search = fixture('search'), profile = fixture('profile')[0], content = fixture('content')[0];
const hit = decodeCreatorSearch('xiaohongshu', search, 8).find(row => row.author_id === profile.author_id)!;

test('真实搜索与原生作者、完整正文进入当前校验，重复笔记按原生ID合并', () => {
  assert.equal(decodeCreatorSearch('xiaohongshu', search, 8).length, 8);
  const native = xiaohongshuReadPlan('content', profile.author_id, hit.source_url).decode([content]);
  assert.equal(native.author_id, profile.author_id);
  assert.match(String(native.text), /LangChain/);
  assert.throws(() => xiaohongshuReadPlan('content', '0'.repeat(24), hit.source_url).decode([content]), { code: 'opencli_invalid_output' });
  assert.throws(() => xiaohongshuReadPlan('content', profile.author_id, hit.source_url).decode([{ ...content, note_id: '0'.repeat(24) }]), { code: 'opencli_invalid_output' });
  assert.equal(decodeCreatorSearch('xiaohongshu', [search[0], search[0]], 2).length, 1);
  assert.throws(() => decodeCreatorSearch('xiaohongshu', [{ ...search[0], author_url: profile.profile_url + '/comments' }], 1));
});

test('粉丝未知保留null，主页链接属于公开联系方式，平台登录与具体错误分开', () => {
  const read = xiaohongshuReadPlan('profile', profile.author_id, profile.profile_url);
  assert.equal(read.decode([profile]).follower_count, 9403);
  assert.equal(read.decode([{ ...profile, follower_count: null, follower_display: null }]).follower_count, null);
  assert.equal(discoveryContactPlatform(profile.profile_url), 'xiaohongshu');
  assert.equal(discoverySocialProfile(profile.profile_url), true);
  assert.equal(creatorAuthProbe('xiaohongshu').site, 'onecrew-xiaohongshu-auth');
  for (const [message, expected] of [['ONECREW_XIAOHONGSHU_AUTH', 'opencli_auth_required'], ['ONECREW_XIAOHONGSHU_RISK', 'opencli_risk_control'], ['ONECREW_XIAOHONGSHU_PAGE_TIMEOUT', 'opencli_timeout'], ['ONECREW_XIAOHONGSHU_UNAVAILABLE', 'provider_source_unavailable'], ['ONECREW_XIAOHONGSHU_CONTENT_IDENTITY_INVALID', 'provider_response_shape_invalid']]) {
    const failure = classifyOpenCliFailure(1, Buffer.alloc(0), Buffer.from(message!));
    assert.equal(failure.code, expected); assert.ok(JSON.stringify(failure).includes(message!));
  }
});

test('真实材料经搜索、主页、按需正文和评分材料通路保存；跨任务复用成功账号', async t => {
  const directory = await mkdtemp(fileURLToPath(new URL('../../.scratch/xiaohongshu-flow-', import.meta.url))), path = join(directory, 'creator.sqlite');
  await initializeNewCreatorDatabase(path);
  const db = new DatabaseSync(path); t.after(() => db.close()); db.exec('PRAGMA foreign_keys=ON');
  const repository = new DiscoveryRepository(db), criteria = { category: 'AI Agent', min_subscribers: 2000, contact_preference: 'public_email_first' };
  const add = (campaign: string, minimum: number) => {
    repository.create({ id: campaign, brief: '在小红书找AI Agent相关创作者，粉丝至少2000', opencli_version: '1.8.8', criteria: { ...criteria, min_subscribers: minimum }, target_count: 10,
      scopes: [{ platform: 'xiaohongshu', profile_ref: 'captured', keywords: ['AI Agent 教程'] }] });
    repository.approve(campaign, 1);
    repository.finishSearch(repository.claimSearch(campaign, 1, 'xiaohongshu', 'captured'), 'completed', null, [hit]);
    return String(db.prepare('SELECT id FROM discovery_search_hits WHERE discovery_campaign_id=?').get(campaign)!.id);
  };
  const first = add('xhs-first', 2000), claim = repository.claimProfile('xhs-first', 1, first);
  repository.finishProfile(claim, profile, null);
  assert.equal(repository.nextProfileHit('xhs-first'), null);
  assert.equal(repository.nextContentHit('xhs-first', 'xiaohongshu'), null, '资料足够时先评分，尚无缺证据判断不补读');
  const material = repository.prepareReviewMaterial('xhs-first', 1, first);
  assert.equal(material.format_version, 2); assert.equal(material.native_followers, 9403);
  assert.equal(material.native_content ?? null, null);
  assert.equal(containsDiscoveryCitation(material, { kind: 'native_profile', profile_url: profile.profile_url, quote: profile.bio }), true);
  const bodyClaim = repository.claimContent('xhs-first', 1, first);
  repository.finishContent(bodyClaim, content, null);
  const complete = repository.prepareReviewMaterial('xhs-first', 1, first);
  assert.equal((complete.native_content!.native_content as { author_id: string }).author_id, profile.author_id);
  const review = repository.submitReview('captured-reviewer', {
    campaign_id: 'xhs-first', expected_revision: 1, search_hit_id: first, material_hash: complete.material_hash,
    request_id: 'captured-xhs-selection-quality', reviewer: { kind: 'agent', label: SELECTION_QUALITY_REVIEWER },
    findings: [
      { criterion: 'selection_fit', outcome: 'supported', rationale: '已保存的作者绑定笔记介绍LangChain智能体，原生粉丝9403达到2000门槛。', citations: [{ kind: 'native_content', quote: content.text }] },
      { criterion: 'quality_rating', outcome: 'supported', match_level: 'good', rationale: '已保存笔记提供相关智能体内容，足以支持主题匹配和质量判断。', citations: [{ kind: 'native_content', quote: content.text }] },
    ],
  });
  assert.equal(review.verdict, 'qualified');
  assert.equal(review.subscribers, 9403);
  assert.deepEqual(review.reasons, []);
  assert.equal(repository.listCandidates({ campaign_id: 'xhs-first', after_id: null, limit: 10 })[0]!.verdict, 'qualified');
  const { discoveryContactSchema } = await import('../../onecrew_lite-library/dist/modules/tables/discovery-projection.js');
  const contacts = repository.readLibraryMaterial('xhs-first', 1).sources.rows.flatMap(row => row.contacts ?? []);
  assert.ok(contacts.some(contact => contact.kind === 'social_profile' && contact.normalized_value === profile.profile_url));
  for (const contact of contacts) discoveryContactSchema.parse({ ...contact, captured_at: Number(contact.captured_at) });
  const original = repository.getProfile('xhs-first', first), next = add('xhs-reuse', 10000);
  const reused = repository.reuseProfile(repository.claimProfile('xhs-reuse', 1, next));
  assert.ok(reused); assert.equal(reused.captured_at, original.captured_at); assert.deepEqual(reused.native_profile, original.native_profile);
  assert.equal(repository.nextProfileHit('xhs-reuse'), null);
  assert.equal(db.prepare('SELECT verdict FROM discovery_candidates WHERE discovery_campaign_id=?').get('xhs-reuse')!.verdict, 'rejected');
  assert.deepEqual(repository.getProfile('xhs-first', first), original);
});

test('96升级97保留真实任务、候选和成功读取的全部历史行', async t => {
  const directory = await mkdtemp(fileURLToPath(new URL('../../.scratch/xiaohongshu-upgrade-', import.meta.url)));
  const source = join(directory, 'source.sqlite'), staging = join(directory, 'staging', 'creator.sqlite');
  await copyFile(new URL('./fixtures/discovery-content-hydration/bilibili-after.sqlite', import.meta.url), source);
  await backupLegacyDatabase(source, staging, 'creator');
  await upgradeCreatorStagingCopy({ sourcePath: source, stagingPath: staging });
  const current = new DatabaseSync(staging);
  current.exec('PRAGMA foreign_keys=OFF; BEGIN IMMEDIATE'); migrateCreatorReceiptPlatformEdge(current, 96, true); current.exec('PRAGMA user_version=96; COMMIT');
  validateCreatorSchemaVersion(current, 96); current.close();
  const predecessor = join(directory, 'version96.sqlite'); await copyFile(staging, predecessor);
  const snapshot = (db: DatabaseSync) => Object.fromEntries(db.prepare("SELECT name FROM sqlite_schema WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name").all().map(row => {
    const name = String(row.name), query = db.prepare(`SELECT * FROM "${name}"`); query.setReadBigInts(true);
    return [name, query.all().map(row => stringifyApplicationJson(row)).sort()];
  }));
  const before = new DatabaseSync(predecessor, { readOnly: true }); const expected = snapshot(before); before.close();
  assert.deepEqual(await upgradeCreatorStagingCopy({ sourcePath: predecessor, stagingPath: staging }), { fromVersion: 96, toVersion: 97 });
  const after = new DatabaseSync(staging); t.after(() => after.close()); validateCreatorSchemaVersion(after, 97);
  assert.deepEqual(snapshot(after), expected); assert.deepEqual(after.prepare('PRAGMA foreign_key_check').all(), []);
});
