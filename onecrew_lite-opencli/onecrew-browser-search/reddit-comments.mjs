// Fixed read of the original JSON; no translated DOM, expansion, or pagination.
export async function fetchRedditComments(postId, limit) {
  if (!['https://www.reddit.com', 'https://reddit.com'].includes(location.origin)) return {stop: 'read'};
  const path = `/comments/${postId}.json`;
  const response = await fetch(`${path}?sort=best&limit=${limit}&depth=1&raw_json=1`, {credentials: 'include'});
  const url = new URL(response.url);
  if (['https://www.reddit.com', 'https://reddit.com'].includes(url.origin)
      && /^\/login(?:\/|$)/.test(url.pathname)) return {stop: 'auth'};
  if (url.origin !== location.origin || url.pathname !== path) return {stop: 'read'};
  if (response.status === 401) return {stop: 'auth'};
  if ([403, 404].includes(response.status)) return {stop: 'inaccessible'};
  if (response.status === 429) return {stop: 'risk'};
  if (!response.ok) return {stop: 'read'};
  return response.json();
}

export function projectRedditComments(payload, postId, limit) {
  if (payload?.stop === 'auth') throw new Error('ONECREW_COMMENTS_AUTH');
  if (payload?.stop === 'risk') throw new Error('ONECREW_COMMENTS_RISK');
  if (payload?.stop) throw new Error(`ONECREW_REDDIT_${payload.stop === 'inaccessible' ? 'INACCESSIBLE' : 'READ_FAILED'}`);
  const invalid = () => { throw new Error('ONECREW_REDDIT_INVALID'); };
  if (!/^[a-z0-9]{1,15}$/.test(postId) || !Number.isInteger(limit) || limit < 1 || limit > 20) invalid();
  const posts = payload?.[0]?.data?.children;
  const comments = payload?.[1]?.data?.children;
  if (!Array.isArray(payload) || payload.length !== 2 || payload.some(p => p?.kind !== 'Listing')
      || !Array.isArray(posts) || posts.length !== 1 || posts[0]?.kind !== 't3'
      || !Array.isArray(comments) || comments.length > 100) invalid();
  const post = posts[0].data;
  if (post?.id !== postId || post.name !== `t3_${postId}` || typeof post.title !== 'string'
      || typeof post.selftext !== 'string') invalid();
  const authorId = row => typeof row.author === 'string' && !['[deleted]', '[removed]'].includes(row.author)
    && /^t2_[a-z0-9]{1,15}$/.test(row.author_fullname ?? '') ? row.author_fullname : null;
  const text = (value, max) => ({text: Array.from(value).slice(0, max).join(''), text_truncated: Array.from(value).length > max});
  const row = (data, type, source, value, max) => ({
    type, id: data.name,
    ...(type === 'POST' ? {native_author_id: authorId(data), author: typeof data.author === 'string' ? data.author.slice(0, 200) : ''} : {}),
    score: Number.isSafeInteger(data.score) ? data.score : null,
    ...(type==='POST'?{observations:{published_at:Number.isFinite(data.created_utc)&&data.created_utc>0?new Date(data.created_utc*1000).toISOString():null,
      score:Number.isSafeInteger(data.score)?data.score:null,comment_count:Number.isSafeInteger(data.num_comments)&&data.num_comments>=0?data.num_comments:null,
      text_completeness:Array.from(value).length>max?'truncated':'complete'}}:{}),
    source_url: source, ...text(value, max),
  });
  const source = `https://www.reddit.com/comments/${postId}/`;
  const rows = [row(post, 'POST', source, post.title + '\n\n' + post.selftext, 4000)];
  const seen = new Set();
  for (const entry of comments) {
    if (entry?.kind === 'more') continue;
    const data = entry?.data;
    if (entry?.kind !== 't1' || !/^[a-z0-9]{1,15}$/.test(data?.id ?? '')
        || data.name !== `t1_${data.id}` || data.link_id !== post.name || data.parent_id !== post.name
        || typeof data.body !== 'string' || seen.has(data.id)) invalid();
    seen.add(data.id);
    // Removed content is not a language observation. Never read nested replies.
    if (['[deleted]', '[removed]'].includes(data.body)) continue;
    rows.push(row(data, 'L0', `${source}_/${data.id}/`, data.body, 2000));
    if (rows.length === limit + 1) break;
  }
  return rows;
}
