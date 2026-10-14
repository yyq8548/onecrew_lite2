import assert from 'node:assert/strict';
import test from 'node:test';
import vm from 'node:vm';
import { projectSearch, searchTikTok } from './onecrew-tiktok/search.mjs';

const video = (id = '1234567890123456789') => ({ type: 1, item: {
  id, author: { id: '9876543210123456789', uniqueId: 'shop.creator' },
  desc: 'Shopify store walkthrough\nOriginal caption', cookie: 'must-not-leak',
} });
const fixture = entries => ({ status_code: 0, data: entries, cursor: 0, cookie: 'must-not-leak' });

test('retains exact video and author IDs without coercion, returns only bounded native fields', () => {
  const rows = projectSearch(fixture([video(), video(), video('1234567890123456790')]), 2);
  assert.equal(rows.length, 2);
  assert.equal(rows[0].author_id, '9876543210123456789');
  assert.equal(rows[0].url, 'https://www.tiktok.com/@shop.creator/video/1234567890123456789');
  assert.equal(rows[0].desc, video().item.desc);
  assert.doesNotMatch(JSON.stringify(rows), /must-not-leak|cookie|followers/);
  assert.equal(projectSearch(fixture([video(), video('1234567890123456790')]), 1).length, 1);
});

test('invalid API envelopes are not valid zero results', () => {
  for (const data of [{}, { status_code: 1, data: [] }, { status_code: 0 }, { data: [] }, fixture(Array(201).fill(video()))]) {
    assert.throws(() => projectSearch(data, 5), /ONECREW_TIKTOK_INVALID/);
  }
  assert.deepEqual(projectSearch(fixture([]), 5), []);
});

test('rejects missing, conflicting or unsafe identity and oversized captions', () => {
  for (const change of [
    v => { v.item.id = 1234567890123456789; },
    v => { delete v.item.author.id; },
    v => { v.item.author.uniqueId = 'creator/../evil'; },
    v => { v.item.desc = '字'.repeat(5334); },
    v => { v.item.desc = '\u0000'; },
  ]) {
    const item = video(); change(item);
    assert.throws(() => projectSearch(fixture([item]), 5));
  }
  const conflicting = video(); conflicting.item.author.id = '12';
  assert.throws(() => projectSearch(fixture([video(), conflicting]), 5));
});

function browserFixture({ body = '', origin = 'https://www.tiktok.com', response = () => new Response(JSON.stringify(fixture([video()]))) } = {}) {
  const calls = [];
  const page = {
    goto: () => assert.fail('Regression: goto reproduces Navigation rejected'),
    newTab: async url => { calls.push(['newTab', url]); return 'exact-target'; },
    setActivePage: async identity => calls.push(['bind', identity]),
    wait: async seconds => calls.push(['wait', seconds]),
    evaluate: async script => script.includes('async function restoreOriginalLanguage()') ? true : vm.runInNewContext(script, {
      location: { origin }, document: { body: { innerText: body } },
      TextEncoder, TextDecoder, URLSearchParams, AbortSignal,
      fetch: async (url, options) => { calls.push(['fetch', url, options.redirect, options.credentials]); return response(); },
    }),
  };
  return { calls, page };
}

test('single fixed-origin search binds the new tab, no goto fallback or extra requests', async () => {
  const { page, calls } = browserFixture();
  const rows = await searchTikTok(page, 'ecommerce " ; globalThis.injected = true; //', 3);
  assert.equal(rows[0].id, video().item.id);
  assert.deepEqual(calls.slice(0, 3), [['newTab', 'https://www.tiktok.com/explore'], ['bind', 'exact-target'], ['wait', 3]]);
  assert.equal(calls.length, 4);
  assert.equal(new URL(calls[3][1], 'https://www.tiktok.com').searchParams.get('keyword'), 'ecommerce " ; globalThis.injected = true; //');
  assert.deepEqual(calls[3].slice(2), ['error', 'include']);
});

test('challenge, wrong origin and invalid inputs stop before the endpoint read', async () => {
  for (const options of [{ body: 'Verify you are human' }, { origin: 'https://other.example' }]) {
    const { page, calls } = browserFixture(options);
    await assert.rejects(searchTikTok(page, 'ecommerce', 3));
    assert.equal(calls.filter(([name]) => name === 'fetch').length, 0);
  }
  for (const [query, limit] of [['', 3], ['q\n', 3], ['x'.repeat(161), 3], ['q', 0], ['q', 51]]) {
    const { page, calls } = browserFixture();
    await assert.rejects(searchTikTok(page, query, limit), /ONECREW_TIKTOK_INPUT/);
    assert.equal(calls.length, 0);
  }
});

test('auth, risk, bad JSON and oversize responses stop once without retry', async () => {
  for (const [response, expected] of [
    [() => new Response('', { status: 401 }), /ONECREW_TIKTOK_AUTH/],
    [() => new Response('', { status: 429 }), /ONECREW_TIKTOK_RISK/],
    [() => new Response('', { status: 500 }), /ONECREW_TIKTOK_HTTP/],
    [() => new Response('<html>sign in</html>'), /JSON/],
    [() => new Response('x'.repeat(4 * 1024 * 1024 + 1)), /ONECREW_TIKTOK_TOO_LARGE/],
  ]) {
    const { page, calls } = browserFixture({ response });
    await assert.rejects(searchTikTok(page, 'ecommerce', 3), expected);
    assert.equal(calls.filter(([name]) => name === 'fetch').length, 1);
  }
});
