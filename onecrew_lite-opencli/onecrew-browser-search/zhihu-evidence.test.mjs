import test from 'node:test';
import assert from 'node:assert/strict';
import {projectProfile, projectAnswer, projectArticle, projectSearch, projectComments, originalHtml, readEvidence, fetchNative} from './zhihu-evidence.mjs';
import {runInNewContext} from 'node:vm';

const author = {id: 'a'.repeat(32), url_token: 'creator-123', name: 'Creator'};
const answer = {id: '123', type: 'answer', author, question: {id: '456', title: 'Keyboard'},
  content: '<p>键盘介绍</p><a href="mailto:business@example.com">联系邮箱</a>'};

test('comments take the original first five once, without sixth-comment refill', () => {
  const data = Array.from({length: 6}, (_, i) => ({id: String(i+1), content: i < 5 ? '<p>English</p>' : '<p>中文评论</p>'}));
  const rows = projectComments({data});
  assert.equal(rows.length, 5);
  assert.ok(rows.every(row => row.text === 'English'));
  assert.throws(() => projectComments({data: [data[0], data[0]]}));
  assert.throws(() => projectComments({data: [{id: '1', content: null}]}));
});

test('article carries native author, original HTML and exact article source', () => {
  const article = {id: '789', type: 'article', author, title: 'Review', content: '<p>Contact: creator@example.com</p>'};
  const row = projectArticle(article, '789')[0];
  assert.equal(row.source_url, 'https://zhuanlan.zhihu.com/p/789');
  assert.equal(row.author_id, author.id);
  assert.match(row.text, /creator@example.com/);
  assert.throws(() => projectArticle(article, '790'));
  assert.throws(() => projectArticle({...article, type: 'answer'}, '789'));
  const hits = projectSearch({data: [{type: 'search_result', object: article}]}, 1);
  assert.equal(hits[0].source_url, row.source_url);
  assert.equal(hits[0].article_id, '789');
});

test('profile preserves native identity, actual zero and unknown followers', () => {
  const row = projectProfile({...author, headline: '合作 contact@example.com'}, author.url_token)[0];
  assert.equal(row.author_id, author.id);
  assert.equal(row.follower_count, null);
  assert.match(row.bio, /contact@example.com/);
  assert.equal(projectProfile({...author, follower_count: 0}, author.url_token)[0].follower_count, 0);
  assert.throws(() => projectProfile({...author, follower_count: -1}, author.url_token));
  assert.throws(() => projectProfile(author, 'other'));
  assert.throws(() => projectProfile({...author, is_anonymous: true}, author.url_token));
  assert.throws(() => projectProfile({...author, id: 'Creator'}, author.url_token));
});
test('answer binds native question and answer IDs and keeps original contact link', () => {
  const row = projectAnswer(answer, '123')[0];
  assert.equal(row.source_url, 'https://www.zhihu.com/question/456/answer/123');
  assert.equal(row.author_id, author.id);
  assert.match(row.text, /键盘介绍/);
  assert.deepEqual(row.public_links, ['mailto:business@example.com']);
  assert.throws(() => projectAnswer(answer, '999'));
  assert.throws(() => projectAnswer({...answer, id: Number.MAX_SAFE_INTEGER + 1}, '123'));
  assert.throws(() => projectAnswer({...answer, type: 'question'}, '123'));
});
test('search does not promote aggregate or anonymous results into author identities', () => {
  const data = [{type: 'search_result', object: {type: 'question'}},
    {type: 'search_result', object: {...answer, author: {...author, is_anonymous: true}}},
    {type: 'search_result', object: answer}];
  assert.equal(projectSearch({data}, 2).length, 0); // do not refill beyond sample
  assert.equal(projectSearch({data}, 3)[0].author_id, author.id);
  assert.throws(() => projectSearch({data: [{type: 'search_result', object: {...answer, author: {name: 'Creator'}}}]}, 1));
});
test('HTML projection decodes text, strips scripts, bounds bytes, excludes credential links', () => {
  const row = originalHtml('<script>secret@example.com</script><p>&#x65e5;&amp;本</p>'
    + '<a href="https://user:secret@example.com/contact">bad</a>'
    + '<a href="https://example.com/contact">good</a>' + '中'.repeat(6000));
  assert.match(row.text, /日&本/);
  assert.doesNotMatch(row.text, /secret@example/);
  assert.ok(Buffer.byteLength(row.text) <= 16000);
  assert.deepEqual(row.public_links, ['https://example.com/contact']);
});
test('owned transport validates input first and stops risk without retry', async () => {
  let reads = 0, navigations = 0;
  const page = {goto: async url => { assert.equal(url, 'https://www.zhihu.com/'); navigations++; },
    evaluate: async script => { reads++; assert.match(script, /api\/v4\/answers\/123/); return {stop: 'risk'}; }};
  const own = async (p, callback) => callback(p);
  await assert.rejects(readEvidence(page, {id: '../123'}, 'answer', own), /INPUT/);
  assert.equal(reads, 0); assert.equal(navigations, 0);
  await assert.rejects(readEvidence(page, {id: '123'}, 'answer', own), /RISK/);
  assert.equal(reads, 1); assert.equal(navigations, 1);
});

test('403 and 429 retain only the HTTP diagnostic and never read the blocked body', async () => {
  for (const status of [403, 429]) {
    let requests = 0;
    const response = await runInNewContext(`(${fetchNative.toString()})('/api/v4/articles/123')`, {
      location: {origin: 'https://www.zhihu.com'}, AbortSignal,
      fetch: async () => { requests++; return {status, get body() { throw new Error('must not read'); }}; },
    });
    assert.equal(response.stop, 'risk');
    assert.equal(response.http_status, status);
    const page = {goto: async () => {}, evaluate: async () => response};
    await assert.rejects(readEvidence(page, {id: '123'}, 'article', async (p, cb) => cb(p)),
      error => error.message === 'ONECREW_ZHIHU_RISK' && error.httpStatus === status);
    assert.equal(requests, 1);
  }
});
