import test from 'node:test';
import assert from 'node:assert/strict';
import vm from 'node:vm';
import { projectIdentity, readTwitterIdentity } from './onecrew-twitter-identity/profile.mjs';

const handle = 'creator_1';
const native = () => ({ __typename: 'User', rest_id: '1234567890123456789',
  core: { screen_name: 'Creator_1', name: 'Creator' },
  legacy: { screen_name: 'creator_1', name: 'Creator', description: 'Demo\nnotes', followers_count: 0 },
  location: { location: 'Self-entered' }, private_token: 'must-not-escape' });

test('preserves native string identity and real zero without guessing absent counts', () => {
  const input = native();
  const result = projectIdentity(input, handle);
  assert.equal(result.account_id, '1234567890123456789');
  assert.equal(result.handle, handle);
  assert.equal(result.follower_count, 0);
  assert.equal(result.identity_kind, 'native_id');
  assert.equal(result.evidence_source, 'x_user_by_screen_name');
  assert.equal(result.profile_url, 'https://x.com/creator_1');
  assert.ok(!JSON.stringify(result).includes('must-not-escape'));
  delete input.legacy.followers_count;
  assert.equal(projectIdentity(input, handle).follower_count, null);
});

test('rejects mismatched, unavailable, rounded or ambiguous identities and counts', () => {
  for (const update of [
    value => { value.rest_id = 1234567890123456789; },
    ...['', '0', '01', '-1', '1'.repeat(21)].map(id => value => { value.rest_id = id; }),
    value => { value.__typename = 'UserUnavailable'; },
    value => { value.core.screen_name = 'other'; },
    value => { delete value.core.screen_name; delete value.legacy.screen_name; },
    value => { value.relationship_counts = { followers: 123 }; },
    ...[-1, 1.5, '10K', Number.MAX_SAFE_INTEGER + 1].map(count => value => { value.legacy.followers_count = count; }),
    value => { value.legacy.description = 'x'.repeat(2001); },
    value => { value.core.name = value.legacy.name = ''; },
    value => { value.location.location = 'bad\0location'; },
  ]) {
    const value = native(); update(value);
    assert.throws(() => projectIdentity(value, handle), /ONECREW_X_IDENTITY_INVALID/);
  }
});

function fixture({ status = 200, cookie = 'ct0=test-session', origin = 'https://x.com',
  payload = { data: { user: { result: native() } } }, contentType = 'application/json',
  redirected = false, large = false, queryId = 'test-query', networkFailure = false } = {}) {
  const calls = [];
  let reads = 0, canceled = false;
  const page = {
    newTab: async url => { calls.push(['tab', url]); return 'dedicated-tab'; },
    setActivePage: async tab => calls.push(['bind', tab]),
    wait: async () => {},
    evaluate: async code => code.includes('async function restoreOriginalLanguage()') ? true : vm.runInNewContext(code, {
      URL, TextDecoder, AbortSignal,
      location: { origin, pathname: '/creator_1' }, document: { cookie },
      fetch: async (url, options) => {
        reads++;
        assert.equal(options.method, 'GET');
        assert.equal(options.redirect, 'error');
        assert.equal(options.credentials, 'include');
        assert.equal(options.headers['X-Csrf-Token'], 'test-session');
        assert.equal(new URL(url).pathname, '/i/api/graphql/test-query/UserByScreenName');
        assert.equal(JSON.parse(new URL(url).searchParams.get('variables')).screen_name, handle);
        if (networkFailure) throw new Error('Network failure');
        let sent = false;
        return { status, ok: status === 200, redirected, url,
          headers: { get: () => contentType }, body: { getReader: () => ({
            read: async () => {
              if (sent) return { done: true };
              sent = true;
              return { done: false, value: new TextEncoder().encode(large ? ' '.repeat(1048577) : JSON.stringify(payload)) };
            }, cancel: async () => { canceled = true; },
          }) },
        };
      },
    }),
  };
  return { page, calls, read: () => readTwitterIdentity(page, handle, async () => ({ queryId, features: {} }), 'public-client-token'),
    reads: () => reads, canceled: () => canceled };
}

test('one bounded public read uses selected tab, returns no cookies, and releases response reader', async () => {
  const f = fixture();
  const rows = await f.read();
  assert.equal(rows[0].account_id, '1234567890123456789');
  assert.equal(f.reads(), 1);
  assert.equal(f.canceled(), true);
  assert.deepEqual(f.calls, [['tab', 'https://x.com/creator_1'], ['bind', 'dedicated-tab']]);
  assert.ok(!JSON.stringify(rows).includes('test-session'));
});

test('auth, risk, redirect, partial GraphQL errors and malformed pages stop without retries', async () => {
  for (const [options, error] of [
    [{ status: 401 }, /AUTH/], [{ status: 403 }, /AUTH/], [{ status: 429 }, /RISK/],
    [{ status: 500 }, /INVALID/], [{ redirected: true }, /INVALID/],
    [{ contentType: 'text/html' }, /INVALID/], [{ payload: { errors: [{ message: 'private error' }] } }, /INVALID/],
    [{ payload: { data: { user: { result: native() } }, errors: [{}] } }, /INVALID/],
    [{ networkFailure: true }, /Network failure/], [{ large: true }, /TOO_LARGE/],
  ]) {
    const f = fixture(options);
    await assert.rejects(f.read(), error);
    assert.equal(f.reads(), 1);
    if (options.large) assert.equal(f.canceled(), true);
  }
});

test('invalid input, wrong origin and missing session do not issue an API read', async () => {
  for (const options of [{ origin: 'https://x.com.evil.example' }, { cookie: '' }, { queryId: '../wrong' }]) {
    const f = fixture(options);
    await assert.rejects(f.read(), /ONECREW_X_IDENTITY/);
    assert.equal(f.reads(), 0);
  }
  const f = fixture();
  await assert.rejects(readTwitterIdentity(f.page, 'creator?x=1', () => assert.fail(), ''), /INPUT/);
  assert.equal(f.calls.length, 0);
});
