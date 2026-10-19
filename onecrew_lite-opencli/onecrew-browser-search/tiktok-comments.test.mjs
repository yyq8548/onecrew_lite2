import test from 'node:test';
import assert from 'node:assert/strict';
import {projectTikTokPost, projectTikTokComments, readTikTokComments, readTikTokCommentPage} from './tiktok-comments.mjs';

const source = 'https://www.tiktok.com/@creator/video/12345';
const bootstrap = (changes = {}) => ({__DEFAULT_SCOPE__: {'webapp.video-detail': {statusCode: 0,
  itemInfo: {itemStruct: {id: '12345', author: {id: '98765', uniqueId: 'creator'}, desc: '原始说明', ...changes}}}}});
test('TikTok original post is bound to exact video and stable author identity', () => {
  const row = projectTikTokPost(bootstrap(), 'creator', '12345');
  assert.equal(row.native_author_id, '98765'); assert.equal(row.text, '原始说明');
  assert.equal(row.source_url, source);
  for (const changes of [{id: 'other'}, {author: {id: '98765', uniqueId: 'other'}}, {author: {id: 98765, uniqueId: 'creator'}}, {desc: null}])
    assert.throws(() => projectTikTokPost(bootstrap(changes), 'creator', '12345'), /SOURCE_INVALID/);
});
test('TikTok comments require no commenter identity and retain only language provenance', () => {
  const rows = projectTikTokComments([{text: 'audience observation'}], source, 5);
  assert.deepEqual(Object.keys(rows[0]).sort(), ['source_url','text','text_truncated','type']);
  assert.equal(rows[0].source_url, source);
});
test('TikTok bounds and deduplicates samples without inventing independent comment locators', () => {
  const input = [{author_url:'/@a', text:'😀'.repeat(2001)}, {author_url:'/@a', text:'😀'.repeat(2001)}, {author_url:'/@b', text:'second'}];
  const rows = projectTikTokComments(input, source, 2);
  assert.equal(rows.length, 2); assert.equal(Array.from(rows[0].text).length, 2000);
  assert.equal(rows[0].text_truncated, true); assert.equal(rows[1].text, 'second');
  assert.equal(projectTikTokComments(input, source, 1).length, 1);
});
test('TikTok invalid input cannot navigate; valid request only uses one owned target', async () => {
  for (const id of ['https://evil.example/@creator/video/12345', source+'?x=1', source+'/', '--help'])
    await assert.rejects(readTikTokComments({}, {id}, () => assert.fail('browser accessed')), /INPUT/);
  const calls = [];
  const owned = {goto: async (...args) => calls.push(args), evaluate: async code => {
    assert.match(code, /readTikTokCommentPage/); return {session: {}, data: [{type: 'POST'}]};
  }};
  assert.deepEqual(await readTikTokComments({}, {id: source, limit: 5}, (_page, run) => run(owned)), [{type:'POST'}]);
  assert.deepEqual(calls, [[source, {settleMs:3000}]]);
});

test('TikTok waits for late author/action hydration, restores original text, then reads only that comment list', async t => {
  let polls = 0, clicks = 0, restores = 0;
  const trigger = {getClientRects: () => [1], click: () => {clicks++;}};
  const comment = {innerText:'公开评论', parentElement:{querySelector: () => ({getAttribute: () => '/@audience'})}};
  const root = {getClientRects: () => [1], querySelector: () => comment, querySelectorAll: () => [comment]};
  const doc = {body:{innerText:'Log in to comment'}, querySelector: () => {
    polls++; return polls < 3 ? null : {querySelector: selector => selector.startsWith('a[') ? {} : trigger};
  }, querySelectorAll: selector => selector.startsWith('script') ? [{textContent:JSON.stringify(bootstrap())}] : [root]};
  for (const [key, value] of [['document', doc], ['location', {href:source}]]) {
    const previous = Object.getOwnPropertyDescriptor(globalThis, key);
    Object.defineProperty(globalThis, key, {configurable:true, value});
    t.after(() => previous ? Object.defineProperty(globalThis, key, previous) : delete globalThis[key]);
  }
  t.mock.method(globalThis, 'setTimeout', callback => {callback(); return 0;});
  const read = () => readTikTokCommentPage('creator', '12345', 5, projectTikTokPost, projectTikTokComments, async () => {restores++;});
  const rows = await read();
  assert.equal(polls, 3); assert.equal(clicks, 1); assert.equal(restores, 1);
  assert.equal(rows[1].text, '公开评论');
  doc.body.innerText = 'Verify you are human';
  await assert.rejects(read(), /RISK/); assert.equal(clicks, 1);
  doc.body.innerText = 'Log in to view comments';
  await assert.rejects(read(), /AUTH/); assert.equal(clicks, 1);
  doc.body.innerText = '';
  polls = 0; doc.querySelector = () => {polls++; return null;};
  await assert.rejects(read(), /visible_author/); assert.equal(polls, 24); assert.equal(clicks, 1);
});
