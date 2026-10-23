import {restoreOriginalLanguage} from './original-language.mjs';

export function instagramPostTarget(source) {
  const match = typeof source === 'string' && /^https:\/\/www\.instagram\.com\/(?:([A-Za-z0-9_.]{1,30})\/)?(p|reel)\/([A-Za-z0-9_-]{1,32})\/$/.exec(source);
  if (!match) throw new Error('ONECREW_COMMENTS_INPUT');
  return {username: match[1] ?? null, code: match[3]};
}

export function projectInstagramComments(data, source, code, expectedUsername, limit) {
  const invalid = () => {throw new Error('ONECREW_INSTAGRAM_COMMENTS_INVALID');};
  const profile = value => {
    let url;
    try {url = new URL(value, 'https://www.instagram.com');} catch {invalid();}
    const match = /^\/([A-Za-z0-9_.]{1,30})\/$/.exec(url.pathname);
    if (url.origin !== 'https://www.instagram.com' || url.username || url.password || url.search || url.hash || !match) invalid();
    return {author: match[1], author_url: url.href};
  };
  if (!Number.isInteger(limit) || limit < 1 || limit > 20 || typeof data?.caption !== 'string'
      || !data.caption.trim() || data.caption.includes('\0') || !Array.isArray(data.comments) || data.comments.length > 100) invalid();
  const author = profile(data.author_url);
  if (expectedUsername && author.author !== expectedUsername) invalid();
  const text = value => {
    if (typeof value !== 'string' || !value.trim() || value.includes('\0')) invalid();
    const chars = Array.from(value);
    return {text: chars.slice(0, 2000).join(''), text_truncated: chars.length > 2000};
  };
  const rows = [{type:'POST', id:code, native_author_id:null, ...author, source_url:source,
    identity_source:'visible_profile_link', ...text(data.caption)}];
  const seen = new Set();
  for (const comment of data.comments) {
    let url;
    try {url = new URL(comment.source_url, 'https://www.instagram.com');} catch {invalid();}
    const match = /^\/p\/([A-Za-z0-9_-]{1,32})\/c\/([1-9][0-9]{0,24})\/$/.exec(url.pathname);
    if (url.origin !== 'https://www.instagram.com' || url.username || url.password || url.search || url.hash || !match || match[1] !== code) invalid();
    if (seen.has(match[2])) invalid();
    seen.add(match[2]);
    rows.push({type:'COMMENT', id:match[2], source_url:url.href, ...text(comment.text)});
    if (rows.length === limit + 1) break;
  }
  return rows;
}

export async function readInstagramCommentPage(source, code, username, limit, project, restore) {
  const check = () => {
    if (location.origin !== 'https://www.instagram.com') throw new Error('ONECREW_INSTAGRAM_SOURCE_INVALID');
    if (/^\/accounts\/login/.test(location.pathname)) throw new Error('ONECREW_COMMENTS_AUTH');
    if (/^\/(challenge|checkpoint)\//.test(location.pathname)) throw new Error('ONECREW_COMMENTS_RISK');
    const current = /\/(?:p|reel)\/([A-Za-z0-9_-]+)\/?$/.exec(location.pathname);
    if (current?.[1] !== code) throw new Error('ONECREW_INSTAGRAM_SOURCE_INVALID');
  };
  for (let attempt = 0; attempt < 24; attempt++) {
    check();
    const articles = [...document.querySelectorAll('article')].filter(n => n.getClientRects().length && n.querySelector('h1'));
    if (articles.length > 1) throw new Error('ONECREW_INSTAGRAM_SOURCE_AMBIGUOUS');
    const article = articles[0];
    // Standalone posts currently use <main> without article/h1/h3. Bind each
    // text block to its own time + profile header, never the recommendation grid.
    if (!article) {
      const main = document.querySelector('main');
      if (main) {
        await restore();
        check();
        const comments = [], captions = [];
        for (const time of [...main.querySelectorAll('time')].slice(0, 101)) {
          const locator = time.closest('a[href]');
          const href = locator?.getAttribute('href');
          if (href && !href.startsWith(`/p/${code}/c/`)) continue;
          let node = time.parentElement;
          for (let depth = 0; depth < 6 && node && node !== main; depth++, node = node.parentElement) {
            const children = [...node.children];
            if (children.length !== 2 || !children[0].contains(time) || children[1].contains(time)) continue;
            const author = [...children[0].querySelectorAll('a[href]')].find(a => /^\/[A-Za-z0-9_.]{1,30}\/$/.test(a.getAttribute('href')));
            if ((!href && !author) || !children[1].innerText?.trim()) continue;
            const row = {text:children[1].innerText, source_url:href};
            if (!href) row.author_url = author.getAttribute('href');
            if (href) comments.push(row); else captions.push(row);
            break;
          }
        }
        if (captions.length > 1) throw new Error('ONECREW_INSTAGRAM_SOURCE_AMBIGUOUS');
        if (captions.length === 1 && comments.length) return project({caption:captions[0].text, author_url:captions[0].author_url, comments}, source, code, username, limit);
      }
    }
    if (article && !article.querySelector('[role="status"]')) {
      await restore();
      check();
      const caption = article.querySelector('h1');
      const author = caption.parentElement.querySelector('h2 a[href]');
      const comments = [...article.querySelectorAll('h3')].slice(0, 100).map(heading => {
        const content = heading.parentElement;
        return {text:heading.nextElementSibling?.querySelector('span[dir="auto"]')?.innerText,
          source_url:content.querySelector('a[href*="/c/"]')?.getAttribute('href')};
      });
      // Without a loaded comment or explicit empty state, absence stays unknown.
      if (comments.length) return project({caption:caption.innerText, author_url:author?.getAttribute('href'), comments}, source, code, username, limit);
    }
    if (attempt < 23) await new Promise(resolve => setTimeout(resolve, 500));
  }
  throw new Error('ONECREW_INSTAGRAM_COMMENTS_UNAVAILABLE');
}

export async function readInstagramComments(page, args, ownPage) {
  const target = instagramPostTarget(args.id), limit = args.limit ?? 20;
  if (!Number.isInteger(limit) || limit < 1 || limit > 20) throw new Error('ONECREW_COMMENTS_INPUT');
  return ownPage(page, async owned => {
    await owned.goto(args.id, {settleMs:3000});
    const result = await owned.evaluate(`(${readInstagramCommentPage.toString()})(${JSON.stringify(args.id)}, ${JSON.stringify(target.code)}, ${JSON.stringify(target.username)}, ${limit}, ${projectInstagramComments.toString()}, ${restoreOriginalLanguage.toString()})`);
    return result && 'session' in Object(result) && 'data' in Object(result) ? result.data : result;
  });
}
