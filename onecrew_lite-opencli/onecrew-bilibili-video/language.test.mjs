import test from 'node:test';
import assert from 'node:assert/strict';
import { runInNewContext } from 'node:vm';
import { projectBilibiliLanguage, projectBilibiliRecent, readBilibiliLanguage, readBilibiliApi } from './language.mjs';
import { projectPageLanguage } from './language-page.mjs';
const view = { bvid: 'BV1234567890', aid: 123, owner: { mid: 42 }, title: '原帖标题', desc: 'original text' };
const reply = { rpid_str: '9876543210123456789', member: { mid: '51' }, content: { message: '中文评论' } };

test('HTML HTTP risk stops remain risk errors instead of permitting page fallback', async () => {
  for (const status of [412, 429]) {
    const page = { evaluate: script => runInNewContext(`(${script})()`, {
      fetch: async () => new Response('<!DOCTYPE html><title>出错啦!</title>', { status }),
    }) };
    await assert.rejects(readBilibiliApi(page, '/x/space/wbi/arc/search'), /CAPTCHA or rate limit/);
  }
});

test('API transport keeps auth, provider failure and valid JSON distinct', async () => {
  for (const [status, expected] of [[401, /AUTH_REQUIRED/], [403, /HTTP 403/], [503, /HTTP 503/]]) {
    const page = { evaluate: script => runInNewContext(`(${script})()`, {
      fetch: async () => new Response('<html>Unavailable</html>', { status }),
    }) };
    await assert.rejects(readBilibiliApi(page, '/x/web-interface/view'), expected);
  }
  let observedUrl;
  let signatures = 0;
  const page = { evaluate: script => runInNewContext(`(${script})()`, {
    fetch: async url => { observedUrl = new URL(url); return Response.json({code: 0, data: {replies: null}}); },
  }) };
  const data = await readBilibiliApi(page, '/x/v2/reply/main', {signed: true, params: {oid: 123}}, async (_page, params) => {
    signatures++;
    return {...params, w_rid: 'fixture-signature'};
  });
  assert.equal(signatures, 1);
  assert.equal(observedUrl.searchParams.get('oid'), '123');
  assert.equal(observedUrl.searchParams.get('w_rid'), 'fixture-signature');
  assert.equal(data.code, 0);
  assert.equal(data.data.replies, null);
});
test('public-page comments retain provenance and deduplicate text without commenter IDs', () => {
  const rows = Object.entries({bvid:view.bvid,author:'Runner (mid: 42)',title:'Original',description:'Body'}).map(([field,value]) => ({field,value}));
  const sample = projectPageLanguage(rows, [{author_id:'51',text:'中文'},{author_id:'52',text:'中文'},{author_id:'51',text:'中文'}], '42', view.bvid);
  assert.equal(sample.comments.length, 1);
  assert.ok(sample.comments.every(c => c.source_url === sample.source_url));
  assert.throws(() => projectPageLanguage(rows, [], '43', view.bvid));
});
test('original text, exact author and stable comment IDs stay separate', () => {
  const result = projectBilibiliLanguage(view, [reply], '42', view.bvid);
  assert.equal(result.author_id, '42');
  assert.equal(result.text, '原帖标题\noriginal text');
  assert.ok(!Object.hasOwn(result.comments[0], 'author_id'));
  assert.equal(result.comments[0].source_url, 'https://www.bilibili.com/video/BV1234567890#reply9876543210123456789');
});
test('reject wrong author, ambiguous numeric IDs and duplicate comments', () => {
  assert.throws(() => projectBilibiliLanguage(view, [], '43', view.bvid));
  assert.throws(() => projectBilibiliLanguage(view, [reply, reply], '42', view.bvid));
  assert.throws(() => projectBilibiliLanguage(view, [{...reply, rpid_str: undefined, rpid: 9876543210123456789}], '42', view.bvid));
});
test('recent locators are bounded and native author bound', () => {
  const videos = Array.from({length: 4}, (_, i) => ({ bvid: `BV123456789${i}`, mid: 42 }));
  assert.equal(projectBilibiliRecent({list: {vlist: videos}}, '42').length, 3);
  assert.throws(() => projectBilibiliRecent({list: {vlist: [{...videos[0], mid: 43}]}}, '42'));
});
test('valid zero comments is evidence; missing reply field is not', async () => {
  const page = { goto: async () => {} };
  const own = (p, f) => f(p);
  const api = async (_p, path) => path.includes('/view') ? view : {replies: null};
  const [result] = await readBilibiliLanguage(page, '42', view.bvid, api, own, x => x);
  assert.deepEqual(result.comments, []);
  await assert.rejects(readBilibiliLanguage(page, '42', view.bvid,
    async (_p, path) => path.includes('/view') ? view : {}, own, x => x));
});
test('wrong author stops before reading comments', async () => {
  let calls = 0;
  await assert.rejects(readBilibiliLanguage({goto: async () => {}}, '43', view.bvid,
    async () => { calls++; return view; }, (p, f) => f(p), x => x));
  assert.equal(calls, 1);
});

test('matched-post language reading requests and returns at most five comments', async () => {
  const replies = Array.from({length: 6}, (_, i) => ({rpid_str: String(100 + i), content: {message: `原文评论${i}`}}));
  let requested;
  const [sample] = await readBilibiliLanguage({goto: async () => {}}, '42', view.bvid,
    async (_page, path, options) => {
      if (path.includes('/view')) return view;
      requested = options.params.ps;
      return {replies};
    }, (p, f) => f(p), x => x);
  assert.equal(requested, 5);
  assert.equal(sample.comments.length, 5);
  assert.ok(!sample.comments.some(c => c.text === '原文评论5'));
  const rows = Object.entries({bvid:view.bvid, author:'Runner (mid: 42)', title:'Original', description:'Body'}).map(([field,value]) => ({field,value}));
  assert.throws(() => projectPageLanguage(rows, replies.map(r => ({text:r.content.message})), '42', view.bvid));
});
