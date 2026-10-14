import test from 'node:test';
import assert from 'node:assert/strict';
import {projectProfile, projectSearch, projectThread, readEvidence} from './evidence.mjs';
const did = 'did:plc:abcdefghijklmnopqrstuvwx';
const uri = `at://${did}/app.bsky.feed.post/3abcdefghijk`;
const profile = () => ({did, handle: 'author.bsky.social', displayName: 'Creator', followersCount: 15000,
  description: 'Business: studio@example.com TG: keyboard_studio', accessJwt: 'never export'});
const post = (id = uri, body = 'Original keyboard review') => ({uri: id,
  author: {did, handle: 'author.bsky.social'}, record: {$type: 'app.bsky.feed.post', text: body}});
const reply = index => ({$type: 'app.bsky.feed.defs#threadViewPost', post: {
  ...post(`at://${did}/app.bsky.feed.post/comment${index}`, `原创中文评论 ${index}`),
  record: {$type: 'app.bsky.feed.post', text: `原创中文评论 ${index}`, reply: {parent: {uri}}}}});
const thread = () => ({thread: {$type: 'app.bsky.feed.defs#threadViewPost', post: post(),
  replies: Array.from({length: 6}, (_, index) => reply(index))}});

test('native profile preserves contact text and unknown/zero counts without secrets', () => {
  for (const count of [undefined, 0, 15000]) {
    const row = projectProfile({...profile(), followersCount: count}, did);
    assert.equal(row.follower_count, count ?? null);
    assert.match(row.biography, /studio@example.com/);
    assert.equal(row.profile_url, `https://bsky.app/profile/${did}`);
    assert.doesNotMatch(JSON.stringify(row), /accessJwt|never export/);
  }
  for (const changes of [{did: 'did:plc:zzzzzzzzzzzzzzzzzzzzzzzz'}, {handle: '--bad'},
    {followersCount: -1}, {followersCount: null}, {description: '字'.repeat(3000)}])
    assert.throws(() => projectProfile({...profile(), ...changes}, did));
});
test('search binds relevant original posts to native DID and rejects contradictory author', () => {
  const rows = projectSearch({posts: [post()]}, 5);
  assert.equal(rows[0].author_id, did);
  assert.match(rows[0].source_url, /\/post\/3abcdefghijk$/);
  const wrong = post(); wrong.author.did = 'did:plc:zzzzzzzzzzzzzzzzzzzzzzzz';
  assert.throws(() => projectSearch({posts: [wrong]}, 5));
  assert.throws(() => projectSearch({posts: [post(), post()]}, 5));
  assert.throws(() => projectSearch({posts: [post()]}, 0));
});
test('comments use only first five direct replies with original native source', () => {
  const row = projectThread(thread(), uri, true);
  assert.equal(row.comments.length, 5);
  assert.ok(row.comments.every(comment => comment.source_url.includes('/post/comment')));
  assert.ok(row.comments.every(comment => !comment.text.endsWith('5')));
  assert.equal(projectThread(thread(), uri, false).comments.length, 0);
});
test('wrong parent, duplicate, blocked or mismatched native post fail without sixth refill', () => {
  for (const change of [
    t => {t.thread.post.uri = uri + 'other';},
    t => {t.thread.replies[0].post.record.reply.parent.uri = uri + 'other';},
    t => {t.thread.replies[1] = t.thread.replies[0];},
    t => {t.thread.replies[0].$type = 'app.bsky.feed.defs#blockedPost';},
    t => {t.thread.replies[0].post.record.text = '字'.repeat(667);},
  ]) {
    const payload = thread(); change(payload);
    assert.throws(() => projectThread(payload, uri, true));
  }
});
test('native richtext and external links retain provenance without rewriting original text', () => {
  const payload = thread();
  payload.thread.post.record.text = '官网链接';
  payload.thread.post.record.facets = [{index: {byteStart: 0, byteEnd: 12},
    features: [{$type: 'app.bsky.richtext.facet#link', uri: 'https://creator.example/contact'},
      {$type: 'app.bsky.richtext.facet#link', uri: 'https://t.me/keyboard_studio'}]}];
  payload.thread.post.record.embed = {$type: 'app.bsky.embed.external', external: {uri: 'mailto:studio@example.com'}};
  const row = projectThread(payload, uri, false);
  assert.equal(row.text, '官网链接');
  assert.deepEqual(row.public_links, ['https://creator.example/contact', 'https://t.me/keyboard_studio', 'mailto:studio@example.com']);
  payload.thread.post.record.facets[0].index.byteStart = 1;
  assert.throws(() => projectThread(payload, uri, false));
});
test('optional empty replies give no votes; quoted links and credential links are not promoted', () => {
  const payload = thread(); delete payload.thread.replies;
  payload.thread.post.record.embed = {$type: 'app.bsky.embed.record', record: {uri: 'https://unrelated.example/contact'}};
  payload.thread.post.record.facets = [{index: {byteStart: 0, byteEnd: 8},
    features: [{$type: 'app.bsky.richtext.facet#link', uri: 'https://creator.example/?access_token=must-not-export'}]}];
  const row = projectThread(payload, uri, true);
  assert.deepEqual(row.comments, []); assert.deepEqual(row.public_links, []);
  assert.doesNotMatch(JSON.stringify(row), /must-not-export|unrelated/);
});
function response(url, payload, status = 200) {
  const bytes = Buffer.from(JSON.stringify(payload));
  return {url, status, ok: status === 200, headers: new Headers({'content-type': 'application/json'}),
    body: new ReadableStream({start(controller) {controller.enqueue(bytes); controller.close();}})};
}
test('one fixed public endpoint read, no credentials, redirect or pagination', async () => {
  const calls = [];
  const rows = await readEvidence('comments', {uri}, async (url, options) => {
    calls.push({url, options}); return response(url, thread());
  });
  assert.equal(rows[0].comments.length, 5);
  assert.equal(calls.length, 1);
  const parsed = new URL(calls[0].url);
  assert.equal(parsed.origin, 'https://public.api.bsky.app');
  assert.equal(parsed.searchParams.get('depth'), '1');
  assert.equal(parsed.searchParams.get('parentHeight'), '0');
  assert.equal(calls[0].options.credentials, 'omit');
  assert.equal(calls[0].options.redirect, 'manual');
});
test('risk, redirects, overflow and invalid targets stop without retry', async () => {
  for (const status of [401, 403, 429]) {
    let calls = 0;
    await assert.rejects(readEvidence('profile', {did}, async url => {
      calls++; return response(url, {}, status);
    }), status === 401 ? /BLUESKY_AUTH/ : /BLUESKY_RISK/);
    assert.equal(calls, 1);
  }
  await assert.rejects(readEvidence('profile', {did}, async () => response('https://other.example/', profile())));
  await assert.rejects(readEvidence('profile', {did}, async url => response(url, {blob: 'x'.repeat(400001)})));
  await assert.rejects(readEvidence('profile', {did: 'https://other.example/'}, () => assert.fail('fetch')));
  await assert.rejects(readEvidence('post', {uri: 'at://evil/other/abc'}, () => assert.fail('fetch')));
});
