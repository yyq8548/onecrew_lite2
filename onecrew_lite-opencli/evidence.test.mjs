import assert from 'node:assert/strict';
import test from 'node:test';
import { projectWatchHtml, readVideoEvidence } from './onecrew-youtube/evidence.mjs';

const id = 'abcdefghijk';
const channelId = 'UC' + 'a'.repeat(22);
function fixture(change = () => {}) {
  const player = { videoDetails: {
    videoId: id, channelId, title: 'Actual tool walkthrough', author: 'Creator',
    shortDescription: 'I use this tool to remove pauses: https://www.descript.com/',
  }, playabilityStatus: { status: 'OK' } };
  const section = {
    owner: { videoOwnerRenderer: { subscriberCountText: { simpleText: '12K subscribers' },
      navigationEndpoint: { browseEndpoint: { browseId: channelId } } } },
    attributedDescription: { content: 'Try https://www.descript.com/…', commandRuns: [{
      startIndex: 4, length: 25, onTap: { innertubeCommand: { urlEndpoint: {
        url: '/redirect?q=https%3A%2F%2Fwww.descript.com%2F&redir_token=not-exported',
      } } },
    }] },
  };
  change(player, section);
  const initial = { contents: { twoColumnWatchNextResults: { results: { results: {
    contents: [{ videoSecondaryInfoRenderer: section }],
  } } } }, recommendations: { url: 'https://not-author-owned.example/' } };
  return '<script>var ytInitialPlayerResponse = ' + JSON.stringify(player)
    + ';var ytInitialData = ' + JSON.stringify(initial) + ';</script>';
}

test('preserves raw description and separately attributes real description endpoints', () => {
  const result = projectWatchHtml(fixture(), id);
  assert.match(result.description, /descript\.com\/$/);
  assert.match(result.displayDescription, /…$/);
  assert.equal(result.links.length, 2);
  assert.deepEqual(result.links.map(link => link.source),
    ['shortDescription', 'attributedDescription.commandRuns']);
  assert.ok(result.links.every(link => link.url === 'https://www.descript.com/'));
  assert.doesNotMatch(JSON.stringify(result), /not-exported|not-author-owned/);
});

test('missing, stale, conflicting owner and gated bootstrap fail closed', () => {
  for (const change of [
    p => { p.videoDetails.videoId = 'stale1234567'; },
    p => { p.videoDetails.channelId = '@not-a-stable-id'; },
    p => { p.playabilityStatus.status = 'LOGIN_REQUIRED'; },
    (_, s) => { s.owner.videoOwnerRenderer.navigationEndpoint.browseEndpoint.browseId = 'different'; },
    p => { delete p.videoDetails.shortDescription; },
    (_, s) => { s.attributedDescription.commandRuns[0].length = 999; },
  ]) assert.throws(() => projectWatchHtml(fixture(change), id), /EVIDENCE_INVALID/);
  assert.throws(() => projectWatchHtml('<html>Sign in</html>', id));
});

test('never repairs ellipsized URLs or exports credential URLs', () => {
  const result = projectWatchHtml(fixture((p, s) => {
    p.videoDetails.shortDescription = 'https://tool.example/path... https://tool.example/… '
      + 'https://tool.example/?access_token=secret https://user:pass@tool.example/';
    s.attributedDescription.commandRuns = [];
  }), id);
  assert.deepEqual(result.links, []);
});

test('original description retains late contact details beyond the visible preview', () => {
  const description = 'Original video notes. '.repeat(250)
    + '\nBusiness: studio@example.org https://t.me/creator_bot';
  const result = projectWatchHtml(fixture((p, s) => {
    p.videoDetails.shortDescription = description;
    s.attributedDescription.content = 'Original video notes…';
    s.attributedDescription.commandRuns = [];
  }), id);
  assert.equal(result.description, description);
  assert.ok(result.description.indexOf('studio@example.org') > 4000);
  assert.notEqual(result.description, result.displayDescription);
});

test('bounded UTF-8 text rejects overflow instead of cutting a URL', () => {
  assert.throws(() => projectWatchHtml(fixture(p => {
    p.videoDetails.shortDescription = '字'.repeat(5334);
  }), id));
  assert.throws(() => projectWatchHtml(' '.repeat(8 * 1024 * 1024 + 1), id));
});

test('bootstrap scanner handles quoted braces and escaped quotes without executing code', () => {
  const result = projectWatchHtml(fixture(p => {
    p.videoDetails.title = 'A } brace and "quote" \\ path';
  }).replaceAll('var ytInitialData = ', 'window["ytInitialData"]='), id);
  assert.equal(result.title, 'A } brace and "quote" \\ path');
});

test('invalid identifier never navigates', async () => {
  for (const bad of ['--profile', 'https://evil.example/', 'abcdefghijk?x=1', '\nabcdefghijk']) {
    await assert.rejects(readVideoEvidence({ goto: () => assert.fail('navigation') }, bad));
  }
});
