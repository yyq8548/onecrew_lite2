import {restoreOriginalLanguage} from './original-language.mjs';
import {readFacebookReelComments} from './facebook-reel-comments.mjs';

export function facebookPostTarget(source) {
  const match = typeof source === 'string' && /^https:\/\/www\.facebook\.com\/([A-Za-z0-9.]{1,100})\/posts\/(pfbid[A-Za-z0-9]{10,150}|[1-9][0-9]{0,24})\/?$/.exec(source);
  if (!match) throw new Error('ONECREW_COMMENTS_INPUT');
  return {author:match[1], post:match[2], source:source.replace(/\/$/, '')};
}

export function projectFacebookComments(data, source, author, limit) {
  const invalid = () => {throw new Error('ONECREW_FACEBOOK_COMMENTS_INVALID');};
  if (!Number.isInteger(limit) || limit < 1 || limit > 20 || !Array.isArray(data?.comments) || data.comments.length > 100) invalid();
  const text = (value, max) => {
    if (typeof value !== 'string' || !value.trim() || value.includes('\0')) invalid();
    const chars = Array.from(value);
    return {text:chars.slice(0,max).join(''),text_truncated:chars.length > max};
  };
  let profile;
  try {profile = new URL(data.author_url);} catch {invalid();}
  if (profile.origin !== 'https://www.facebook.com' || profile.username || profile.password
      || profile.pathname.replace(/\/$/,'').toLowerCase() !== `/${author}`.toLowerCase()) invalid();
  const rows = [{type:'POST',source_url:source,author_url:`https://www.facebook.com/${author}`,
    identity_source:'visible_profile_link',...text(data.text,8000)}];
  const seen = new Set();
  for (const comment of data.comments) {
    let url;
    try {url = new URL(comment.source_url);} catch {invalid();}
    const id = url.searchParams.get('comment_id');
    if (url.origin !== 'https://www.facebook.com' || url.username || url.password || url.hash
        || `${url.origin}${url.pathname.replace(/\/$/,'')}` !== source
        || !/^[1-9][0-9]{0,24}$/.test(id ?? '') || url.searchParams.has('reply_comment_id')) invalid();
    if (seen.has(id)) continue;
    seen.add(id);
    rows.push({type:'COMMENT',source_url:`${source}?comment_id=${id}`,...text(comment.text,2000)});
    if (rows.length === limit + 1) break;
  }
  return rows;
}

export async function readFacebookCommentPage(source, author, limit, project, restore) {
  const check = () => {
    if (location.origin !== 'https://www.facebook.com') throw new Error('ONECREW_FACEBOOK_SOURCE_INVALID');
    if (/^\/(?:login|recover)(?:\/|\.php|$)/.test(location.pathname)) throw new Error('ONECREW_COMMENTS_AUTH');
    if (/^\/(?:checkpoint|challenge|two_step_verification)(?:\/|$)/.test(location.pathname)) throw new Error('ONECREW_COMMENTS_RISK');
    if (`${location.origin}${location.pathname.replace(/\/$/,'')}` !== source) throw new Error('ONECREW_FACEBOOK_SOURCE_INVALID');
  };
  for (let attempt=0; attempt<24; attempt++) {
    check();
    // Facebook nests two dialogs around a post. Select the innermost one with
    // a post message, excluding notifications, composer, and background feed.
    const roots = [...document.querySelectorAll('[role="dialog"]')].filter(n => n.getClientRects().length
      && n.querySelector('[data-ad-preview="message"]')
      && ![...n.querySelectorAll('[role="dialog"]')].some(d => d.querySelector('[data-ad-preview="message"]')));
    if (roots.length > 1) throw new Error('ONECREW_FACEBOOK_SOURCE_AMBIGUOUS');
    const root = roots[0];
    if (root) {
      await restore();
      check();
      const messages = root.querySelectorAll('[data-ad-preview="message"]');
      const authorLink = root.querySelector('h3 a[href]');
      const comments = [];
      for (const article of [...root.querySelectorAll('[role="article"]')].slice(0,100)) {
        if (article.parentElement.closest('[role="article"]') || !article.getClientRects().length) continue;
        const blocks = [...article.querySelectorAll('div[dir="auto"]')].filter(n => !n.querySelector('div[dir="auto"]') && !n.closest('[role="button"],a'));
        const links = [...article.querySelectorAll('a[href]')].filter(a => {
          const u = new URL(a.href);
          return `${u.origin}${u.pathname.replace(/\/$/,'')}` === source && /^[1-9][0-9]{0,24}$/.test(u.searchParams.get('comment_id') ?? '') && !u.searchParams.has('reply_comment_id');
        });
        if (blocks.length === 1 && links.length === 1) comments.push({text:blocks[0].innerText,source_url:links[0].href});
      }
      // Missing comments is not proof that the post has none. Do not fabricate
      // a valid empty response or expand replies to make a read succeed.
      if (messages.length === 1 && authorLink && comments.length)
        return project({text:messages[0].innerText,author_url:authorLink.href,comments},source,author,limit);
    }
    if (attempt<23) await new Promise(resolve => setTimeout(resolve,500));
  }
  throw new Error('ONECREW_FACEBOOK_COMMENTS_UNAVAILABLE');
}

export async function readFacebookComments(page,args,ownPage) {
  if (typeof args.id === 'string' && /^https:\/\/www\.facebook\.com\/reel\//.test(args.id))
    return readFacebookReelComments(page,args,ownPage);
  const target=facebookPostTarget(args.id), limit=args.limit ?? 20;
  if (!Number.isInteger(limit) || limit<1 || limit>20) throw new Error('ONECREW_COMMENTS_INPUT');
  return ownPage(page,async owned => {
    await owned.goto(target.source,{settleMs:3000});
    const result=await owned.evaluate(`(${readFacebookCommentPage.toString()})(${JSON.stringify(target.source)},${JSON.stringify(target.author)},${limit},${projectFacebookComments.toString()},${restoreOriginalLanguage.toString()})`);
    return result && 'session' in Object(result) && 'data' in Object(result) ? result.data : result;
  });
}
