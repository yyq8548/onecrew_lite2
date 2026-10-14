import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';
import { channelsResultSchema } from '../../src/modules/platforms/youtube.js';

const readerNames = ['utils.js', 'search.js', 'video.js', 'channel.js'];
const readerSources = Object.fromEntries(
  readerNames.map((name) => [
    name,
    fs.readFileSync(
      new URL('../../browser-extension/platforms/youtube/' + name, import.meta.url),
      'utf8',
    ),
  ]),
);
const platformSource = fs.readFileSync(
  new URL('../../browser-extension/platforms/youtube/index.js', import.meta.url),
  'utf8',
);

function video(id: string, channelId?: string) {
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
function channelPayload(channelId: string, videosParams?: string) {
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
              ...(videosParams ? { endpoint: { browseEndpoint: { params: videosParams } } } : {}),
              content: { richGridRenderer: { contents: [video('recent-' + channelId)] } },
            },
          },
        ],
      },
    },
  };
}

function channelBootstrap(channelId: string) {
  return channelPayload(channelId, 'videos');
}

async function runChannelFixture(videosResponse: unknown) {
  let requestCount = 0;
  const context = vm.createContext({
    URL,
    Map,
    Set,
    TextEncoder,
    Uint8Array,
    AbortController,
    setTimeout,
    clearTimeout,
    ytcfg: { data_: { INNERTUBE_API_KEY: 'key', INNERTUBE_CONTEXT: {} } },
    fetch: async (_request: string, _init?: { body?: string }) => {
      requestCount += 1;
      return {
        ok: true,
        status: 200,
        json: async () => (requestCount === 1 ? channelBootstrap('UCfixture') : videosResponse),
      };
    },
  });
  for (const name of ['utils.js', 'channel.js']) vm.runInContext(readerSources[name], context);
  return context.OneCrewYoutubeReaders.getChannel('UCfixture', 3, 15000);
}

const readersContext = vm.createContext({
  URL,
  Map,
  Set,
  TextEncoder,
  Uint8Array,
  AbortController,
});
for (const name of readerNames) vm.runInContext(readerSources[name], readersContext);
type ReaderApi = {
  extractSearchPage(payload: unknown): { entries: Array<[string, unknown]> };
  extractJsonAssignmentFromHtml(html: string, key: string): unknown;
  extractSelectedRichGridContents(payload: unknown): unknown[];
  parseVideoItem(item: unknown): { url: string } | null;
};
const readers = (readersContext as unknown as { OneCrewYoutubeReaders: ReaderApi })
  .OneCrewYoutubeReaders;
const searchPage = readers.extractSearchPage({
  contents: [video('v1', 'UCsame'), video('v2', 'UCsame'), video('v3', 'UCother')],
});
assert.deepEqual(
  Array.from(searchPage.entries, ([id]) => id),
  ['v1', 'v2', 'v3'],
);
const watch = readers.extractJsonAssignmentFromHtml(
  'var ytInitialPlayerResponse = {"videoDetails":{"channelId":"UCwatch","shortDescription":"brace } in string"}};',
  'ytInitialPlayerResponse',
) as { videoDetails: { channelId: string; shortDescription: string } };
assert.equal(watch.videoDetails.channelId, 'UCwatch');
assert.equal(watch.videoDetails.shortDescription, 'brace } in string');
const selected = readers.extractSelectedRichGridContents({
  contents: {
    twoColumnBrowseResultsRenderer: {
      tabs: [
        {
          tabRenderer: {
            selected: true,
            tabIdentifier: 'VIDEOS',
            endpoint: { browseEndpoint: { params: 'videos' } },
            content: { richGridRenderer: { contents: [video('recent')] } },
          },
        },
        {
          tabRenderer: {
            title: 'Home',
            content: { richGridRenderer: { contents: [video('home-recommendation')] } },
          },
        },
      ],
    },
  },
});
const parsedSelected = readers.parseVideoItem(selected[0]);
if (!parsedSelected) throw new Error('selected Videos tab item was not parsed');
assert.equal(parsedSelected.url, 'https://www.youtube.com/watch?v=recent');

const selectedHomeWithGrid = await runChannelFixture({
  contents: {
    twoColumnBrowseResultsRenderer: {
      tabs: [
        {
          tabRenderer: {
            selected: true,
            title: 'Home',
            content: { richGridRenderer: { contents: [video('home-only')] } },
          },
        },
        {
          tabRenderer: {
            tabIdentifier: 'VIDEOS',
            endpoint: { browseEndpoint: { params: 'videos' } },
            content: { richGridRenderer: { contents: [video('videos-not-selected')] } },
          },
        },
      ],
    },
  },
});
assert.equal(selectedHomeWithGrid.recentVideos.length, 0);
assert.deepEqual(Array.from(selectedHomeWithGrid.warnings || []), ['recent_videos_unavailable']);

