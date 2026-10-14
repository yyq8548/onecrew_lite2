import { fetchRedditComments, projectRedditComments } from './reddit-comments.mjs';
// These are read commands, never the similarly named comment/reply write commands.
export const commentReaders = Object.freeze([
  ['bilibili', 'comments'], ['zhihu', 'answer-comments'],
  ['weibo', 'comments'], ['reddit', 'read'],
]);

export function boundedCommentArgs(site, args) {
  const limit = args.limit ?? 20;
  if (!Number.isInteger(limit) || limit < 1 || limit > 20) throw new Error('Comment limit must be 1..20');
  // Bound nested traversal as well as the number of top-level rows.
  return { ...args, limit, ...(site === 'reddit'
    ? { depth: 1, replies: 1, 'max-length': 2000, 'expand-more': false }
    : site === 'zhihu' ? { 'replies-limit': 0 } : {}) };
}

export async function readComments(page, args, site, native, ownPage) {
  if (!commentReaders.some(([s, n]) => s === site && n === native.name)
      || native.access !== 'read' || typeof native.func !== 'function') {
    throw new Error('Reviewed comment reader unavailable');
  }
  const bounded = boundedCommentArgs(site, args);
  if (site === 'reddit' && !/^[a-z0-9]{1,15}$/.test(String(bounded['post-id'] ?? '')))
    throw new Error('ONECREW_COMMENTS_INPUT');
  if (site === 'weibo' && !/^[1-9][0-9]{0,19}$/.test(String(bounded.id ?? '')))
    throw new Error('ONECREW_COMMENTS_INPUT');
  return ownPage(page, async owned => {
    // Some upstream readers assume navigateBefore has already selected their domain.
    await owned.goto(`https://${native.domain}/`);
    if (site === 'reddit') {
      const postId = String(bounded['post-id']);
      const response = await owned.evaluate(`(${fetchRedditComments.toString()})(${JSON.stringify(postId)}, ${bounded.limit})`);
      const data = response && 'session' in Object(response) && 'data' in Object(response) ? response.data : response;
      return projectRedditComments(data, postId, bounded.limit);
    }
    if (site === 'weibo') {
      // The upstream display projection discards both comment and user IDs.
      // Read the same fixed endpoint once, preserving those native identities.
      const response = await owned.evaluate(`(${fetchWeiboComments.toString()})(${JSON.stringify(String(bounded.id))}, ${bounded.limit})`);
      const data = response && 'session' in Object(response) && 'data' in Object(response) ? response.data : response;
      return projectWeiboComments(data, bounded.limit);
    }
    if (site !== 'zhihu') return native.func(owned, bounded);
    // Keep native IDs from the same read before upstream drops them for display.
    const authors = new Map();
    const observed = new Proxy(owned, {get(object, key) {
      if (key === 'evaluate') return async (...params) => {
        const result = await object.evaluate(...params);
        captureZhihuCommentAuthors(authors, params[1], bounded.id, result);
        return result;
      };
      const value = Reflect.get(object, key);
      return typeof value === 'function' ? value.bind(object) : value;
    }});
    const rows = await native.func(observed, bounded);
    return rows.map(row => ({...row, native_author_id: authors.get(String(row.id)) ?? null}));
  });
}

// Runs only in the owned Weibo page. API text is original content, independent
// of page translation. No replies expansion, pagination, or second request.
export async function fetchWeiboComments(id, limit) {
  if (location.origin === 'https://passport.weibo.com') return { stop: 'auth' };
  if (location.origin !== 'https://weibo.com') return { stop: 'read' };
  const response = await fetch('/ajax/statuses/buildComments?flow=0&is_reload=1&id='
    + id + '&is_show_bulletin=2&is_mix=0&count=' + limit, { credentials: 'include' });
  const url = new URL(response.url);
  if (url.origin === 'https://passport.weibo.com'
      || (url.origin === location.origin && /^\/login(?:\/|$)/.test(url.pathname))) return { stop: 'auth' };
  if (url.origin !== location.origin || url.pathname !== '/ajax/statuses/buildComments') return { stop: 'read' };
  if (response.status === 401) return { stop: 'auth' };
  if (response.status === 403 || response.status === 429) return { stop: 'risk' };
  if (!response.ok) return { stop: 'read' };
  return response.json();
}

