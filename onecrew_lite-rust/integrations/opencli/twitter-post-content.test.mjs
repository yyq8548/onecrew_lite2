import test from 'node:test';
import assert from 'node:assert/strict';
import vm from 'node:vm';
import { projectPostContent, readTwitterPostContent, postContentDiagnostic } from './onecrew-twitter-post-content/content.mjs';
import { projectPostAuthor } from './onecrew-twitter-post-identity/post.mjs';
import { projectLanguageSample } from './onecrew-twitter-post-content/language.mjs';

const handle = 'creator_1', postId = '1234567890123456789', accountId = '987654321012345678';

test('diagnostics expose fixed categories, not untrusted error payloads', () => {
  for (const code of ['AUTHOR', 'NOTE', 'TEXT', 'TIME', 'ENTITIES', 'NOTE_URLS_MISSING', 'URLS_TYPE', 'URL', 'MEDIA', 'QUOTE', 'OPERATION', 'RESPONSE']) {
    assert.equal(postContentDiagnostic('secret ONECREW_X_POST_CONTENT_INVALID_' + code + ' secret'), code.toLowerCase());
  }
  assert.equal(postContentDiagnostic('secret arbitrary error'), 'unclassified');
  assert.equal(postContentDiagnostic('ONECREW_X_POST_IDENTITY_INVALID secret'), 'identity');
  assert.equal(postContentDiagnostic('ONECREW_X_POST_IDENTITY_ORIGIN secret'), 'origin');
});
const tweet = () => ({ __typename: 'Tweet', rest_id: postId,
  legacy: { id_str: postId, user_id_str: accountId, full_text: 'My tool demonstration https://t.co/demo',
    created_at: 'Fri Sep 25 23:00:10 +0000 2026', entities: { urls: [{ expanded_url: 'https://example.com/tool' }] },
    extended_entities: { media: [{ type: 'video', secret: 'not-exported' }] } },
  core: { user_results: { result: { __typename: 'User', rest_id: accountId, core: { screen_name: handle } } } } });
const entry = t => ({ content: { itemContent: { tweet_results: { result: t } } } });
const payload = t => ({ data: { threaded_conversation_with_injections_v2: {
  instructions: [{ entries: [entry({ ...tweet(), rest_id: '222' }), entry(t)] }] } } });
const project = t => projectPostContent(payload(t), handle, postId, accountId, projectPostAuthor);

test('language evidence retains only direct replies, without commenter IDs', () => {
  const reply = (id, author, parent=postId) => ({...tweet(), rest_id:id,
    legacy:{...tweet().legacy,id_str:id,user_id_str:author,in_reply_to_status_id_str:parent,full_text:'原始评论'},
    core:{user_results:{result:{__typename:'User',rest_id:author,core:{screen_name:'reply_author'}}}},
  });
  const data = payload(tweet());
  data.data.threaded_conversation_with_injections_v2.instructions[0].entries.push(
    entry(reply('333','111')),entry(reply('444',accountId)),entry(reply('555','222','999')),
  );
  const sample = projectLanguageSample(data,handle,postId,accountId,projectPostContent,projectPostAuthor);
  assert.equal(sample.text,tweet().legacy.full_text);
  assert.equal(sample.comments.length,2);
  assert.ok(sample.comments.every(c=>!Object.hasOwn(c,'author_id')));
  assert.ok(sample.comments.every(c=>c.text==='原始评论'));
  assert.throws(()=>projectLanguageSample(data,handle,postId,'999',projectPostContent,projectPostAuthor),/AUTHOR/);
});

test('first five direct replies cannot be filled with a sixth usable reply',()=>{
  const data=payload(tweet());
  const entries=data.data.threaded_conversation_with_injections_v2.instructions[0].entries;
  for(let i=1;i<=6;i++) {
    const id=String(300+i);
    entries.push(entry({...tweet(),rest_id:id,legacy:{...tweet().legacy,id_str:id,in_reply_to_status_id_str:postId,
      full_text:i===3||i===4 ? `usable ${i}` : i===6 ? 'sixth must not count':''}}));
  }
  const sample=projectLanguageSample(data,handle,postId,accountId,projectPostContent,projectPostAuthor);
  assert.deepEqual(sample.comments.map(c=>c.text),['usable 3','usable 4']);
});

test('focal text and links stay bound to the native author; quote and reply content never escape', () => {
  const focal = tweet();
  focal.quoted_status_result = { result: { ...tweet(), rest_id: '333', secret: 'quote-not-exported' } };
  const result = project(focal);
  assert.equal(result.identity.account_id, accountId);
  assert.equal(result.identity.post_id, postId);
  assert.equal(result.text, focal.legacy.full_text);
  assert.equal(result.text_source, 'legacy_full_text');
  assert.equal(result.has_media, true);
  assert.equal(result.quotes_another_post, true);
  assert.deepEqual(result.expanded_urls, ['https://example.com/tool']);
  assert.ok(!JSON.stringify(result).includes('not-exported'));
  assert.equal(result.qualified, undefined);
  assert.throws(() => projectPostContent(payload(focal), handle, postId, '111', projectPostAuthor), /INVALID/);
});

test('full notes use their own text and entities; clipping preserves Unicode without preview fallback', () => {
  const focal = tweet();
  focal.note_tweet = { note_tweet_results: { result: {
    text: '😀'.repeat(2100), entity_set: { urls: [{ expanded_url: 'https://example.com/note' }] },
  } } };
  const result = project(focal);
  assert.equal(result.text, '😀'.repeat(2000));
  assert.equal(Buffer.byteLength(result.text), 8000);
  assert.equal(result.text_truncated, true);
  assert.equal(result.text_source, 'note_tweet');
  assert.deepEqual(result.expanded_urls, ['https://example.com/note']);
  delete focal.note_tweet.note_tweet_results.result.text;
  assert.throws(() => project(focal), /INVALID/);
});

