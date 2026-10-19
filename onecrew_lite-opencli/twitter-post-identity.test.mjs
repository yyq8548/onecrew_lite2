import test from 'node:test';
import assert from 'node:assert/strict';
import vm from 'node:vm';
import { projectPostAuthor, readTwitterPostAuthor } from './onecrew-twitter-post-identity/post.mjs';

const handle = 'creator_1', postId = '1234567890123456789', accountId = '987654321012345678';
const tweet = () => ({ __typename: 'Tweet', rest_id: postId,
  legacy: { id_str: postId, user_id_str: accountId },
  core: { user_results: { result: { __typename: 'User', rest_id: accountId,
    core: { screen_name: 'Creator_1' }, legacy: { screen_name: handle },
    private_token: 'must-not-escape' } } } });
const entry = value => ({ content: { itemContent: { tweet_results: { result: value } } } });
const payload = entries => ({ data: { threaded_conversation_with_injections_v2: {
  instructions: [{ type: 'TimelineAddEntries', entries }] } } });

test('binds the exact post to a native author, never a reply or quoted author', () => {
  const focal = tweet();
  focal.quoted_status_result = { result: { ...tweet(), rest_id: '321' } };
  const data = payload([entry({ ...tweet(), rest_id: '456' }), entry(focal)]);
  const result = projectPostAuthor(data, handle, postId);
  assert.deepEqual(result, { adapter_version: '1.0.0', post_id: postId,
    account_id: accountId, handle, profile_url: 'https://x.com/creator_1',
    source_url: 'https://x.com/creator_1/status/' + postId,
    identity_kind: 'native_post_author_id', evidence_source: 'x_tweet_detail' });
  assert.ok(!JSON.stringify(result).includes('must-not-escape'));
  assert.equal(projectPostAuthor(payload([entry({ __typename: 'TweetWithVisibilityResults', tweet: focal })]), handle, postId).account_id, accountId);
});

test('rejects wrong, unavailable, retweeted, conflicting or ambiguous focal identities', () => {
  for (const change of [
    t => { t.rest_id = '999'; }, t => { t.rest_id = Number(postId); },
    t => { t.legacy.id_str = '999'; }, t => { t.legacy.user_id_str = '999'; },
    t => { delete t.legacy.user_id_str; }, t => { t.__typename = 'TweetUnavailable'; },
    t => { t.legacy.retweeted_status_result = { result: tweet() }; },
    t => { t.core.user_results.result.__typename = 'UserUnavailable'; },
    t => { t.core.user_results.result.rest_id = Number(accountId); },
    t => { t.core.user_results.result.core.screen_name = 'reassigned'; },
    t => { t.core.user_results.result.legacy.screen_name = 'other'; },
  ]) {
    const t = tweet(); change(t);
    assert.throws(() => projectPostAuthor(payload([entry(t)]), handle, postId), /INVALID/);
  }
  for (const data of [payload([]), payload([entry(tweet()), entry(tweet())]),
    { ...payload([entry(tweet())]), errors: [{}] },
    payload([entry({ ...tweet(), rest_id: '456', quoted_status_result: { result: tweet() } })]),
  ]) assert.throws(() => projectPostAuthor(data, handle, postId), /INVALID/);
});

function fixture({ status = 200, cookie = 'ct0=test-session', origin = 'https://x.com',
  path = '/creator_1/status/' + postId, redirected = false, large = false,
  data = payload([entry(tweet())]), contentType = 'application/json', queryId = 'test-query' } = {}) {
  const calls = [];
  let reads = 0, canceled = false;
  const page = {
    newTab: async url => { calls.push(['tab', url]); return 'selected-tab'; },
    setActivePage: async tab => calls.push(['bind', tab]),
    wait: async () => {},
    evaluate: async code => code.includes('async function restoreOriginalLanguage()') ? true : vm.runInNewContext(code, {
      URL, TextDecoder, AbortSignal, location: { origin, pathname: path }, document: { cookie },
      fetch: async (url, options) => {
        reads++;
        assert.equal(options.method, 'GET');
        assert.equal(options.redirect, 'error');
        assert.equal(options.credentials, 'include');
        assert.equal(options.headers['X-Csrf-Token'], 'test-session');
        const target = new URL(url);
        assert.equal(target.pathname, '/i/api/graphql/test-query/TweetDetail');
        const variables = JSON.parse(target.searchParams.get('variables'));
        assert.equal(variables.focalTweetId, postId);
        assert.equal(variables.cursor, undefined);
        assert.equal(variables.includePromotedContent, false);
        let sent = false;
        return { status, ok: status === 200, redirected, url,
          headers: { get: () => contentType }, body: { getReader: () => ({
            read: async () => {
              if (sent) return { done: true };
              sent = true;
              return { done: false, value: new TextEncoder().encode(large ? ' '.repeat(1048577) : JSON.stringify(data)) };
            }, cancel: async () => { canceled = true; },
          }) } };
      },
    }),
  };
  return { calls, page, reads: () => reads, canceled: () => canceled,
    read: () => readTwitterPostAuthor(page, handle, postId,
      async () => ({ queryId, features: {}, fieldToggles: {} }), 'public-client-token') };
}

test('one bounded read on the selected tab returns only focal identity and releases the body', async () => {
  const f = fixture();
  const rows = await f.read();
  assert.equal(rows[0].account_id, accountId);
  assert.equal(f.reads(), 1);
  assert.equal(f.canceled(), true);
  assert.deepEqual(f.calls, [['tab', 'https://x.com/creator_1/status/' + postId], ['bind', 'selected-tab']]);
  assert.ok(!JSON.stringify(rows).includes('test-session'));
});

test('auth, risk, partial errors, redirects and excessive bodies stop without retries or pagination', async () => {
  for (const [options, pattern] of [
    [{ status: 401 }, /AUTH/], [{ status: 403 }, /AUTH/], [{ status: 429 }, /RISK/],
    [{ status: 500 }, /INVALID/], [{ redirected: true }, /INVALID/],
    [{ contentType: 'text/html' }, /INVALID/], [{ large: true }, /TOO_LARGE/],
    [{ data: { ...payload([entry(tweet())]), errors: [{}] } }, /INVALID/],
  ]) {
    const f = fixture(options);
    await assert.rejects(f.read(), pattern);
    assert.equal(f.reads(), 1);
    if (options.large) assert.equal(f.canceled(), true);
  }
});

test('invalid target, wrong tab or absent login never dispatch the API read', async () => {
  for (const options of [{ origin: 'https://x.com.evil.test' }, { path: '/home' },
    { cookie: '' }, { queryId: '../unsafe' }]) {
    const f = fixture(options);
    await assert.rejects(f.read(), /ONECREW_X_POST_IDENTITY/);
    assert.equal(f.reads(), 0);
  }
  for (const [h, id] of [[handle, '0'], [handle, '01'], [handle, '1'.repeat(21)],
    [handle, Number(postId)], ['Creator_1', postId], ['creator?x=1', postId]]) {
    const f = fixture();
    await assert.rejects(readTwitterPostAuthor(f.page, h, id, () => assert.fail(), ''), /INPUT/);
    assert.equal(f.calls.length, 0);
  }
});
