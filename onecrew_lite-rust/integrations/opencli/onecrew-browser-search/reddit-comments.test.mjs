import test from 'node:test';
import assert from 'node:assert/strict';
import {fetchRedditComments, projectRedditComments} from './reddit-comments.mjs';
import {readComments} from './comments.mjs';

const comment = (changes = {}) => ({kind: 't1', data: {id: 'reply1', name: 't1_reply1',
  link_id: 't3_post1', parent_id: 't3_post1', author: 'same-name', author_fullname: 't2_reader',
  body: '原始评论', score: 2, ...changes}});
const payload = (comments = [comment()]) => [
  {kind: 'Listing', data: {children: [{kind: 't3', data: {id: 'post1', name: 't3_post1',
    author: 'same-name', author_fullname: 't2_writer', title: 'Original', selftext: '正文'}}]}},
  {kind: 'Listing', data: {children: comments}},
];
test('Reddit preserves distinct native authors and original text from a single owned read', async () => {
  let reads = 0;
  const owned = {goto: async () => {}, evaluate: async () => { reads++; return payload(); }};
  const rows = await readComments({}, {'post-id': 'post1', limit: 5}, 'reddit', {
    name: 'read', access: 'read', domain: 'reddit.com', func: () => assert.fail('second read'),
  }, (_page, run) => run(owned));
  assert.equal(reads, 1);
  assert.equal(rows[0].native_author_id, 't2_writer');
  assert.ok(!Object.hasOwn(rows[1], 'native_author_id'));
  assert.equal(rows[1].text, '原始评论');
  assert.equal(rows[1].source_url, 'https://www.reddit.com/comments/post1/_/reply1/');
});
test('Reddit rejects cross-post, cross-parent, duplicate and conflicting identities', () => {
  for (const changes of [{link_id: 't3_other'}, {parent_id: 't1_other'}, {name: 't1_other'}, {id: '../x'}])
    assert.throws(() => projectRedditComments(payload([comment(changes)]), 'post1', 5), /INVALID/);
  assert.throws(() => projectRedditComments(payload([comment(), comment()]), 'post1', 5), /INVALID/);
  const data = payload(); data[0].data.children[0].data.id = 'other';
  assert.throws(() => projectRedditComments(data, 'post1', 5), /INVALID/);
});
test('Reddit keeps missing author IDs unknown and skips removed content', () => {
  for (const changes of [{author_fullname: undefined}, {author: '[deleted]'}, {author_fullname: 'nickname'}])
    assert.ok(!Object.hasOwn(projectRedditComments(payload([comment(changes)]), 'post1', 5)[1], 'native_author_id'));
  assert.equal(projectRedditComments(payload([comment({body: '[removed]'})]), 'post1', 5).length, 1);
  assert.equal(projectRedditComments(payload(), 'post1', 5)[0].score, null);
});
test('Reddit caps comments and text without expanding replies or more stubs', () => {
  const data = payload([comment({body: '😀'.repeat(2001), replies: {data: {children: [comment({id: 'nested'})]}}}),
    {kind: 'more'}, comment({id: 'reply2', name: 't1_reply2'})]);
  const rows = projectRedditComments(data, 'post1', 1);
  assert.equal(rows.length, 2); assert.equal(Array.from(rows[1].text).length, 2000);
  assert.equal(rows[1].text_truncated, true);
  assert.equal(projectRedditComments(payload([]), 'post1', 5).length, 1);
  assert.throws(() => projectRedditComments({}, 'post1', 5), /INVALID/);
});
test('Reddit rejects bad source before any browser access', async () => {
  for (const id of ['https://reddit.com', 'post1&limit=100', '--help'])
    await assert.rejects(readComments({}, {'post-id': id}, 'reddit', {
      name: 'read', access: 'read', func() {},
    }, () => assert.fail('browser accessed')), /INPUT/);
});
test('Reddit single fetch distinguishes inaccessible, login, rate limit and empty', async t => {
  const previous = Object.getOwnPropertyDescriptor(globalThis, 'location');
  Object.defineProperty(globalThis, 'location', {configurable: true, value: {origin: 'https://www.reddit.com'}});
  t.after(() => previous ? Object.defineProperty(globalThis, 'location', previous) : delete globalThis.location);
  let response = {url: 'https://www.reddit.com/comments/post1.json', status: 200, ok: true, json: async () => payload([])};
  t.mock.method(globalThis, 'fetch', async (url, options) => {
    assert.equal(url, '/comments/post1.json?sort=best&limit=5&depth=1&raw_json=1');
    assert.equal(options.credentials, 'include'); return response;
  });
  assert.equal(projectRedditComments(await fetchRedditComments('post1', 5), 'post1', 5).length, 1);
  for (const [status, stop] of [[401, 'auth'], [403, 'inaccessible'], [404, 'inaccessible'], [429, 'risk'], [500, 'read']]) {
    response = {...response, status, ok: false};
    assert.deepEqual(await fetchRedditComments('post1', 5), {stop});
    assert.throws(() => projectRedditComments({stop}, 'post1', 5));
  }
  response = {...response, status: 200, ok: true, url: 'https://evil.example/comments/post1.json'};
  assert.deepEqual(await fetchRedditComments('post1', 5), {stop: 'read'});
});
