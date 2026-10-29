import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { test } from 'node:test';
import { load } from 'cheerio';
import { zhihuReadPlan } from '../src/platforms/opencli/zhihu.ts';
import { classifyOpenCliFailure } from '../src/platforms/opencli/process.ts';
import { decodeCreatorSearch } from '../src/platforms/opencli/search-results.ts';
import { creatorProfileSource } from '../src/platforms/opencli/profile-source.ts';
// @ts-expect-error 浏览器适配器是运行时 JavaScript 模块。
import { zhihuIdentity, zhihuProfile, zhihuContent, zhihuSearchRows } from '../../onecrew_lite-opencli/onecrew-zhihu/read.mjs';

const captured = JSON.parse(await readFile(new URL('./fixtures/zhihu-answer-live-20261026.json', import.meta.url), 'utf8'));
const data = captured.body, author = data.author.id;
const source = `https://www.zhihu.com/question/${data.question.id}/answer/${data.id}`;

test('真实知乎回答保留原生作者、长整数ID和完整正文，原证据校验可用', () => {
  const $ = load(data.content);
  const native = zhihuContent({ ...data, plain_text: $.text().trim(), public_links: [] }, source, author);
  const decoded = zhihuReadPlan('content', author, source).decode([native]);
  assert.equal(decoded.author_id, author);
  assert.equal(decoded.answer_id, '2073192470771414849');
  assert.equal(decoded.author_url, 'https://www.zhihu.com/people/10-27-52-1-65');
  assert.match(String(decoded.text), /Agent/);
  assert.throws(() => zhihuReadPlan('content', '0'.repeat(32), source).decode([native]), { code: 'opencli_invalid_output' });
  assert.throws(() => zhihuReadPlan('content', author, source).decode([{ ...native, question_id: '1' }]), { code: 'opencli_invalid_output' });
});

test('真实作者材料缺少粉丝数时保持未知，主页仍通过身份校验', () => {
  const profile = zhihuProfile(data.author, author, data.author.url_token);
  assert.equal(profile.follower_count, null);
  assert.equal(zhihuReadPlan('profile', author, profile.author_url).decode([profile]).follower_count, null);
  assert.equal(creatorProfileSource('zhihu', author, profile.author_url), data.author.url_token);
  assert.throws(() => zhihuProfile(data.author, '0'.repeat(32), data.author.url_token));
});

test('知乎搜索作者与主页绑定进入现有搜索解码', () => {
  const identity = zhihuIdentity(data.author);
  const row = { ...identity, author: data.author.name, title: data.question.title, url: source };
  assert.equal(decodeCreatorSearch('zhihu', [row], 1)[0]!.author_id, author);
  assert.throws(() => decodeCreatorSearch('zhihu', [{ ...row, author_url: 'https://www.zhihu.com/people/other' }], 1));
  assert.deepEqual(zhihuSearchRows({ data: [] }, 20), []);
});

test('知乎403保留阶段和具体原因，区别登录错误与限流', () => {
  const message = 'ONECREW_ZHIHU_HTTP stage=content HTTP 403 code=10003 message=Forbidden';
  const failure = classifyOpenCliFailure(1, Buffer.alloc(0), Buffer.from(message));
  assert.equal(failure.code, 'provider_response_http_error');
  assert.ok(JSON.stringify(failure).includes('10003'));
  assert.equal(classifyOpenCliFailure(77, Buffer.alloc(0), Buffer.from('AUTH_REQUIRED HTTP 401')).code, 'opencli_auth_required');
  assert.equal(classifyOpenCliFailure(1, Buffer.alloc(0), Buffer.from('ONECREW_ZHIHU_HTTP stage=search HTTP 429')).code, 'opencli_risk_control');
});
