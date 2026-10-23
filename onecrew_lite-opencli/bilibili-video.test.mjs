import assert from 'node:assert/strict';
import test from 'node:test';
import { projectVideo, readBilibiliVideo, readVideoPage } from './onecrew-bilibili-video/video.mjs';

const bvid = 'BV1bTYv6cELe';
const mid = '168310298';
const observation = {
  url: `https://www.bilibili.com/video/${bvid}/`,
  author_url: `https://space.bilibili.com/${mid}`,
  title: '【鞋子推荐】9双日系神鞋', visible_title: '【鞋子推荐】9双日系神鞋',
  author: '谈徽中', visible_author: '谈徽中', description: '9双日系神鞋', payment_marker: false,
};

test('projects only a matching native video and author; absent metrics are not filled in', () => {
  const rows = projectVideo(observation, bvid, mid);
  assert.equal(rows.length, 21);
  const fields = Object.fromEntries(rows.map(row => [row.field, row.value]));
  assert.equal(fields.bvid, bvid);
  assert.equal(fields.author, `谈徽中 (mid: ${mid})`);
  assert.equal(fields.description, '9双日系神鞋');
  assert.equal(fields.view, '');
  for (const change of [
    { author_url: 'https://space.bilibili.com/999' },
    { visible_title: 'other video' },
    { visible_author: 'other author' },
    { payment_marker: true },
  ]) assert.throws(() => projectVideo({ ...observation, ...change }, bvid, mid));
});

test('read command is bound to one validated video and author', async () => {
  const calls = [];
  const page = {
    newTab: async url => { calls.push(['tab', url]); return 'tab-1'; },
    setActivePage: async id => { calls.push(['active', id]); },
    closeTab: async id => { calls.push(['close', id]); },
    sleep: async seconds => { calls.push(['sleep', seconds]); },
    wait: async input => { calls.push(['wait', input.selector]); },
    evaluate: async script => {
      if (script.includes('async function restoreOriginalLanguage()')) return true;
      assert.match(script, /BV1bTYv6cELe/);
      return projectVideo(observation, bvid, mid);
    },
  };
  assert.equal((await readBilibiliVideo(page, bvid, mid)).length, 21);
  assert.deepEqual(calls, [
    ['tab', `https://www.bilibili.com/video/${bvid}/`],
    ['active', 'tab-1'], ['sleep', 2], ['wait', 'h1.video-title'], ['close', 'tab-1'],
  ]);
  await assert.rejects(readBilibiliVideo(page, '../video', mid));
  await assert.rejects(readBilibiliVideo(page, bvid, '0'));
});

test('failed video reads close their exact owned tab without another navigation', async () => {
  for (const operation of ['setActivePage', 'sleep', 'wait', 'evaluate']) {
    const closed = [];
    let opened = 0;
    const page = {
      newTab: async () => { opened++; return 'owned-video'; },
      setActivePage: async () => {}, sleep: async () => {}, wait: async () => {},
      evaluate: async () => [], closeTab: async id => closed.push(id),
    };
    page[operation] = async () => { throw new Error('video read failed'); };
    await assert.rejects(readBilibiliVideo(page, bvid, mid), /video read failed/);
    assert.equal(opened, 1);
    assert.deepEqual(closed, ['owned-video']);
  }
});

test('cleanup failure cannot replace a video risk error', async () => {
  const page = {
    newTab: async () => 'owned-video', setActivePage: async () => {},
    sleep: async () => {}, wait: async () => {},
    evaluate: async () => { throw new Error('ONECREW_BILIBILI_VIDEO_RISK'); },
    closeTab: async () => { throw new Error('cleanup unavailable'); },
  };
  await assert.rejects(readBilibiliVideo(page, bvid, mid), /ONECREW_BILIBILI_VIDEO_RISK/);
});

test('page observation checks JSON-LD against the visible author and title', () => {
  const originalDocument = globalThis.document;
  const originalLocation = globalThis.location;
  const metadata = { '@type': 'VideoObject', url: observation.url,
    author: { url: observation.author_url, name: observation.author },
    name: observation.title, description: observation.description };
  const title = { textContent: observation.title };
  const author = { href: `${observation.author_url}/?from=video`, textContent: observation.author };
  globalThis.location = { href: observation.url };
  globalThis.document = {
    body: { innerText: `${observation.title}\n请先登录或注册` },
    querySelectorAll: selector => {
      if (selector === 'script[type="application/ld+json"]') return [{ textContent: JSON.stringify(metadata) }];
      if (selector === 'h1.video-title') return [title];
      if (selector === 'a.up-name[href]') return [author];
      return [];
    },
    querySelector: () => null,
  };
  try {
    assert.equal(readVideoPage(bvid, mid, projectVideo).length, 21);
    author.href = 'https://space.bilibili.com/999/';
    assert.throws(() => readVideoPage(bvid, mid, projectVideo));
    author.href = `${observation.author_url}/`;
    metadata.url = 'https://www.bilibili.com/video/BV1xx411c7mD/';
    assert.throws(() => readVideoPage(bvid, mid, projectVideo));
  } finally {
    globalThis.document = originalDocument;
    globalThis.location = originalLocation;
  }
});
