import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import * as XLSX from 'xlsx';
import { buildCreatorWorkbook } from '../../src/modules/tables/files.js';
import type { DiscoveryCreatorView, TableDto } from '../../src/modules/tables/data.js';
import { recommendationCapture } from '../../src/modules/tables/recommendation-capture.js';
import { contactDisplay, isSocialProfile } from '@onecrew/contracts/tables';

const table: TableDto = {
  id: 'table',
  name: '测试',
  columns: [
    { id: 'score', name: 'Match evaluation', data_type: 'score' },
    { id: 'category', name: 'Content category', data_type: 'string' },
  ],
};
const member = {
  candidate_id: 'author',
  platform: 'twitter',
  native_author_id: '123',
  display_name: '作者',
  profile_url: 'https://x.com/author',
  source_hit_ids: ['hit'],
};
const contact = {
  kind: 'email' as const,
  original_value: 'business@example.com',
  normalized_value: 'business@example.com',
  native_author_id: '123',
  source_url: 'https://x.com/author',
  source_excerpt: 'business@example.com',
  captured_at: 1,
  attribution: 'author_published' as const,
};
const material: DiscoveryCreatorView = {
  state: 'current',
  projection: {
    table_id: 'table',
    campaign_id: 'campaign',
    projection_format: 1,
    criteria_revision: 1,
    source_revision: 1,
    source_hash: 'a'.repeat(64),
    identity_revision: 1,
    identity_hash: 'b'.repeat(64),
    source_rows: [
      {
        search_hit_id: 'hit',
        candidate_id: 'author',
        native_author_id: '123',
        evidence_version: 'c'.repeat(64),
      },
    ],
    groups: [
      {
        group_id: 'd'.repeat(64),
        has_same_author_review: false,
        decision_ids: [],
        members: [member],
      },
    ],
  },
  sources: [
    {
      search_hit_id: 'hit',
      verdict: 'qualified',
      reasons: [],
      contacts: [contact],
      row: {
        id: 'row',
        values: { score: { level: 'medium', reason: '已通过' }, category: '科技' },
      },
    },
  ],
};
assert.equal(contactDisplay({...contact, kind:'social_profile', normalized_value:'https://www.tiktok.com/@creator'}),
  'TikTok 私信：https://www.tiktok.com/@creator');
const read = (value: DiscoveryCreatorView) => {
  const workbook = XLSX.read(buildCreatorWorkbook(table, value, 'zh-CN'), { type: 'array' });
  assert.deepEqual(workbook.SheetNames, ['推荐']);
  return XLSX.utils.sheet_to_json(workbook.Sheets['推荐'], { header: 1 });
};
assert.deepEqual(read(material), [
  ['名字', '类别', '平台', '评分', '联系方式'],
  ['作者', '科技', 'X', '一般', 'business@example.com'],
]);
const exported = XLSX.read(buildCreatorWorkbook(table, material, 'zh-CN'), {type: 'array'}).Sheets['推荐'];
const followerTable = { ...table, columns: [...table.columns, { id: 'followers', name: 'Followers', data_type: 'string' as const }] };
for (const [value, expected] of [['4029', '4029'], ['~1万', '约 1万'], [null, '未知']] as const) {
  const counted = structuredClone(material);
  counted.sources[0].row.values.followers = value;
  const sheet = XLSX.read(buildCreatorWorkbook(followerTable, counted, 'zh-CN'), { type: 'array' }).Sheets['推荐'];
  assert.equal(sheet.F1.v, '粉丝量');
  assert.equal(sheet.F2.v, `X · 作者: ${expected}`);
}
assert.equal(exported.A2.l?.Target, member.profile_url);
assert.equal(exported.D2.c?.[0].t, '已通过');
for (const source of [
  { ...material.sources[0], contacts: [] },
  { ...material.sources[0], verdict: 'rejected' },
  { ...material.sources[0], verdict: 'pending' },
])
  assert.equal(read({ ...material, sources: [source] }).length, 1);
assert.throws(() => read({ ...material, state: 'missing', projection: null }));
console.log('creator export: five columns, contact gate and owner snapshot passed');

const channel = 'UCabcdefghijklmnopqrstuv';
const homepage = `https://www.youtube.com/channel/${channel}`;
const hidden = {
  ...contact,
  kind: 'hidden_email' as const,
  original_value: '有隐藏邮箱，需手动查看',
  normalized_value: homepage,
  source_url: homepage,
  native_author_id: channel,
};
const social = {
  ...contact,
  kind: 'social_profile' as const,
  original_value: 'https://www.instagram.com/creator',
  normalized_value: 'https://www.instagram.com/creator',
};
const withContacts = (contacts: (typeof material.sources)[0]['contacts']) => ({
  ...material,
  sources: [{ ...material.sources[0], contacts }],
});
assert.equal(
  (read(withContacts([hidden, social]))[1] as unknown[])[4],
  `隐藏邮箱，在更多信息手动查看 — ${homepage}\n${social.normalized_value}`,
);
assert.equal(
  (read(withContacts([hidden, social, contact]))[1] as unknown[])[4],
  `${social.normalized_value}\nbusiness@example.com`,
);
assert.equal((read(withContacts([social]))[1] as unknown[])[4], social.normalized_value);
const { discoveryContactSchema } = await import('../../src/modules/tables/discovery-projection.js');
assert.equal(discoveryContactSchema.safeParse(hidden).success, true);
assert.equal(
  discoveryContactSchema.safeParse({ ...hidden, native_author_id: 'UCotherotherotherotherxx' })
    .success,
  false,
);
assert.equal(
  discoveryContactSchema.safeParse({ ...social, normalized_value: 'https://x.com/search' }).success,
  false,
);
console.log('hidden email + account fallback: export priority and author-bound schema passed');