test('legacy entities may omit URLs without discarding the bound post text', async () => {
  const focal = tweet();
  delete focal.legacy.entities.urls;
  const f = fixture({ data: payload(focal) });
  const [result] = await read(f);
  assert.equal(result.text, focal.legacy.full_text);
  assert.equal(result.identity.account_id, accountId);
  assert.equal(result.text_source, 'legacy_full_text');
  assert.equal(result.expanded_urls.length, 0);
  assert.equal(f.reads(), 1);
  // Neither fabricate links from text nor borrow the legacy preview's entities.
  focal.note_tweet = { note_tweet_results: { result: { text: 'Full note', entity_set: {} } } };
  assert.throws(() => project(focal), /NOTE_URLS_MISSING/);
  delete focal.note_tweet;
  for (const value of [null, false, {}, 'https://example.com/']) {
    focal.legacy.entities.urls = value;
    assert.throws(() => project(focal), /URLS_TYPE/);
  }
});

test('untrusted shapes, unsafe URLs, retweets and duplicate focal posts fail closed', () => {
  for (const change of [
    t => { t.legacy.user_id_str = '111'; },
    t => { t.legacy.retweeted_status_result = {}; },
    t => { t.legacy.full_text = ''; }, t => { t.legacy.full_text = 'bad\0text'; },
    t => { delete t.legacy.created_at; }, t => { t.legacy.created_at = '\n'; },
    t => { delete t.legacy.entities; }, t => { t.legacy.entities.urls = {}; },
    t => { t.legacy.entities.urls = [{ expanded_url: 'javascript:alert(1)' }]; },
    t => { t.legacy.entities.urls = [{ expanded_url: 'https://user:pass@example.com/' }]; },
    t => { t.legacy.entities.urls = [{ expanded_url: 'https://example.com/\n' }]; },
    t => { t.legacy.entities.urls = Array.from({ length: 21 }, (_, i) => ({ expanded_url: `https://example.com/${i}` })); },
    t => { t.legacy.extended_entities.media = [{ type: 'unknown' }]; },
    t => { t.legacy.is_quote_status = 'false'; },
    t => { t.note_tweet = {}; },
    t => { t.note_tweet = { note_tweet_results: { result: null } }; },
  ]) { const t = tweet(); change(t); assert.throws(() => project(t), /INVALID/); }
  const data = payload(tweet());
  data.data.threaded_conversation_with_injections_v2.instructions[0].entries.push(entry(tweet()));
  assert.throws(() => projectPostContent(data, handle, postId, accountId, projectPostAuthor), /INVALID/);
  const bare = tweet();
  bare.legacy.entities.urls = [];
  delete bare.legacy.extended_entities;
  assert.equal(project(bare).has_media, false);
  assert.deepEqual(project(bare).expanded_urls, []);
});

function fixture({ status = 200, cookie = 'ct0=synthetic-session', data = payload(tweet()) } = {}) {
  let reads = 0, tabs = 0;
  const page = {
    newTab: async url => { tabs++; assert.equal(url, `https://x.com/${handle}/status/${postId}`); return 'owned-tab'; },
    setActivePage: async tab => assert.equal(tab, 'owned-tab'), wait: async () => {},
    evaluate: async code => vm.runInNewContext(code, {
      URL, TextEncoder, TextDecoder, AbortSignal, setTimeout,
      location: { origin: 'https://x.com', pathname: `/${handle}/status/${postId}` },
      document: { cookie, documentElement: { setAttribute() {}, classList: { add() {}, contains() { return false; } } }, querySelectorAll() { return []; } },
      fetch: async (url, options) => {
        reads++;
        assert.equal(options.method, 'GET');
        assert.equal(options.redirect, 'error');
        assert.equal(JSON.parse(new URL(url).searchParams.get('variables')).focalTweetId, postId);
        const response = new Response(JSON.stringify(data), { status, headers: { 'content-type': 'application/json' } });
        Object.defineProperty(response, 'url', { value: url });
        return response;
      },
    }),
  };
  return { page, reads: () => reads, tabs: () => tabs };
}
const metadata = async () => ({ queryId: 'observed-query', features: {}, fieldToggles: {} });
const read = f => readTwitterPostContent(f.page, handle, postId, accountId, metadata, 'public-client-token');

test('serialized browser projection performs one read and keeps session values inside the tab', async () => {
  const f = fixture();
  const result = await read(f);
  assert.equal(f.reads(), 1);
  assert.equal(f.tabs(), 1);
  assert.equal(result[0].identity.account_id, accountId);
  assert.equal(result[0].text, tweet().legacy.full_text);
  assert.ok(!JSON.stringify(result).includes('synthetic-session'));
});

test('invalid account never navigates; wrong authors, auth and rate limits never retry', async () => {
  for (const account of ['0', '01', '', '1'.repeat(21), Number(accountId)]) {
    const f = fixture();
    await assert.rejects(readTwitterPostContent(f.page, handle, postId, account, metadata, ''), /INPUT/);
    assert.equal(f.tabs(), 0); assert.equal(f.reads(), 0);
  }
  for (const [options, pattern, count] of [[{ status: 401 }, /AUTH/, 1], [{ status: 429 }, /RISK/, 1],
    [{ cookie: '' }, /AUTH/, 0], [{ data: payload({ ...tweet(), rest_id: '999' }) }, /INVALID/, 1]]) {
    const f = fixture(options);
    await assert.rejects(read(f), pattern);
    assert.equal(f.reads(), count);
  }
});
