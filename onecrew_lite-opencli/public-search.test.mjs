import test from 'node:test';
import assert from 'node:assert/strict';
import { readPublic, lobstersRows } from './onecrew-public-search/read.mjs';

test('public reads recover transient HTTP failure and retain source identities', async () => {
  let calls = 0;
  const text = await readPublic('https://lobste.rs/t/finance.json', { sleep: async () => {}, fetcher: async () => {
    if (++calls === 1) return new Response('', { status: 503 });
    return new Response(JSON.stringify([{ short_id: 'yknfii', title: 'Story', submitter_user: 'writer', comments_url: 'https://lobste.rs/s/yknfii/story' }]), { headers: { 'content-type': 'application/json' } });
  } });
  assert.equal(calls, 2);
  assert.equal(lobstersRows(text, 1)[0].id, 'yknfii');
});
test('risk controls, missing scopes and changed response shapes are never retried', async () => {
  for (const [status, type, code] of [[429, 'text/html', 'PUBLIC_RISK'], [403, 'text/html', 'PUBLIC_RISK'], [404, 'text/html', 'PUBLIC_SCOPE'], [200, 'text/html', 'PUBLIC_SHAPE']]) {
    let calls = 0;
    await assert.rejects(readPublic('https://lobste.rs/t/finance.json', { fetcher: async () => { calls++; return new Response('', { status, headers: { 'content-type': type } }); } }), { message: code });
    assert.equal(calls, 1);
  }
});
test('transient transport retries are bounded and malformed content cannot become an empty success', async () => {
  let calls = 0;
  await assert.rejects(readPublic('https://lobste.rs/t/finance.json', { sleep: async () => {}, fetcher: async () => { calls++; throw Error('transport'); } }), { message: 'PUBLIC_TRANSPORT' });
  assert.equal(calls, 3);
  assert.throws(() => lobstersRows('{}', 2), { message: 'PUBLIC_SHAPE' });
});
