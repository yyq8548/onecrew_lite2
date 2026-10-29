import test from 'node:test';
import assert from 'node:assert/strict';
import { boundedCommentArgs, commentReaders, readComments, projectWeiboComments, fetchWeiboComments } from './comments.mjs';
test('comment reads cannot expand arbitrary nested trees', () => {
  assert.equal(boundedCommentArgs('bilibili', {}).limit, 20);
  assert.ok(commentReaders.every(([site]) => site !== 'zhihu'));
  assert.throws(() => boundedCommentArgs('weibo', {limit: 21}));
});
test('write commands rejected before browser access', async () => {
  await assert.rejects(readComments({}, {}, 'bilibili', {name: 'comments', access: 'write', func() {}},
    () => assert.fail('must not access browser')));
});
test('native reader receives only the owned original-language page', async () => {
  const calls = [];
  const owned = {goto: async url => calls.push(url)};
  const result = await readComments({}, {limit: 1}, 'bilibili', {
    name: 'comments', access: 'read', domain: 'www.bilibili.com',
    func: async (p, a) => { assert.equal(p, owned); assert.equal(a.limit, 1); return [{text:'原文'}]; },
  }, (_p, f) => f(owned));
  assert.deepEqual(calls, ['https://www.bilibili.com/']);
  assert.deepEqual(result, [{text:'原文'}]);
});

const weiboComment = (overrides = {}) => ({
  idstr: '5091234567891234', user: {idstr: '1234567890', screen_name: '同名'},
  text: '<span>原文 &amp; original</span>', like_count: 2, total_number: 0,
  created_at: 'Mon Oct 05 00:00:00 +0800 2026', ...overrides,
});

test('Weibo reads once and keeps original text and native identities', async () => {
  const calls = [];
  const page = {goto: async url => calls.push(url), evaluate: async code => {
    calls.push(code);
    return {session: {}, data: {ok: 1, data: [weiboComment()]}};
  }};
  const rows = await readComments({}, {id: '5090000000000000', limit: 1}, 'weibo', {
    name: 'comments', access: 'read', domain: 'weibo.com',
    func: () => assert.fail('must not issue a second display-only read'),
  }, (_page, run) => run(page));
  assert.equal(calls.length, 2);
  assert.equal(calls[0], 'https://weibo.com/');
  assert.equal(rows[0].id, '5091234567891234');
  assert.equal(rows[0].native_author_id, '1234567890');
  assert.equal(rows[0].text, '原文 & original');
  assert.equal(rows[0].text_truncated, false);
});

test('Weibo rejects unsafe, duplicate or conflicting native IDs', () => {
  const project = data => projectWeiboComments({ok: 1, data}, 20);
  assert.throws(() => project([weiboComment({idstr: undefined, id: Number.MAX_SAFE_INTEGER + 1})]), /INVALID_ID/);
  assert.throws(() => project([weiboComment({id: '123'})]), /ID_CONFLICT/);
  assert.throws(() => project([weiboComment(), weiboComment()]), /INVALID_ID/);
  assert.equal(project([weiboComment({id: Number.MAX_SAFE_INTEGER + 1})])[0].id, '5091234567891234');
});

test('Weibo keeps missing and anonymous authors unknown, and never invents counts', () => {
  for (const user of [undefined, {screen_name: '同名'}, {id: 0}, {idstr: '123', is_anonymous: true}]) {
    const row = projectWeiboComments({ok: true, data: [weiboComment({user, like_count: undefined})]}, 1)[0];
    assert.equal(row.native_author_id, null);
    assert.equal(row.likes, null);
  }
  const row = projectWeiboComments({ok: 1, data: [weiboComment({text_raw: '中'.repeat(2001)})]}, 1)[0];
  assert.equal(row.text.length, 2000);
  assert.equal(row.text_truncated, true);
});

test('Weibo rejects oversized pages and distinguishes failure from valid empty results', () => {
  assert.deepEqual(projectWeiboComments({ok: 1, data: []}, 1), []);
  for (const payload of [{ok: 0, data: []}, {ok: 1}, {ok: 1, data: Array(21).fill(weiboComment())}])
    assert.throws(() => projectWeiboComments(payload, 1), /INVALID/);
  assert.throws(() => projectWeiboComments({stop: 'auth'}, 1), /AUTH/);
  assert.throws(() => projectWeiboComments({stop: 'risk'}, 1), /RISK/);
});

test('Weibo fixed 20-row page is capped to the requested sample without pagination', () => {
  const data = Array.from({length: 20}, (_, index) => weiboComment({idstr: String(index + 1)}));
  const rows = projectWeiboComments({ok: 1, data}, 5);
  assert.equal(rows.length, 5);
  assert.deepEqual(rows.map(row => row.id), ['1', '2', '3', '4', '5']);
});

test('Weibo invalid source never opens browser', async () => {
  for (const id of ['https://weibo.com/123/a', '1&count=100', '0'])
    await assert.rejects(readComments({}, {id}, 'weibo', {
      name: 'comments', access: 'read', domain: 'weibo.com', func() {},
    }, () => assert.fail('must validate before browser')), /INPUT/);
});

test('Weibo API redirects, auth and rate limits cannot become empty comments', async t => {
  const previousLocation = Object.getOwnPropertyDescriptor(globalThis, 'location');
  Object.defineProperty(globalThis, 'location', {value: {origin: 'https://weibo.com'}, configurable: true});
  t.after(() => previousLocation ? Object.defineProperty(globalThis, 'location', previousLocation) : delete globalThis.location);
  let response = {url: 'https://weibo.com/ajax/statuses/buildComments', status: 200, ok: true,
    json: async () => ({ok: 1, data: []})};
  t.mock.method(globalThis, 'fetch', async (url, options) => {
    assert.equal(url, '/ajax/statuses/buildComments?flow=0&is_reload=1&id=123&is_show_bulletin=2&is_mix=0&count=2');
    assert.equal(options.credentials, 'include');
    return response;
  });
  assert.deepEqual(await fetchWeiboComments('123', 2), {ok: 1, data: []});
  for (const [status, stop] of [[401, 'auth'], [403, 'risk'], [429, 'risk'], [500, 'read']]) {
    response = {...response, status, ok: false};
    assert.deepEqual(await fetchWeiboComments('123', 2), {stop});
  }
  response = {...response, url: 'https://passport.weibo.com/login'};
  assert.deepEqual(await fetchWeiboComments('123', 2), {stop: 'auth'});
  response = {...response, url: 'https://example.com/'};
  assert.deepEqual(await fetchWeiboComments('123', 2), {stop: 'read'});
});