export function projectWeiboComments(payload, limit) {
  if (payload?.stop === 'auth') throw new Error('ONECREW_COMMENTS_AUTH');
  if (payload?.stop === 'risk') throw new Error('ONECREW_COMMENTS_RISK');
  if (![1, true].includes(payload?.ok) || !Array.isArray(payload.data)
      || !Number.isInteger(limit) || limit < 1 || limit > 20 || payload.data.length > 20)
    throw new Error('ONECREW_COMMENTS_INVALID: status='
      + (payload?.ok === undefined ? 'missing' : [1, true].includes(payload.ok) ? 'success' : 'rejected')
      + '; data=' + (Array.isArray(payload?.data) ? 'array' : 'missing')
      + '; rows=' + (Array.isArray(payload?.data) ? payload.data.length : 0));
  const id = object => {
    const fromString = typeof object?.idstr === 'string' ? object.idstr : null;
    const other = typeof object?.id === 'string' ? object.id
      : Number.isSafeInteger(object?.id) ? String(object.id) : null;
    if (fromString !== null && other !== null && fromString !== other)
      throw new Error('ONECREW_COMMENTS_ID_CONFLICT');
    const value = fromString ?? other;
    return typeof value === 'string' && /^[1-9][0-9]{0,19}$/.test(value) ? value : null;
  };
  const strip = html => html.replace(/<[^>]+>/g, '').replace(/&nbsp;/g, ' ')
    .replace(/&lt;/g, '<').replace(/&gt;/g, '>').replace(/&amp;/g, '&').trim();
  const seen = new Set();
  // This endpoint can ignore count=5 and return its fixed page of 20. Do not
  // paginate: retain only the requested sample within the reviewed page bound.
  return payload.data.slice(0, limit).map((comment, index) => {
    const nativeId = id(comment);
    if (!nativeId || seen.has(nativeId)) throw new Error('ONECREW_COMMENTS_INVALID_ID');
    seen.add(nativeId);
    if (typeof comment.text_raw !== 'string' && typeof comment.text !== 'string')
      throw new Error('ONECREW_COMMENTS_INVALID_TEXT');
    const text = typeof comment.text_raw === 'string' ? comment.text_raw : strip(comment.text);
    const characters = Array.from(text);
    const count = value => Number.isSafeInteger(value) && value >= 0 ? value : null;
    return {
      rank: index + 1, id: nativeId,
      native_author_id: comment.is_anonymous === true || comment.user?.is_anonymous === true
        ? null : id(comment.user),
      author: typeof comment.user?.screen_name === 'string' ? comment.user.screen_name.slice(0, 200) : '',
      text: characters.slice(0, 2000).join(''), text_truncated: characters.length > 2000,
      likes: count(comment.like_count), replies: count(comment.total_number),
      time: typeof comment.created_at === 'string' ? comment.created_at.slice(0, 100) : '',
    };
  });
}

export function captureZhihuCommentAuthors(authors, request, answerId, result) {
  if (!/^[1-9][0-9]{0,19}$/.test(String(answerId))) return;
  let url;
  try { url = new URL(request); } catch { return; }
  if (url.origin !== 'https://www.zhihu.com' || url.username || url.password
      || url.pathname !== `/api/v4/comment_v5/answers/${answerId}/root_comment`) return;
  const payload = result && 'session' in Object(result) && 'data' in Object(result) ? result.data : result;
  if (!Array.isArray(payload?.data)) return;
  if (payload.data.length > 20) throw new Error('Zhihu comment page exceeded reviewed limit');
  for (const comment of payload.data) {
    const id = typeof comment?.id === 'string' ? comment.id
      : Number.isSafeInteger(comment?.id) ? String(comment.id) : '';
    if (!/^[1-9][0-9]{0,19}$/.test(id)) throw new Error('Zhihu comment ID invalid');
    const author = comment.author?.member ?? comment.author;
    const nativeId = typeof author?.id === 'string' && /^[a-zA-Z0-9_-]{1,128}$/.test(author.id)
      && author.id !== '0' && author.is_anonymous !== true
      && comment.author?.is_anonymous !== true && comment.is_anonymous !== true ? author.id : null;
    if (authors.has(id) && authors.get(id) !== nativeId) throw new Error('Zhihu comment author conflict');
    authors.set(id, nativeId);
  }
}
