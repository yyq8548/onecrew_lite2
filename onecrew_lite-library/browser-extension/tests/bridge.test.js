import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';

// Load the real worker graph, with only Chrome/network boundaries substituted.
test('worker loads platform modules and dispatches browser/search operations', async () => {
  const stored = {
    bridgeDisabled: true,
    ownedBrowserTabs: [7],
    pendingResults: { session: 's', items: { c: { result: { ok: true } } } },
  };
  const listener = { addListener() {} };
  let posted;
  const context = vm.createContext({
    setTimeout,
    clearTimeout,
    AbortController,
    AbortSignal,
    URL,
    chrome: {
      i18n: { getMessage: (key) => key },
      storage: {
        local: {
          get: async () => ({ ...stored }),
          set: async (value) => Object.assign(stored, value),
          remove: async () => {},
        },
      },
      tabs: { get: async () => ({ id: 7, url: 'https://www.youtube.com/', title: 'YouTube' }) },
      alarms: { create() {}, onAlarm: listener },
      runtime: {
        id: 'test-extension',
        onStartup: listener,
        onInstalled: listener,
        onMessage: listener,
      },
    },
    fetch: async (url, options) => {
      posted = { url, options };
      return { ok: true, status: 200 };
    },
  });
  context.importScripts = (...paths) =>
    paths.forEach((path) =>
      vm.runInContext(fs.readFileSync(new URL(`../${path}`, import.meta.url), 'utf8'), context, {
        filename: path,
      }),
    );
  context.importScripts('background.js');
  const tabs = await vm.runInContext('command({operation:"browser.tabs"})', context);
  assert.equal(tabs.tabs[0].tab_id, 7);
  await assert.rejects(
    vm.runInContext(
      'command({operation:"browser.open",input:{url:"https://example.com"}})',
      context,
    ),
    /bridgeUnsupportedUrl/,
  );
  vm.runInContext(
    'OneCrewPlatforms.operations["youtube.discover_kols"] = async input => ({query:input.query})',
    context,
  );
  const result = await vm.runInContext(
    'command({operation:"youtube.discover_kols",input:{query:"music"}})',
    context,
  );
  assert.equal(result.query, 'music');
  await vm.runInContext('serviceUrl="http://127.0.0.1:8092"; flushResults("token","s")', context);
  assert.equal(posted.url, 'http://127.0.0.1:8092/api/v1/browser/results');
  assert.ok(posted.options.signal);
  assert.deepEqual(Object.keys(stored.pendingResults.items), []);
});
