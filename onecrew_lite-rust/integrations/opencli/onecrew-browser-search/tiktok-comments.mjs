import {restoreOriginalLanguage} from './original-language.mjs';

export function projectTikTokPost(bootstrap, username, videoId) {
  const detail = bootstrap?.__DEFAULT_SCOPE__?.['webapp.video-detail'];
  const item = detail?.itemInfo?.itemStruct;
  if (detail?.statusCode !== 0 || item?.id !== videoId || item.author?.uniqueId !== username
      || typeof item.author.id !== 'string' || !/^[1-9][0-9]{0,24}$/.test(item.author.id)
      || typeof item.desc !== 'string' || item.desc.includes('\0')) throw new Error('ONECREW_TIKTOK_SOURCE_INVALID: bootstrap_identity');
  return {type: 'POST', id: videoId, native_author_id: item.author.id, author: username,
    source_url: `https://www.tiktok.com/@${username}/video/${videoId}`,
    text: Array.from(item.desc).slice(0, 4000).join(''), text_truncated: Array.from(item.desc).length > 4000};
}

// Language-only observations: retain text and this post's provenance, no commenter identity.
export function projectTikTokComments(comments, source, limit) {
  if (!Array.isArray(comments) || comments.length > 100 || !Number.isInteger(limit) || limit < 1 || limit > 20)
    throw new Error('ONECREW_TIKTOK_COMMENTS_INVALID');
  const rows = [], seen = new Set();
  for (const comment of comments) {
    if (typeof comment?.text !== 'string' || comment.text.includes('\0')) throw new Error('ONECREW_TIKTOK_COMMENTS_INVALID');
    if (!comment.text.trim()) continue;
    const key = comment.text;
    if (seen.has(key)) continue;
    seen.add(key);
    rows.push({type: 'COMMENT', source_url: source,
      text: Array.from(comment.text).slice(0, 2000).join(''), text_truncated: Array.from(comment.text).length > 2000});
    if (rows.length === limit) break;
  }
  return rows;
}

export async function readTikTokCommentPage(username, videoId, limit, projectPost, projectComments, restore) {
  const source = `https://www.tiktok.com/@${username}/video/${videoId}`;
  const check = () => {
    if (location.href !== source) throw new Error('ONECREW_TIKTOK_SOURCE_INVALID: navigation');
    const text = document.body?.innerText ?? '';
    if (/verify you are human|complete the puzzle|security verification|too many requests/i.test(text))
      throw new Error('ONECREW_COMMENTS_RISK');
    if (/(?:log|sign) in to (?:view|see) (?:this video|comments)/i.test(text))
      throw new Error('ONECREW_COMMENTS_AUTH');
  };
  check();
  const nodes = document.querySelectorAll('script#__UNIVERSAL_DATA_FOR_REHYDRATION__');
  if (nodes.length !== 1 || nodes[0].textContent.length > 4 * 1024 * 1024) throw new Error('ONECREW_TIKTOK_SOURCE_INVALID: bootstrap_shape');
  const post = projectPost(JSON.parse(nodes[0].textContent), username, videoId);
  // Hydration may expose the caption before the author and action bar exist.
  // Wait on this video only; never select a recommendation with a comments button.
  let trigger;
  for (let attempt = 0; attempt < 24; attempt++) {
    check();
    const first = document.querySelector('article');
    if (first?.querySelector(`a[href="/@${username}"]`)) {
      if (limit === 0) return [post];
      const candidate = first.querySelector('[data-e2e="comment-icon"]');
      if (candidate?.getClientRects().length) { trigger = candidate; break; }
    }
    if (attempt < 23) await new Promise(resolve => setTimeout(resolve, 500));
  }
  if (!trigger) throw new Error('ONECREW_TIKTOK_SOURCE_INVALID: visible_author');
  trigger.click(); // Open comments only; never Reply, Like, or the editor.
  for (let attempt = 0; attempt < 24; attempt++) {
    check();
    const roots = [...document.querySelectorAll('[class*="DivCommentListContainer"]')].filter(n => n.getClientRects().length);
    if (roots.length > 1) throw new Error('ONECREW_TIKTOK_COMMENTS_AMBIGUOUS');
    if (roots.length === 1 && roots[0].querySelector('[data-e2e="comment-level-1"]')) {
      await restore();
      check();
      const comments = [...roots[0].querySelectorAll('[data-e2e="comment-level-1"]')].slice(0, 100).map(node => ({
        text: node.innerText,
      }));
      return [post, ...projectComments(comments, source, limit)];
    }
    // An absent list is not evidence that the video has no comments.
    if (attempt < 23) await new Promise(resolve => setTimeout(resolve, 500));
  }
  throw new Error('ONECREW_TIKTOK_COMMENTS_UNAVAILABLE');
}

export async function readTikTokComments(page, args, ownPage) {
  const source = args.id;
  const match = typeof source === 'string' && /^https:\/\/www\.tiktok\.com\/@([A-Za-z0-9_.]{1,24})\/video\/([1-9][0-9]{0,24})$/.exec(source);
  const limit = args.limit ?? 20;
  if (!match || !Number.isInteger(limit) || limit < 0 || limit > 20) throw new Error('ONECREW_COMMENTS_INPUT');
  return ownPage(page, async owned => {
    await owned.goto(source, {settleMs: 3000});
    const result = await owned.evaluate(`(${readTikTokCommentPage.toString()})(${JSON.stringify(match[1])}, ${JSON.stringify(match[2])}, ${limit}, ${projectTikTokPost.toString()}, ${projectTikTokComments.toString()}, ${restoreOriginalLanguage.toString()})`);
    return result && 'session' in Object(result) && 'data' in Object(result) ? result.data : result;
  });
}
