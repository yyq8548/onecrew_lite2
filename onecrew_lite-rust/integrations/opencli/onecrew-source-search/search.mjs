// Visible search cards only. No unstable feed API, cookies or hidden page stores.
export function readCards(site, query, limit) {
  const host = site === 'nowcoder' ? 'www.nowcoder.com' : 'www.xiaohongshu.com';
  if (location.origin !== `https://${host}` || !location.pathname.startsWith('/search')
    || new URL(location.href).searchParams.get(site === 'nowcoder' ? 'query' : 'keyword') !== query)
    return {status:'wrong_page',rows:[]};
  const clean = value => (value || '').replace(/\s+/g,' ').trim();
  const visible = node => node && node.getClientRects().length > 0 && getComputedStyle(node).visibility !== 'hidden';
  const href = node => {try {const u = new URL(node.getAttribute('href'),location.origin);
    return u.protocol === 'https:' && u.hostname === host && !u.username && !u.password && !u.port ? u : null;
  } catch {return null;} };
  const body = document.body?.innerText || '';
  if (/请求太频繁|访问频次异常|安全限制|请完成安全验证/.test(body)
    || Array.from(document.querySelectorAll('.captcha-container,.verify-dialog')).some(visible))
    return {status:'risk',rows:[]};
  const walls = Array.from(document.querySelectorAll('.login-container,.login-modal,.login-main,[role="dialog"]')).filter(visible);
  if (/登录后查看搜索结果/.test(body) || walls.some(n=>/登录|扫码/.test(n.innerText))) return {status:'auth',rows:[]};
  if (site === 'nowcoder') {
    const search = Array.from(document.querySelectorAll('input[placeholder="搜索面经/职位/试题/公司"]')).find(visible);
    if (!search || clean(search.value) !== query) return {status:'wrong_page',rows:[]};
  }
  const rows = [], seen = new Set();
  const candidates = site === 'nowcoder'
    ? document.querySelectorAll('a[href*="/discuss/"],a[href*="/feed/main/detail/"]')
    : document.querySelectorAll('section.note-item,section:has(a[href*="/search_result/"]),section:has(a[href*="/explore/"])');
  for (const candidate of candidates) {
    if (!visible(candidate)) continue;
    let row;
    if (site === 'nowcoder') {
      // Observed search feed container; keeps each author bound to its own post.
      const card = candidate.closest('.tw-px-5.tw-relative');
      if (!card) continue;
      const post = href(candidate); if (!post) continue;
      const content = post.pathname.match(/^\/discuss\/([1-9]\d*)$/);
      const moment = post.pathname.match(/^\/feed\/main\/detail\/([a-f0-9]{32})$/i);
      if (!content && !moment) continue;
      const authors = Array.from(card.querySelectorAll('a[href*="/users/"]')).filter(visible)
        .map(a => ({node:a,url:href(a)})).filter(a => a.url && /^\/users\/[1-9]\d*$/.test(a.url.pathname));
      const identities = new Set(authors.map(a => a.url.pathname));
      if (identities.size !== 1) return {status:'invalid_author',rows:[]};
      const author = authors.find(a => clean(a.node.textContent));
      if (!author) return {status:'invalid_author',rows:[]};
      const title = clean(candidate.textContent); if (!title) continue;
      const id = (content || moment)[1];
      row = {post_type:content?'content':'moment',id,uuid:moment?id:'',title,
        author:clean(author.node.textContent),author_id:author.url.pathname.split('/').pop(),
        author_url:author.url.origin+author.url.pathname,url:post.origin+post.pathname};
    } else {
      if (candidate.classList.contains('query-note-item')) continue;
      const note = candidate.querySelector('a.cover.mask,a[href*="/search_result/"],a[href*="/explore/"]');
      const post = note && href(note); if (!post || !/^\/(search_result|explore)\/[a-f0-9]{24}$/i.test(post.pathname)) continue;
      const titleNode = candidate.querySelector('.title,.note-title,.footer .title span');
      const title = clean(titleNode?.textContent); if (!title) continue;
      const authorNode = candidate.querySelector('a.author,a[href*="/user/profile/"]');
      const authorUrl = authorNode && href(authorNode);
      if (!authorUrl || !/^\/user\/profile\/[a-f0-9]{24}$/i.test(authorUrl.pathname)) return {status:'invalid_author',rows:[]};
      row = {title,author:clean(authorNode.querySelector('.name')?.textContent || authorNode.textContent)
        .replace(/\s*(?:\d{1,2}-\d{1,2}|\d{4}-\d{1,2}-\d{1,2}|\d+天前|昨天|刚刚)$/u,'').trim(),
        author_id:authorUrl.pathname.split('/').pop(),author_url:authorUrl.origin+authorUrl.pathname,
        // The note security token is retained as an observed locator, never fabricated.
        url:post.href};
    }
    if (seen.has(row.url)) continue; seen.add(row.url); rows.push(row);
    if (rows.length >= limit) break;
  }
  if (rows.length) return {status:'content',rows};
  if (/没有找到相关|暂无搜索结果|没有搜索到|未找到相关/.test(body)) return {status:'empty',rows:[]};
  return {status:'loading',rows:[]};
}

export async function searchCards(page, site, query, limit) {
  if (site !== 'nowcoder' || typeof query !== 'string' || !query.trim()
    || [...query].length > 160 || /[\u0000-\u001f\u007f]/.test(query)
    || !Number.isInteger(limit) || limit < 1 || limit > 50) throw new Error('SOURCE_INPUT');
  const host = site === 'nowcoder' ? 'www.nowcoder.com' : 'www.xiaohongshu.com';
  const url = site === 'nowcoder' ? `https://${host}/search/all?type=all&query=${encodeURIComponent(query)}`
    : `https://${host}/search_result?keyword=${encodeURIComponent(query)}&source=web_search_result_notes`;
  // New owned tab avoids a reused blank/collapsed target. Never close a user tab.
  const identity = await page.newTab(url);
  if (!identity) throw new Error('SOURCE_TARGET');
  await page.setActivePage(identity);
  await page.sleep(2);
  try {
    // Bounded polling permits hydration; does not silently retry another profile or API.
    for (let attempt = 0; attempt < 12; attempt++) {
      await page.evaluate(`(${restoreOriginalLanguage.toString()})()`);
      const raw = await page.evaluate(`(${readCards.toString()})(${JSON.stringify(site)},${JSON.stringify(query)},${limit})`);
      const state = raw && 'session' in raw && 'data' in raw ? raw.data : raw;
      if (!state || !Array.isArray(state.rows) || state.rows.length > limit) throw new Error('SOURCE_SHAPE');
      if (state.status === 'content') return state.rows;
      if (state.status === 'empty') return [];
      if (state.status !== 'loading' && state.status !== 'wrong_page') throw new Error(`SOURCE_${state.status.toUpperCase()}`);
      await page.sleep(1);
    }
    throw new Error('SOURCE_TIMEOUT');
  } finally { await page.closeTab(identity); }
}
import { restoreOriginalLanguage } from '../onecrew-browser-search/original-language.mjs';
