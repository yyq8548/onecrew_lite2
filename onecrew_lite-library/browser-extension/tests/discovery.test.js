import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';
const source = fs.readFileSync(new URL('../discovery.js', import.meta.url), 'utf8');
function discovery(fetch) {
  const ctx = vm.createContext({ fetch, AbortSignal });
  vm.runInContext(source, ctx);
  return ctx.OneCrewDiscovery;
}
test('skips an occupied non-OneCrew port and finds the next service', async () => {
  const visited = [];
  const api = discovery(async (url, options) => {
    visited.push(url);
    assert.equal(options.headers, undefined);
    return {
      ok: true,
      json: async () =>
        url.includes(':8092/')
          ? { ok: true, service: 'onecrew', protocol_version: 1 }
          : { ok: true, service: 'another-app' },
    };
  });
  assert.equal(await api.find(), 'http://127.0.0.1:8092');
  assert.equal(visited.length, 2);
});
test('prefers a known local endpoint, ignoring arbitrary external addresses', async () => {
  const visited = [];
  const api = discovery(async (url) => {
    visited.push(url);
    return { ok: true, json: async () => ({ ok: true, service: 'onecrew', protocol_version: 1 }) };
  });
  assert.equal(await api.find('http://127.0.0.1:8100'), 'http://127.0.0.1:8100');
  assert.equal(await api.find('https://example.com'), 'http://127.0.0.1:8091');
  assert(visited.every((url) => url.startsWith('http://127.0.0.1:')));
});
test('stops after the bounded discovery range when offline', async () => {
  let count = 0;
  const api = discovery(async () => {
    count++;
    throw new Error('offline');
  });
  assert.equal(await api.find(), null);
  assert.equal(count, 20);
});