const wechat = {...contact, kind:'wechat' as const, original_value:'微信：cherrymiaojiang（备注来历）', normalized_value:'cherrymiaojiang', source_excerpt:'微信：cherrymiaojiang（备注来历）'};
assert.equal((read(withContacts([wechat]))[1] as unknown[])[4], '微信：cherrymiaojiang（备注来历）');
assert.equal(discoveryContactSchema.safeParse(wechat).success, true);
assert.equal(discoveryContactSchema.safeParse({...wechat, attribution:'unconfirmed'}).success, false);
const metadata = {version_id:'d9a351fd-94f5-48b4-ae80-df503d41f10b', source_instance_id:'cd7d4b88-d3c6-47ab-b901-e4eaf164bb4e', consent_id:'5ec2446c-0b80-4b7c-a5c5-503c077390de', capture_started_at:'2026-10-09T00:00:00.000Z'};
const capture = recommendationCapture(table, withContacts([wechat, social]), metadata);
const events = capture.bytes.toString('utf8').trim().split('\n').map(line => JSON.parse(line));
assert.equal(capture.record_count, 1);
assert.deepEqual(events[1].fields.map((f: {name:string}) => f.name), ['名字','类别','平台','评分','联系方式']);
assert.deepEqual(events[2].values.score, {level:'medium',reason:'已通过'});
assert.equal(events[2].values.name.url, member.profile_url);
assert.equal(events[2].values.contacts, `微信：cherrymiaojiang（备注来历）\n${social.normalized_value}`);
assert.equal(recommendationCapture(table, withContacts([wechat, social]), metadata).bytes.equals(capture.bytes), true);
const emptyCapture = recommendationCapture(table, withContacts([]), metadata);
assert.equal(emptyCapture.record_count, 0);
assert.equal(emptyCapture.bytes.toString().trim().split('\n').length, 2);
if (process.env.ONECREW_UPLOAD_CONTRACT_MODULE) {
  const {captureEventSchema} = await import(process.env.ONECREW_UPLOAD_CONTRACT_MODULE);
  for (const event of [...events, ...emptyCapture.bytes.toString().trim().split('\n').map(line => JSON.parse(line))]) captureEventSchema.parse(event);
  const {beginUploadSchema, completeUploadSchema} = await import(process.env.ONECREW_UPLOAD_CONTRACT_MODULE.replace('capture.ts','uploads.ts'));
  const header = events[0];
  const input = {version_id:header.version_id,consent_id:header.consent_id,local_source_id:metadata.source_instance_id,table_id:header.table_id,table_name:header.table_name,capture_seq:'1',capture_kind:'full_sample',capture_started_at:header.capture_started_at,previous_version_id:null,scopes:['dynamic_table_records','dynamic_table_schema'],format:header.format,format_version:1};
  assert.deepEqual(beginUploadSchema.parse(input),input,'backend normalization must not alter the persisted begin input');
  const completion={attempt_id:metadata.version_id,capture_finished_at:metadata.capture_started_at,record_count:capture.record_count,schema_count:capture.schema_count,parts:[{part_number:1,bytes:capture.bytes.length,etag:'opaque-test-etag',md5:createHash('md5').update(capture.bytes).digest('hex')}]};
  assert.deepEqual(completeUploadSchema.parse(completion),completion);
  console.log('actual admin JSONL v1 contract: passed');
}
console.log('messaging labels, remarks and deterministic full snapshot: passed');

for (const url of ['https://www.facebook.com/profile.php?id=100044572203205', 'https://www.facebook.com/creator', homepage]) {
  assert.equal(isSocialProfile(url), true);
  const route = { ...social, original_value: url, normalized_value: url };
  assert.equal((read(withContacts([route]))[1] as unknown[])[4], contactDisplay(route));
  assert.equal(read(withContacts([route])).length, 2);
}
assert.equal(isSocialProfile('https://www.facebook.com/profile.php?id=123&sk=about'), false);
assert.equal(isSocialProfile('https://www.youtube.com/watch?v=example'), false);

const changedTime = { ...metadata, capture_started_at: '2026-10-09T01:00:00.000Z' };
assert.equal(recommendationCapture(table, material, metadata).content_hash,
  recommendationCapture(table, material, changedTime).content_hash);
assert.notEqual(recommendationCapture(table, material, metadata).content_hash,
  recommendationCapture({ ...table, name: '改名' }, material, metadata).content_hash);
assert.notEqual(recommendationCapture(table, material, metadata).content_hash,
  recommendationCapture(table, { ...material, projection: { ...material.projection!, criteria_revision: 2 } }, metadata).content_hash);
console.log('snapshot change detection: time stable, table rename and criteria revision tracked');

const baseHash = recommendationCapture(table, material, metadata).content_hash;
const revised = structuredClone(material);
const evaluation = revised.sources[0].row.values.score as {level:string;reason:string};
evaluation.reason += ' updated';
assert.notEqual(recommendationCapture(table, revised, metadata).content_hash,baseHash);
assert.notEqual(recommendationCapture(table, withContacts([]), metadata).content_hash,baseHash);
assert.notEqual(recommendationCapture(table, withContacts([wechat]), metadata).content_hash,baseHash);
console.log('snapshot changes: full reason, removal/empty result and contact updates passed');
