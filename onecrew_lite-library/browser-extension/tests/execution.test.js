import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';

const platformSource = fs.readFileSync(
  new URL('../platforms/youtube/index.js', import.meta.url),
  'utf8',
);
const readerFiles = ['utils.js', 'search.js', 'video.js', 'channel.js'];
const readerSources = Object.fromEntries(
  readerFiles.map((file) => [
    file,
    fs.readFileSync(new URL('../platforms/youtube/' + file, import.meta.url), 'utf8'),
  ]),
);

test('manifest and locale catalogs are wired and key sets match', () => {
  const manifest = JSON.parse(fs.readFileSync(new URL('../manifest.json', import.meta.url)));
  assert.equal(manifest.version, '0.1.2');
  assert.equal(manifest.default_locale, 'en');
  assert.match(manifest.name, /^__MSG_[^_]+__$/);
  assert.match(manifest.description, /^__MSG_[^_]+__$/);
  assert.match(manifest.action.default_title, /^__MSG_[^_]+__$/);
  const en = JSON.parse(fs.readFileSync(new URL('../_locales/en/messages.json', import.meta.url)));
  const zh = JSON.parse(
    fs.readFileSync(new URL('../_locales/zh_CN/messages.json', import.meta.url)),
  );
  assert.deepEqual(Object.keys(en).sort(), Object.keys(zh).sort());
  for (const catalog of [en, zh]) {
    for (const entry of Object.values(catalog)) assert.ok(entry.message);
  }
});

function video(id, channelId) {
  return {
    videoRenderer: {
      videoId: id,
      title: { simpleText: 'Video ' + id },
      ...(channelId
        ? {
            ownerText: {
              runs: [
                {
                  text: 'Owner ' + channelId,
                  navigationEndpoint: { browseEndpoint: { browseId: channelId } },
                },
              ],
            },
          }
        : {}),
    },
  };
}

function channelPayload(channelId) {
  return {
    metadata: {
      channelMetadataRenderer: {
        externalId: channelId,
        title: 'Channel ' + channelId,
        channelUrl: 'https://www.youtube.com/channel/' + channelId,
        avatar: { thumbnails: [{ url: 'https://yt3.googleusercontent.com/' + channelId }] },
      },
    },
    contents: {
      twoColumnBrowseResultsRenderer: {
        tabs: [
          {
            tabRenderer: {
              selected: true,
              tabIdentifier: 'VIDEOS',
              content: {
                richGridRenderer: { contents: [video('recent-' + channelId)] },
              },
            },
          },
        ],
      },
    },
  };
}

function makeDiscoverySource() {
  const chrome = { i18n: { getMessage: (key) => key } };
  const context = vm.createContext({
    URL,
    chrome,
    importScripts(file) {
      const name = file.split('/').pop();
      vm.runInContext(readerSources[name], context, { filename: file });
    },
  });
  vm.runInContext(platformSource, context, {
    filename: new URL('../platforms/youtube/index.js', import.meta.url).pathname,
  });
  return context.OneCrewYoutubePlatform.youtubeDiscoverKols.toString();
}

const discoverySource = makeDiscoverySource();

function makeMainContext({ initial, loggedIn = false, fetch }) {
  const context = vm.createContext({
    URL,
    Map,
    Set,
    TextEncoder,
    Uint8Array,
    AbortController,
    setTimeout,
    clearTimeout,
    Date,
    ytcfg: {
      data_: {
        INNERTUBE_API_KEY: 'api-key',
        INNERTUBE_CONTEXT: { client: { clientName: 'WEB', clientVersion: '1' } },
        LOGGED_IN: loggedIn,
      },
      get(name) {
        return name === 'LOGGED_IN' ? loggedIn : undefined;
      },
    },
    ytInitialData: initial,
    document: { querySelector: () => null },
    fetch,
  });
  for (const file of readerFiles) vm.runInContext(readerSources[file], context, { filename: file });
  vm.runInContext('globalThis.discover = (' + discoverySource + ');', context);
  return context;
}

async function invoke(context, ...args) {
  const envelope = await context.discover(...args);
  if (!envelope?.ok) throw new Error(envelope?.error || 'discovery failed');
  return envelope.result;
}

function response(payload, status = 200) {
  return {
    ok: status >= 200 && status < 300,
    status,
    text: async () => JSON.stringify(payload),
    json: async () => payload,
  };
}

test('discovers unique channels from video owners and deduplicates channel profiles', async () => {
  let watchRequests = 0;
  let browseRequests = 0;
  const context = makeMainContext({
    initial: { contents: [video('v1', 'UC1'), video('v2', 'UC1'), video('v3', 'UC2')] },
    fetch: async (request, init) => {
      if (String(request).startsWith('/watch')) {
        watchRequests += 1;
        const videoId = new URL('https://www.youtube.com' + request).searchParams.get('v');
        const channelId = videoId === 'v3' ? 'UC2' : 'UC1';
        return {
          ok: true,
          status: 200,
          text: async () =>
            'var ytInitialPlayerResponse = {"videoDetails":{"videoId":"' +
            videoId +
            '","channelId":"' +
            channelId +
            '","author":"Owner ' +
            channelId +
            '"}};',
          json: async () => ({}),
        };
      }
      browseRequests += 1;
      const channelId = JSON.parse(init?.body || '{}').browseId;
      return response(channelPayload(channelId));
    },
  });
  const result = await invoke(context, 'query', 2, {}, {});
  assert.deepEqual(
    Array.from(result.channels, (channel) => channel.platform_id),
    ['UC1', 'UC2'],
  );
  assert.equal(watchRequests, 3);
  assert.equal(browseRequests, 2);
  assert.equal(result.channels[0].matched_videos.length, 2);
  assert.equal(result.stop_reason, 'limit_reached');
});

