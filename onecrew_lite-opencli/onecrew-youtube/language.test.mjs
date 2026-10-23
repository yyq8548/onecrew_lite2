import test from 'node:test';
import assert from 'node:assert/strict';
import { projectLanguageComments, projectRecentVideos, readYoutubeLanguage } from './language.mjs';
const channel = 'UCaaaaaaaaaaaaaaaaaaaaaa';
const video = 'abcdefghijk';
const mutation = (id, author, text) => ({ payload: { commentEntityPayload: {
  properties: { commentId: id, content: { content: text } }, author: { channelId: author },
} } });
test('comments preserve text and permalink without commenter identity, bounded by UTF-8 bytes and count', () => {
  const comments = projectLanguageComments({ frameworkUpdates: { entityBatchUpdate: { mutations: [
    mutation('Ug1', channel, '作者回复'), mutation('Ug2', 'UCbbbbbbbbbbbbbbbbbbbbbb', '观众评论'),
    ...Array.from({ length: 30 }, (_, i) => mutation(`Ug${i + 3}`, channel, '字'.repeat(1000))),
  ] } } }, video);
  assert.equal(comments.length, 5);
  assert.ok(comments.every(c => !Object.hasOwn(c, 'author_id')));
  assert.equal(comments[0].source_url, `https://www.youtube.com/watch?v=${video}&lc=Ug1`);
  assert.ok(Buffer.byteLength(comments[2].text) <= 2000);
});
test('unusable early comments cannot be replaced by a sixth language vote',()=>{
  const mutations=[mutation('one',channel,''),mutation('two',channel,''),mutation('three',channel,'first'),
    mutation('four',channel,'second'),mutation('five',channel,''),mutation('six',channel,'must not be used')];
  const result=projectLanguageComments({frameworkUpdates:{entityBatchUpdate:{mutations}}},video);
  assert.deepEqual(result.map(c=>c.text),['first','second']);
});
test('missing commenter identity is allowed; malformed locators and duplicates are excluded', () => {
  const mutations = [mutation('one', channel, 'original'), mutation('one', channel, 'duplicate'),
    mutation('two', '@display-only', 'unbound'), mutation('bad&id', channel, 'wrong'), { payload: { recommendation: {} } }];
  assert.equal(projectLanguageComments({ frameworkUpdates: { entityBatchUpdate: { mutations } } }, video).length, 2);
  assert.throws(() => projectLanguageComments({}, video));
});
test('recent videos are limited to three locators from the bound selected channel tab', () => {
  const data = { metadata: { channelMetadataRenderer: { externalId: channel } }, contents: { twoColumnBrowseResultsRenderer: { tabs: [
    { tabRenderer: { selected: true, content: { richGridRenderer: { contents: Array.from({ length: 5 }, (_, i) => ({ richItemRenderer: { content: { videoRenderer: { videoId: `abcdefghij${i}` } } } })) } } } },
  ] } } };
  assert.equal(projectRecentVideos(data, channel).length, 3);
  assert.throws(() => projectRecentVideos(data, 'UCbbbbbbbbbbbbbbbbbbbbbb'));
});
test('translation restoration must complete before reading native language evidence', async () => {
  const calls = [];
  const page = { goto: async () => {}, wait: async () => {}, evaluate: async source => {
    calls.push(source); throw new Error('translation remains');
  } };
  await assert.rejects(readYoutubeLanguage(page, video, channel, function restore() {}));
  assert.equal(calls.length, 1);
  assert.ok(calls[0].includes('restore'));
});
