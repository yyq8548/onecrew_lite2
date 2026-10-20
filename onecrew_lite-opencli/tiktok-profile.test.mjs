import test from 'node:test';
import assert from 'node:assert/strict';
import vm from 'node:vm';
import { projectProfile, readTikTokProfile } from './onecrew-tiktok-profile/profile.mjs';

const identity = { username: 'sample.creator', id: '1234567890123456789' };
const fixture = () => ({ __DEFAULT_SCOPE__: { 'webapp.user-detail': { statusCode: 0, userInfo: {
  user: { id: identity.id, uniqueId: identity.username, nickname: 'Example creator', signature: 'Shopify tutorials\nPublic content', verified: false, privateAccount: false },
  stats: { followerCount: 12345 },
} } } });

test('profile binds the search author and preserves original biography and real zero followers', () => {
  const row = projectProfile(fixture(), identity.username, identity.id);
  assert.equal(row.id, identity.id);
  assert.equal(row.follower_count, 12345);
  assert.equal(row.signature, 'Shopify tutorials\nPublic content');
  assert.equal(row.profile_url, 'https://www.tiktok.com/@sample.creator');
  assert.equal(row.evidence_source, 'profile_bootstrap');
  const zero = fixture(); zero.__DEFAULT_SCOPE__['webapp.user-detail'].userInfo.stats.followerCount = 0;
  assert.equal(projectProfile(zero, identity.username, identity.id).follower_count, 0);
});

test('missing counts, changed identity, malformed flags, gated response and unsafe numbers fail closed', () => {
  for (const change of [
    d => { delete d.userInfo.stats.followerCount; }, d => { delete d.userInfo.stats; },
    d => { d.userInfo.stats.followerCount = null; }, d => { d.userInfo.stats.followerCount = '12K'; },
    d => { d.userInfo.stats.followerCount = -1; }, d => { d.userInfo.stats.followerCount = 1.5; },
    d => { d.userInfo.stats.followerCount = Number.MAX_SAFE_INTEGER + 1; },
    d => { d.userInfo.user.id = '987654321'; }, d => { d.userInfo.user.id = Number(identity.id); },
    d => { d.userInfo.user.uniqueId = 'another'; }, d => { delete d.userInfo.user.privateAccount; },
    d => { d.userInfo.user.verified = 'yes'; }, d => { d.statusCode = 10202; },
    d => { d.userInfo.user.signature = '字'.repeat(5334); },
  ]) {
    const data = fixture(); change(data.__DEFAULT_SCOPE__['webapp.user-detail']);
    assert.throws(() => projectProfile(data, identity.username, identity.id), /ONECREW_TIKTOK_PROFILE_INVALID/);
  }
});

function pageFixture({ href, body = '', nodes = [JSON.stringify(fixture())] } = {}) {
  const calls = [];
  const page = {
    goto: () => assert.fail('No goto fallback'),
    newTab: async url => { calls.push(['newTab', url]); return 'bound-profile-target'; },
    setActivePage: async id => calls.push(['bind', id]),
    closeTab: async id => calls.push(['close', id]),
    wait: async options => {
      // Numeric wait(n) is DOM-stable early-return, not a fixed delay.
      assert.deepEqual(options, { time: 3 }); calls.push(['wait', options]);
    },
    evaluate: async code => {
      if (code.includes('async function restoreOriginalLanguage()')) return true;
      calls.push(['read']);
      return vm.runInNewContext(code, { URL, TextEncoder,
        location: { href: href ?? calls[0][1] },
        document: { body: { innerText: body }, querySelectorAll: selector => {
          if (selector === 'a[data-e2e="user-link"][href]') return [];
          assert.equal(selector, 'script#__UNIVERSAL_DATA_FOR_REHYDRATION__');
          return nodes.map(textContent => ({ textContent }));
        } },
        fetch: () => assert.fail('No extra network read'),
      });
    },
  };
  return { page, calls };
}

test('fixed public target, explicit wait, one bounded projected read and no API calls', async () => {
  const { page, calls } = pageFixture();
  const rows = await readTikTokProfile(page, identity.username, identity.id);
  assert.equal(rows.length, 1);
  assert.equal(rows[0].follower_count, 12345);
  assert.deepEqual(calls, [['newTab', 'https://www.tiktok.com/@sample.creator'], ['bind', 'bound-profile-target'], ['wait', { time: 3 }], ['read'], ['close','bound-profile-target']]);
});

test('origin drift, login/challenge, missing/duplicate/oversized bootstrap stop without fallback', async () => {
  for (const options of [
    { href: 'https://www.tiktok.com/@other' }, { href: 'https://www.tiktok.com.evil.example/@sample.creator' },
    { body: 'Verify you are human' }, { body: 'Log in to view this profile' },
    { nodes: [] }, { nodes: [JSON.stringify(fixture()), JSON.stringify(fixture())] },
    { nodes: ['not JSON'] }, { nodes: ['x'.repeat(4 * 1024 * 1024 + 1)] },
  ]) {
    const { page, calls } = pageFixture(options);
    await assert.rejects(readTikTokProfile(page, identity.username, identity.id));
    assert.equal(calls.length, 5);
  }
});

test('untrusted target identifiers are refused before any browser work', async () => {
  for (const [username, id] of [['', identity.id], ['../other', identity.id], ['a'.repeat(25), identity.id], ['ok', '0'], ['ok', '1;exit'], ['ok', 123]]) {
    const { page, calls } = pageFixture();
    await assert.rejects(readTikTokProfile(page, username, id), /ONECREW_TIKTOK_PROFILE_INPUT/);
    assert.equal(calls.length, 0);
  }
});

test('profile read releases only its own tab on success and extraction failure', async () => {
  for (const fail of [false,true]) {
    const {page,calls}=pageFixture(fail?{nodes:[]}:{});
    if(fail) await assert.rejects(readTikTokProfile(page,identity.username,identity.id));
    else await readTikTokProfile(page,identity.username,identity.id);
    assert.deepEqual(calls.filter(([kind])=>kind==='close'),[['close','bound-profile-target']]);
  }
});
