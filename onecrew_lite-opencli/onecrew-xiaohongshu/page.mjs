// 在用户已登录的 Chrome 中等待真实内容，读取当前页面，不调用独立浏览器。
export async function observeXiaohongshuPage(kind, target, limit) {
  const visible = node => !!node?.getClientRects().length && getComputedStyle(node).visibility !== 'hidden';
  const text = node => (node?.textContent || '').trim();
  const value = input => input?._value ?? input;
  const current = new URL(location.href);
  const expected = new URL(target);
  if (current.origin !== expected.origin || current.pathname.replace(/\/$/, '') !== expected.pathname.replace(/\/$/, '')
    || kind === 'search' && current.searchParams.get('keyword') !== expected.searchParams.get('keyword')) throw new Error('ONECREW_XIAOHONGSHU_SOURCE_INVALID');
  const stop = () => {
    const body = document.body?.innerText || '';
    const notices = [...document.querySelectorAll('[role="dialog"],.captcha-container,.verify-dialog,.login-container,.login-modal,.login-main,.error-container,.error-wrapper')].filter(visible).map(node => node.innerText).join('\n');
    const message = notices || (!document.querySelector('#noteContainer,.user-info,section.note-item') ? body : '');
    if (/请求太频繁|访问频次异常|安全验证|验证你是真人|操作频繁|访问受限|异常访问|captcha|verify you are human/i.test(message)
      || [...document.querySelectorAll('.captcha-container,.verify-dialog')].some(visible)) throw new Error('ONECREW_XIAOHONGSHU_RISK');
    if (location.pathname.startsWith('/login') || /电脑设备登录超限|登录后查看|登录后浏览/.test(message)
      || [...document.querySelectorAll('.login-container,.login-modal,.login-main')].some(node => visible(node) && /登录|扫码/.test(node.innerText))) throw new Error('ONECREW_XIAOHONGSHU_AUTH');
    if (/笔记不存在|笔记已删除|内容已删除|该内容无法展示|用户不存在/.test(message)) throw new Error('ONECREW_XIAOHONGSHU_UNAVAILABLE');
  };
  const noteLink = href => {
    const url = new URL(href, location.origin);
    const id = /^\/(?:explore|search_result)\/([a-f0-9]{24})$/.exec(url.pathname)?.[1];
    const token = url.searchParams.get('xsec_token');
    if (url.origin !== expected.origin || !id || !token) return null;
    return { id, url: `${url.origin}/explore/${id}?${new URLSearchParams({ xsec_token: token, xsec_source: 'pc_search' })}` };
  };
  const authorLink = href => {
    const url = new URL(href, location.origin), id = /^\/user\/profile\/([a-f0-9]{24})$/.exec(url.pathname)?.[1];
    return url.origin === expected.origin && id ? { author_id: id, author_url: `${url.origin}${url.pathname}` } : null;
  };
  const cards = () => {
    const sections = new Set(document.querySelectorAll('section.note-item'));
    if (!sections.size) for (const link of document.querySelectorAll('a[href*="/search_result/"],a[href*="/explore/"]')) {
      const section = link.closest('section'); if (section) sections.add(section);
    }
    const rows = [];
    for (const card of sections) {
      if (!visible(card) || card.classList.contains('query-note-item')) continue;
      const note = [...card.querySelectorAll('a[href]')].map(link => noteLink(link.href)).find(Boolean);
      const author = card.querySelector('a.author[href*="/user/profile/"]') || card.querySelector('a[href*="/user/profile/"]');
      const identity = author && authorLink(author.href);
      const title = text(card.querySelector('.title')) || text(card.querySelector('a[href*="/search_result/"] span'));
      const label = text(author?.querySelector('.name')) || text(author).replace(/\s*(?:\d{1,2}天前|\d+小时前|\d+分钟前|刚刚|昨天|前天|\d{1,2}-\d{1,2}|\d{4}-\d{1,2}-\d{1,2})$/u, '').trim();
      if (note && identity && title && label) rows.push({ ...note, ...identity, author: label, title });
    }
    return rows;
  };
  const read = () => {
    stop();
    if (kind === 'search') {
      const rows = cards();
      return rows.length ? rows : [...document.querySelectorAll('.search-empty-wrapper')].some(visible) ? [] : null;
    }
    if (kind === 'profile') {
      const user = window.__INITIAL_STATE__?.user, data = value(user?.userPageData);
      if (!data?.basicInfo || typeof data.basicInfo.nickname !== 'string' || !Array.isArray(data.interactions)) return null;
      if (data.result?.success === false) throw new Error('ONECREW_XIAOHONGSHU_UNAVAILABLE');
      const groups = value(user?.notes);
      return { observed_url: location.href, basic: JSON.parse(JSON.stringify(data.basicInfo)), interactions: JSON.parse(JSON.stringify(data.interactions)),
        posts: Array.isArray(groups) ? groups.flat().filter(Boolean).slice(0, 3).map(post => ({ id: post.id, title: post.noteCard?.displayTitle || '', author_id: post.noteCard?.user?.userId ?? null })) : [] };
    }
    const container = document.querySelector('#noteContainer') || document.querySelector('.note-container');
    const author = container?.querySelector('.author-wrapper a[href*="/user/profile/"]');
    const identity = author && authorLink(author.href);
    const description = container?.querySelector('#detail-desc');
    if (!identity || !description || !visible(container)) return null;
    const nativeStore = window.__INITIAL_STATE__?.note, details = value(nativeStore?.noteDetailMap);
    const requestedId = /^\/(?:explore|search_result)\/([a-f0-9]{24})$/.exec(current.pathname)?.[1];
    const native = details?.[requestedId]?.note;
    if (native?.noteId && native.noteId !== requestedId || native?.user?.userId && native.user.userId !== identity.author_id) throw new Error('ONECREW_XIAOHONGSHU_IDENTITY_INVALID');
    const expand = [...description.querySelectorAll('button,a,[role="button"]')].some(node => visible(node) && /展开|查看更多|阅读全文/.test(node.innerText));
    const nativeText = native?.noteId === requestedId && native?.user?.userId === identity.author_id && typeof native.desc === 'string' ? native.desc : null;
    return { ...identity, note_id: requestedId, author_label: text(container.querySelector('.author-wrapper .username,.author-wrapper .name')) || text(author),
      observed_url: location.href, title: text(container.querySelector('#detail-title')), text: nativeText ?? text(description),
      tags: [...description.querySelectorAll('a.tag,a[href*="/search_result?"]')].map(node => text(node)),
      text_truncated: nativeText === null && expand,
      text_completeness_known: true };
  };
  let observed;
  for (let poll = 0; poll < 40; poll++) {
    observed = read(); if (observed !== null) break;
    if (poll === 39) throw new Error('ONECREW_XIAOHONGSHU_PAGE_TIMEOUT');
    await new Promise(resolve => setTimeout(resolve, 250));
  }
  if (kind !== 'search' || !observed.length) return observed;
  const found = new Map(observed.map(row => [row.id, row]));
  let idle = 0, previousGeometry = '', lastCount = found.size;
  const deadline = Date.now() + 12000;
  while (found.size < limit && Date.now() < deadline && idle < 3) {
    const last = [...document.querySelectorAll('section.note-item')].filter(visible).at(-1);
    if (last) last.scrollIntoView({ block: 'end' }); else window.scrollBy(0, Math.max(600, innerHeight));
    await new Promise(resolve => setTimeout(resolve, 650));
    for (const row of read() || []) found.set(row.id, row);
    const geometry = `${document.scrollingElement?.scrollTop}:${document.scrollingElement?.scrollHeight}`;
    idle = found.size === lastCount && geometry === previousGeometry ? idle + 1 : 0;
    lastCount = found.size; previousGeometry = geometry;
  }
  return [...found.values()].slice(0, limit);
}
