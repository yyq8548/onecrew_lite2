import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { test } from 'node:test';
import { load } from 'cheerio';
import { validateHistoricalDiscoveryContent, validateHistoricalDiscoveryProfile } from '../src/discovery/historical-native-material.ts';
import { CREATOR_SEARCH_SOURCES, RETIRED_CREATOR_SOURCES, creatorSearchArguments } from '../src/platforms/opencli/sources.ts';
import { creatorProfileSource } from '../src/platforms/opencli/profile-source.ts';
import { discoveryPromotional } from '../src/discovery/contact-attribution.ts';

const captured = JSON.parse(await readFile(new URL('./fixtures/zhihu-answer-live-20261026.json', import.meta.url), 'utf8'));
const data = captured.body, author = data.author.id;
const source = `https://www.zhihu.com/question/${data.question.id}/answer/${data.id}`;
const regression = JSON.parse(await readFile(new URL('./fixtures/zhihu-profile-contact-live-20261029.json', import.meta.url), 'utf8'));

test('知乎不再注册搜索或主页读取，已有任务来源保留为退役标识', () => {
  assert.equal(Object.hasOwn(CREATOR_SEARCH_SOURCES, 'zhihu'), false);
  assert.ok(RETIRED_CREATOR_SOURCES.includes('zhihu'));
  assert.throws(() => creatorSearchArguments('d5pskwub', 'zhihu', 'AI Agent', 10), { code: 'opencli_read_command_unavailable' });
  assert.throws(() => creatorProfileSource('zhihu', author, regression.profile.author_url), { code: 'opencli_read_command_unavailable' });
});

test('已保存的真实知乎回答仍校验作者、长整数ID和完整正文', () => {
  const native = {
    author_id: author, url_token: data.author.url_token, author_label: data.author.name,
    author_url: `https://www.zhihu.com/people/${data.author.url_token}`,
    source_url: source, answer_id: data.id, question_id: data.question.id,
    text: load(data.content).text().trim(), public_links: [],
  };
  const decoded = validateHistoricalDiscoveryContent('zhihu', author, source, native);
  assert.equal(decoded.answer_id, '2073192470771414849');
  assert.match(String(decoded.text), /Agent/);
  assert.throws(() => validateHistoricalDiscoveryContent('zhihu', '0'.repeat(32), source, native));
  assert.throws(() => validateHistoricalDiscoveryContent('zhihu', author, source, { ...native, question_id: '1' }));
});

test('已保存主页完整性和旧联系来源判断保持不变', () => {
  const profile = regression.profile;
  assert.deepEqual(validateHistoricalDiscoveryProfile('zhihu', profile.author_id, profile.author_url, profile), profile);
  assert.equal(profile.bio_status, 'complete');
  assert.equal(discoveryPromotional(regression.contact.original_value, regression.contact.source_excerpt), true);
});