test('uses watch HTML to resolve a video with no owner channelId', async () => {
  const requests = [];
  const context = makeMainContext({
    initial: { contents: [video('v1')] },
    fetch: async (request, init) => {
      requests.push(String(request));
      if (String(request).startsWith('/watch'))
        return {
          ok: true,
          status: 200,
          text: async () =>
            'var ytInitialPlayerResponse = {"videoDetails":{"channelId":"UCwatch","author":"Watch owner"}};',
        };
      return response(channelPayload(JSON.parse(init.body).browseId));
    },
  });
  const result = await invoke(context, 'query', 1, {}, {});
  assert.equal(result.channels[0].platform_id, 'UCwatch');
  assert.ok(requests.some((request) => request.startsWith('/watch')));
});

test('stops on authentication or captcha failures', async () => {
  const context = makeMainContext({
    initial: { contents: [video('v1', 'UC1')] },
    fetch: async () => response({ error: { message: 'authentication required' } }, 403),
  });
  await assert.rejects(invoke(context, 'query', 1, {}, {}), /HTTP 403/);
});

test('requires SAPISIDHASH cookie for signed-in continuation', async () => {
  const context = makeMainContext({
    loggedIn: true,
    initial: {
      contents: [
        video('v1', 'UC1'),
        {
          continuationItemRenderer: {
            continuationEndpoint: { continuationCommand: { token: 'cursor-1' } },
          },
        },
      ],
    },
    fetch: async () => {
      throw new Error('unexpected fetch');
    },
  });
  await assert.rejects(invoke(context, 'query', 2, {}, {}), /SAPISID cookie/);
});

test('readiness marker is available inside serialized MAIN function', async () => {
  const context = makeMainContext({ initial: undefined, fetch: async () => response({}) });
  const envelope = await context.discover('query', 1, {}, {});
  assert.equal(envelope.ok, false);
  await assert.rejects(invoke(context, 'query', 1, {}, {}), (error) => {
    assert.match(error.message, /^ONECREW_YOUTUBE_NOT_READY:/);
    return true;
  });
});

test('unwraps injected envelopes and retries readiness errors', async () => {
  let functionCalls = 0;
  let removed = 0;
  const chrome = {
    runtime: { id: 'test-extension' },
    i18n: { getMessage: (key) => key },
    tabs: {
      async create() {
        return { id: 7 };
      },
      async get() {
        return { id: 7, status: 'complete', url: 'https://www.youtube.com/results' };
      },
      async remove() {
        removed += 1;
      },
    },
    cookies: { async get() {} },
    scripting: {
      async executeScript(options) {
        if (options.files) return [{ result: null }];
        functionCalls += 1;
        if (functionCalls === 1)
          return [{ result: { ok: false, error: 'ONECREW_YOUTUBE_NOT_READY: fixture' } }];
        return [
          {
            result: {
              ok: true,
              result: { channels: [], has_more: false, stop_reason: 'exhausted', warnings: [] },
            },
          },
        ];
      },
    },
  };
  const context = vm.createContext({
    URL,
    chrome,
    importScripts(file) {
      const name = file.split('/').pop();
      vm.runInContext(readerSources[name], context, { filename: file });
    },
    setTimeout,
    clearTimeout,
  });
  vm.runInContext(platformSource, context);
  const result = await context.OneCrewYoutubePlatform.discoverKols('fixture', 1);
  assert.deepEqual(result.channels, []);
  assert.equal(functionCalls, 2);
  assert.equal(removed, 1);
});

test('generic execute preserves raw MAIN results and propagates injection failures', async () => {
  let injectedError = false;
  const chrome = {
    i18n: { getMessage: (key) => key },
    tabs: {
      async get() {
        return { id: 9, url: 'https://www.youtube.com/results' };
      },
    },
    scripting: {
      async executeScript(options) {
        if (injectedError) throw new Error('MAIN injection failed');
        assert.equal(typeof options.func, 'function');
        return [{ result: { raw: true } }];
      },
    },
  };
  const context = vm.createContext({
    URL,
    chrome,
    importScripts(file) {
      const name = file.split('/').pop();
      vm.runInContext(readerSources[name], context, { filename: file });
    },
  });
  vm.runInContext(platformSource, context);
  const execute = context.OneCrewYoutubePlatform.execute;
  const raw = await execute(
    9,
    function () {
      return { raw: true };
    },
    [],
  );
  assert.deepEqual(raw, { raw: true });
  injectedError = true;
  await assert.rejects(
    execute(
      9,
      function () {
        return { raw: true };
      },
      [],
    ),
    /MAIN injection failed/,
  );
});
