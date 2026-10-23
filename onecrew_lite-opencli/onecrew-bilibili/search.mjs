// Bound to public search-result cards. No API calls, cookies, or follower claims.
export function projectCards(cards, limit) {
  const fail = () => { throw new Error('ONECREW_BILIBILI_INVALID'); };
  if (!Array.isArray(cards) || cards.length > 100 || !Number.isInteger(limit) || limit < 1 || limit > 50) fail();
  if (cards.length === 0) throw new Error('ONECREW_BILIBILI_EMPTY_UNVERIFIED');
  const sourceId = (raw, host, pattern) => {
    if (typeof raw !== 'string' || raw.length > 2048) fail();
    let url;
    try { url = new URL(raw); } catch { fail(); }
    if (url.protocol !== 'https:' || url.hostname !== host || url.username || url.password || url.port) fail();
    const match = url.pathname.match(pattern);
    if (!match) fail();
    return match[1];
  };
  const validText = (text, max) => typeof text === 'string' && text.trim().length > 0
    && [...text].length <= max && !/[\u0000-\u001f\u007f-\u009f]/.test(text);
  const rows = new Map();
  for (const card of cards) {
    if (!Array.isArray(card?.video_urls) || card.video_urls.length < 1 || card.video_urls.length > 8
        || !validText(card.title, 500) || !validText(card.author, 200)) fail();
    const ids = new Set(card.video_urls.map(url => sourceId(url, 'www.bilibili.com', /^\/video\/(BV[A-Za-z0-9]{10})\/?$/)));
    if (ids.size !== 1) fail();
    const bvid = [...ids][0];
    const authorId = sourceId(card.author_url, 'space.bilibili.com', /^\/([1-9][0-9]{0,19})\/?$/);
    const row = { adapter_version: '1.0.0', bvid, author_id: authorId, author: card.author.trim(), title: card.title.trim(),
      url: 'https://www.bilibili.com/video/' + bvid, author_url: 'https://space.bilibili.com/' + authorId };
    if (rows.has(bvid) && JSON.stringify(rows.get(bvid)) !== JSON.stringify(row)) fail();
    rows.set(bvid, row);
  }
  return [...rows.values()].slice(0, limit);
}

export function readSearchCards(query, limit, project) {
  const current = new URL(location.href);
  if (current.origin !== 'https://search.bilibili.com' || current.pathname !== '/video'
      || current.searchParams.getAll('keyword').length !== 1 || current.searchParams.get('keyword') !== query) {
    throw new Error('ONECREW_BILIBILI_ORIGIN');
  }
  const text = (document.body?.innerText ?? '').slice(0, 100000);
  if (/验证码|安全验证|操作频繁|访问受限|captcha|too many requests/i.test(text)) throw new Error('ONECREW_BILIBILI_RISK');
  if (/登录后(?:查看|搜索)|请先登录|sign in to search/i.test(text)) throw new Error('ONECREW_BILIBILI_AUTH');
  const cards = [...document.querySelectorAll('.video-list .bili-video-card')];
  if (cards.length > 100) throw new Error('ONECREW_BILIBILI_INVALID');
  const observations = cards.map(card => {
    const titles = card.querySelectorAll('.bili-video-card__info--tit[title]');
    if (titles.length !== 1) throw new Error('ONECREW_BILIBILI_INVALID_CARD');
    const titleLink = titles[0].closest('a[href]');
    if (!titleLink) throw new Error('ONECREW_BILIBILI_INVALID_TITLE_LINK');
    const destination = new URL(titleLink.href);
    // Observed search cards also contain paid redirects and courses. They are
    // not standard creator videos; never follow them or reinterpret their IDs.
    if (destination.protocol === 'https:' && !destination.username && !destination.password && !destination.port
        && ((destination.hostname === 'cm.bilibili.com' && destination.pathname === '/cm/api/fees/pc/sync/v2')
          || (destination.hostname === 'www.bilibili.com' && /^\/cheese\/play\/ss[1-9][0-9]*$/.test(destination.pathname)))) return null;
    const authors = card.querySelectorAll('a.bili-video-card__info--owner');
    if (authors.length !== 1) throw new Error('ONECREW_BILIBILI_INVALID_CARD');
    const names = authors[0].querySelectorAll('.bili-video-card__info--author');
    if (names.length !== 1) throw new Error('ONECREW_BILIBILI_INVALID_AUTHOR');
    const links = [...card.querySelectorAll('a[href]')];
    if (links.length > 20) throw new Error('ONECREW_BILIBILI_INVALID_LINKS');
    const videoUrls = links.map(link => link.href).filter(href => {
      try { return new URL(href).pathname.startsWith('/video/'); } catch { return false; }
    });
    if (!videoUrls.includes(titleLink.href)) throw new Error('ONECREW_BILIBILI_INVALID_TITLE_LINK');
    return { title: titles[0].getAttribute('title'), author: names[0].textContent,
      author_url: authors[0].href, video_urls: videoUrls };
  }).filter(card => card !== null);
  return project(observations, limit);
}

export async function searchBilibili(page, query, limit) {
  if (typeof query !== 'string' || !query.trim() || [...query].length > 160 || /[\u0000-\u001f\u007f-\u009f]/.test(query)
      || !Number.isInteger(limit) || limit < 1 || limit > 50) throw new Error('ONECREW_BILIBILI_INPUT');
  const identity = await page.newTab('https://search.bilibili.com/video?keyword=' + encodeURIComponent(query));
  if (!identity) throw new Error('ONECREW_BILIBILI_IDENTITY');
  await page.setActivePage(identity);
  // OpenCLI's numeric wait may return as soon as the initial blank document is
  // stable, before navigation commits. Give the new tab time to navigate, then
  // wait for actual result cards; a missing page remains an explicit failure.
  await page.sleep(2);
  await page.wait({ selector: '.video-list .bili-video-card', timeout: 10 });
  await page.evaluate(`(${restoreOriginalLanguage.toString()})()`);
  const result = await page.evaluate(`(${readSearchCards.toString()})(${JSON.stringify(query)}, ${limit}, ${projectCards.toString()})`);
  const rows = result && 'session' in result && 'data' in result ? result.data : result;
  if (!Array.isArray(rows) || rows.length > limit) throw new Error('ONECREW_BILIBILI_INVALID');
  return rows;
}
import { restoreOriginalLanguage } from '../onecrew-browser-search/original-language.mjs';
