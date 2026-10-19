import test from 'node:test';
import assert from 'node:assert/strict';
import vm from 'node:vm';
import { projectCards, searchBilibili } from './onecrew-bilibili/search.mjs';

const card = (changes = {}) => ({ video_urls: ['https://www.bilibili.com/video/BV1xx411c7mD/?from=search'],
  author_url: 'https://space.bilibili.com/12345?from=search', author: 'Example creator', title: '跨境电商教程', ...changes });

test('source projection binds exact BVID and author MID, never follower counts', () => {
  const [row] = projectCards([card(), card()], 5);
  assert.deepEqual(row, { adapter_version: '1.0.0', bvid: 'BV1xx411c7mD', author_id: '12345', author: 'Example creator',
    title: '跨境电商教程', url: 'https://www.bilibili.com/video/BV1xx411c7mD', author_url: 'https://space.bilibili.com/12345' });
  assert.equal(projectCards([card(), card()], 5).length, 1);
});

test('rejects ambiguous sources, identity loss, invalid text and unverified empty pages', () => {
  for (const change of [
    { author_url: 'https://evil.example/12345' }, { author_url: 'https://space.bilibili.com/0' },
    { video_urls: ['https://www.bilibili.com.evil.example/video/BV1xx411c7mD'] },
    { video_urls: ['https://user@www.bilibili.com/video/BV1xx411c7mD'] },
    { video_urls: ['https://www.bilibili.com/video/BV1xx411c7mD', 'https://www.bilibili.com/video/BV1xx411c7mE'] },
    { title: '' }, { author: 'bad\nname' }, { title: '字'.repeat(501) },
  ]) assert.throws(() => projectCards([card(change)], 5), /ONECREW_BILIBILI_INVALID/);
  assert.throws(() => projectCards([], 5), /ONECREW_BILIBILI_EMPTY_UNVERIFIED/);
  assert.throws(() => projectCards(Array(101).fill(card()), 5));
  assert.throws(() => projectCards([card(), card({ author_url: 'https://space.bilibili.com/54321' })], 5));
});

function fixture({ text = '', href, missingTitle = false, extraAuthor = false, missingCards = false, nonVideo } = {}) {
  const calls = [];
  const title = { getAttribute: () => '跨境电商教程', closest: () => ({ href: card().video_urls[0] }) };
  const author = { href: card().author_url, querySelectorAll: () => [{ textContent: 'Example creator' }] };
  const element = { querySelectorAll: selector => {
    if (selector === '.bili-video-card__info--tit[title]') return missingTitle ? [] : [title];
    if (selector === 'a.bili-video-card__info--owner') return extraAuthor ? [author, author] : [author];
    if (selector === 'a[href]') return [{ href: card().video_urls[0] }, { href: card().video_urls[0] }, author];
    assert.fail('Unexpected selector: ' + selector);
  } };
  const page = {
    goto: () => assert.fail('No goto fallback'),
    newTab: async url => { calls.push(['newTab', url]); return 'exact-target'; },
    setActivePage: async id => calls.push(['bind', id]),
    sleep: async seconds => calls.push(['sleep', seconds]),
    wait: async options => calls.push(['wait', options]),
    evaluate: async script => {
      if (script.includes('async function restoreOriginalLanguage()')) return true;
      calls.push(['read']);
      return vm.runInNewContext(script, { URL, TextEncoder,
        location: { href: href ?? calls[0][1] },
        document: { body: { innerText: text }, querySelectorAll: selector => {
          assert.equal(selector, '.video-list .bili-video-card');
          const other = { querySelectorAll: () => [{ closest: () => ({ href: nonVideo }) }] };
          return missingCards ? [] : nonVideo ? [other, element] : [element];
        } },
        fetch: () => assert.fail('No extra API requests'),
      });
    },
  };
  return { page, calls };
}

test('reads the bound search card once using encoded query, without script injection or API calls', async () => {
  const { page, calls } = fixture();
  const query = 'Shopify " ; throw new Error("injected") //';
  const rows = await searchBilibili(page, query, 5);
  assert.equal(rows[0].bvid, 'BV1xx411c7mD');
  assert.equal(new URL(calls[0][1]).searchParams.get('keyword'), query);
  assert.deepEqual(calls.slice(1), [['bind', 'exact-target'], ['sleep', 2],
    ['wait', { selector: '.video-list .bili-video-card', timeout: 10 }], ['read']]);
});

test('changed origin/query, challenge, login wall and incomplete cards stop without retry', async () => {
  for (const options of [
    { href: 'https://search.bilibili.com.evil.example/video?keyword=q' },
    { href: 'https://search.bilibili.com/video?keyword=other' },
    { text: '请完成安全验证' }, { text: '登录后查看搜索结果' },
    { missingTitle: true }, { extraAuthor: true }, { missingCards: true },
  ]) {
    const { page, calls } = fixture(options);
    await assert.rejects(searchBilibili(page, 'q', 5));
    assert.equal(calls.filter(([name]) => name === 'read').length, 1);
  }
});

test('explicit paid-redirect and course cards are excluded, not followed or counted as videos', async () => {
  for (const nonVideo of ['https://cm.bilibili.com/cm/api/fees/pc/sync/v2?tracking=ignored', 'https://www.bilibili.com/cheese/play/ss123']) {
    const { page, calls } = fixture({ nonVideo });
    const rows = await searchBilibili(page, 'q', 5);
    assert.equal(rows.length, 1);
    assert.equal(rows[0].bvid, 'BV1xx411c7mD');
    assert.equal(calls.length, 5);
  }
});

test('invalid inputs stop before any browser operation', async () => {
  for (const [query, limit] of [['', 5], ['q\n', 5], ['x'.repeat(161), 5], ['q', 0], ['q', 51]]) {
    const { page, calls } = fixture();
    await assert.rejects(searchBilibili(page, query, limit), /ONECREW_BILIBILI_INPUT/);
    assert.equal(calls.length, 0);
  }
});
