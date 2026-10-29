import test from 'node:test';
import assert from 'node:assert/strict';
import { searchBilibili } from './onecrew-bilibili/search.mjs';
import { readBilibiliProfile } from './onecrew-bilibili-profile/profile.mjs';
import { readBilibiliVideo } from './onecrew-bilibili-video/video.mjs';
import { searchTikTok } from './onecrew-tiktok/search.mjs';
import { readTikTokProfile } from './onecrew-tiktok-profile/profile.mjs';
import { readTwitterIdentity } from './onecrew-twitter-identity/profile.mjs';
import { readTwitterPostAuthor } from './onecrew-twitter-post-identity/post.mjs';

for (const [name, read] of [
  ['Bilibili search', page => searchBilibili(page, 'AI', 5)],
  ['Bilibili profile', page => readBilibiliProfile(page, '8420618')],
  ['Bilibili video', page => readBilibiliVideo(page, 'BV1xx411c7mD', '8420618')],
  ['TikTok search', page => searchTikTok(page, 'AI', 5)],
  ['TikTok profile', page => readTikTokProfile(page, 'creator', '123456789')],
  ['X identity', page => readTwitterIdentity(page, 'creator', () => assert.fail('must not discover an operation'))],
  ['X post identity', page => readTwitterPostAuthor(page, 'creator', '123456789', () => assert.fail('must not discover an operation'))],
]) {
  test(`${name} stops before reading evidence when original text cannot be restored`, async () => {
    let evaluations = 0;
    const page = {
      newTab: async () => 'owned', setActivePage: async () => {},
      goto: async () => {},
      wait: async () => {}, sleep: async () => {}, closeTab: async () => {},
      evaluate: async code => {
        evaluations++;
        assert.match(code, /async function restoreOriginalLanguage\(\)/);
        throw new Error('ONECREW_ORIGINAL_TEXT_UNCONFIRMED');
      },
    };
    await assert.rejects(read(page), /ONECREW_ORIGINAL_TEXT_UNCONFIRMED/);
    assert.equal(evaluations, 1, 'no extraction or retry may run after failed restoration');
  });
}
