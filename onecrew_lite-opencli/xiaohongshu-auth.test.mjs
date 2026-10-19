import assert from 'node:assert/strict';
import { test } from 'node:test';
import vm from 'node:vm';
import { observeXhsLogin, readXhsLogin } from './onecrew-xiaohongshu-auth/session.mjs';

function observe(user, { origin = 'https://www.xiaohongshu.com', loginVisible = false, challenge = false } = {}) {
  return vm.runInNewContext(`(${observeXhsLogin.toString()})()`, {
    location: { origin }, window: { __INITIAL_STATE__: { user } },
    document: { querySelector: selector => selector === '#login-btn'
      ? { getClientRects: () => loginVisible ? [{}] : [] }
      : challenge ? {} : null },
  });
}

test('only a current-user login flag and identity on the search origin prove login', () => {
  assert.equal(observe({ loggedIn: true, userInfo: { userId: 'self' } }), 'logged_in');
  assert.equal(observe({ loggedIn: { _value: true }, userInfo: { _value: { user_id: 'self' } } }), 'logged_in');
  assert.equal(observe({ loggedIn: true, userInfo: {} }), 'unknown');
  assert.equal(observe({ userInfo: { userId: 'other' } }), 'unknown');
  assert.equal(observe({ loggedIn: true, userInfo: { userId: 'self' } }, { origin: 'https://creator.xiaohongshu.com' }), 'unknown');
  assert.equal(observe({ loggedIn: true, userInfo: { userId: 'self' } }, { challenge: true }), 'unknown');
});

test('anonymous requires both an explicit false flag and a visible login control', () => {
  assert.equal(observe({ loggedIn: false }, { loginVisible: true }), 'not_logged_in');
  assert.equal(observe({ loggedIn: false }), 'unknown');
  assert.equal(observe(undefined, { loginVisible: true }), 'unknown');
});

test('hydration checks are bounded, navigate only to the fixed search site, and expose no identity', async () => {
  const visits = [], waits = [];
  const results = ['unknown', { session: 'private', data: 'logged_in' }];
  const page = { newTab: async url => { visits.push(url); return 'owned'; }, setActivePage: () => {}, goto: () => { throw new Error('Navigation rejected'); }, wait: async n => waits.push(n), evaluate: async () => results.shift() ?? 'unknown' };
  assert.equal(await readXhsLogin(page), 'logged_in');
  assert.deepEqual(visits, ['https://www.xiaohongshu.com/explore']);
  assert.equal(waits.length, 2);
  waits.length = 0;
  assert.equal(await readXhsLogin(page), 'unknown');
  assert.equal(waits.length, 4);
});
