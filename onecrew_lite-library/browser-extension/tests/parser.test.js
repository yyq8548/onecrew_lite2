import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';

const files = ['utils.js', 'search.js', 'channel.js'];
const sources = Object.fromEntries(
  files.map((file) => [
    file,
    fs.readFileSync(new URL('../platforms/youtube/' + file, import.meta.url), 'utf8'),
  ]),
);
function loadReaders() {
  const context = vm.createContext({ URL, Map, Set, TextEncoder, Uint8Array, AbortController });
  for (const file of files) vm.runInContext(sources[file], context);
  return context.OneCrewYoutubeReaders;
}
test('extracts balanced bootstrap JSON with braces in strings', () => {
  const readers = loadReaders();
  const payload = readers.extractJsonAssignmentFromHtml(
    'var ytInitialPlayerResponse = {"videoDetails":{"channelId":"UCdemo","description":"} in string"}};',
    'ytInitialPlayerResponse',
  );
  assert.equal(payload.videoDetails.channelId, 'UCdemo');
  assert.equal(payload.videoDetails.description, '} in string');
});
test('extracts video search entries and deduplicates IDs per page', () => {
  const readers = loadReaders();
  const payload = readers.extractSearchPage({
    contents: [
      { videoRenderer: { videoId: 'v1', title: { simpleText: 'One' } } },
      { videoRenderer: { videoId: 'v1', title: { simpleText: 'Duplicate' } } },
      { videoRenderer: { videoId: 'v2', title: { simpleText: 'Two' } } },
    ],
  });
  assert.deepEqual(
    Array.from(payload.entries, ([id]) => id),
    ['v1', 'v2'],
  );
});
test('selects the selected rich grid for channel videos', () => {
  const readers = loadReaders();
  const payload = {
    contents: {
      twoColumnBrowseResultsRenderer: {
        tabs: [
          {
            tabRenderer: {
              selected: true,
              tabIdentifier: 'VIDEOS',
              endpoint: { browseEndpoint: { params: 'videos' } },
              content: { richGridRenderer: { contents: [] } },
            },
          },
          {
            tabRenderer: {
              title: 'Home',
              content: {
                richGridRenderer: {
                  contents: [
                    {
                      videoRenderer: {
                        videoId: 'recent',
                        title: { simpleText: 'Recent' },
                      },
                    },
                  ],
                },
              },
            },
          },
        ],
      },
    },
  };
  assert.equal(readers.extractSelectedRichGridContents(payload).length, 0);
  const item = {
    videoRenderer: {
      videoId: 'recent',
      title: { simpleText: 'Recent' },
    },
  };
  assert.equal(readers.parseVideoItem(item)?.url, 'https://www.youtube.com/watch?v=recent');
});