const selectedVideosWithoutGrid = await runChannelFixture({
  contents: {
    twoColumnBrowseResultsRenderer: {
      tabs: [
        {
          tabRenderer: {
            selected: true,
            tabIdentifier: 'VIDEOS',
            content: {},
          },
        },
        {
          tabRenderer: {
            title: 'Home',
            content: { richGridRenderer: { contents: [video('home-fallback')] } },
          },
        },
      ],
    },
  },
});
assert.equal(selectedVideosWithoutGrid.recentVideos.length, 0);
assert.deepEqual(Array.from(selectedVideosWithoutGrid.warnings || []), [
  'recent_videos_unavailable',
]);

const selectedVideos = await runChannelFixture({
  contents: {
    twoColumnBrowseResultsRenderer: {
      tabs: [
        {
          tabRenderer: {
            selected: true,
            tabIdentifier: 'VIDEOS',
            content: { richGridRenderer: { contents: [video('recent-valid')] } },
          },
        },
      ],
    },
  },
});
assert.equal(selectedVideos.recentVideos.length, 1);
assert.equal(selectedVideos.recentVideos[0].url, 'https://www.youtube.com/watch?v=recent-valid');

const platformContext = vm.createContext({
  URL,
  chrome: { i18n: { getMessage: (key: string) => key } },
});
platformContext.importScripts = (file: string) => {
  const name = file.split('/').pop() as string;
  vm.runInContext(readerSources[name], platformContext, { filename: file });
};
vm.runInContext(platformSource, platformContext);
const discover = (
  platformContext as unknown as {
    OneCrewYoutubePlatform: { youtubeDiscoverKols: (...args: unknown[]) => Promise<unknown> };
  }
).OneCrewYoutubePlatform.youtubeDiscoverKols;
const initial = { contents: Array.from({ length: 5 }, (_, index) => video('v' + index)) };
const mainContext = vm.createContext({
  URL,
  Map,
  Set,
  TextEncoder,
  Uint8Array,
  AbortController,
  Date,
  setTimeout,
  clearTimeout,
  document: { querySelector: () => null },
  ytcfg: { data_: { INNERTUBE_API_KEY: 'key', INNERTUBE_CONTEXT: {} } },
  ytInitialData: initial,
  fetch: async (request: string, init?: { body?: string }) => {
    if (request.startsWith('/watch')) {
      const id = new URL('https://www.youtube.com' + request).searchParams.get('v') || '';
      const index = Number(id.slice(1));
      return {
        ok: true,
        status: 200,
        text: async () =>
          'var ytInitialPlayerResponse = {"videoDetails":{"videoId":"' +
          id +
          '","channelId":"UC' +
          index +
          '","author":"Owner UC' +
          index +
          '"}};',
      };
    }
    const body = JSON.parse(init?.body || '{}') as { browseId?: string; params?: string };
    const index = Number(String(body.browseId || '').slice(2));
    if (index === 0)
      return {
        ok: true,
        status: 200,
        json: async () => ({
          alerts: [{ alertRenderer: { type: 'ERROR', text: { simpleText: 'gone' } } }],
        }),
      };
    if (index === 1)
      return {
        ok: true,
        status: 200,
        json: async () => ({ contents: {} }),
      };
    if (index < 4) return { ok: false, status: 500, json: async () => ({}) };
    return {
      ok: true,
      status: 200,
      json: async () => channelPayload(body.browseId || '', body.params ? undefined : 'videos'),
    };
  },
});
for (const name of readerNames) vm.runInContext(readerSources[name], mainContext);
vm.runInContext('globalThis.discover = (' + discover.toString() + ');', mainContext);
type DiscoveryResult = {
  channels: Array<{ platform_id: string }>;
  stop_reason: string;
  warnings: string[];
};
type Envelope = { ok: true; result: DiscoveryResult } | { ok: false; error: string };
const envelope = JSON.parse(
  JSON.stringify(await vm.runInContext('discover("fixture", 1, {}, {})', mainContext)),
) as Envelope;
assert.equal(envelope.ok, true);
const result = (envelope as { ok: true; result: DiscoveryResult }).result;
assert.deepEqual(
  Array.from(result.channels, (channel) => channel.platform_id),
  ['UC4'],
);
assert.equal(result.stop_reason, 'limit_reached');
assert.equal(result.warnings.length, 4);
channelsResultSchema.parse(result);

const readinessContext = vm.createContext({
  URL,
  Map,
  Set,
  TextEncoder,
  Uint8Array,
  AbortController,
  Date,
  document: { querySelector: () => null },
  ytcfg: { data_: {} },
  ytInitialData: undefined,
});
for (const name of readerNames) vm.runInContext(readerSources[name], readinessContext);
vm.runInContext('globalThis.discover = (' + discover.toString() + ');', readinessContext);
const readiness = JSON.parse(
  JSON.stringify(await vm.runInContext('discover("fixture", 1, {}, {})', readinessContext)),
) as Envelope;
assert.equal(readiness.ok, false);
assert.match((readiness as { ok: false; error: string }).error, /^ONECREW_YOUTUBE_NOT_READY:/);
console.log(
  'youtube discovery integration ok: OpenCLI readers, duplicate authors, balanced HTML, Videos tab, serialized MAIN, profile replenishment and envelope errors',
);
