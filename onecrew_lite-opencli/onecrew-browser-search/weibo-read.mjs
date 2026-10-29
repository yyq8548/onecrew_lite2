export function nativeWeiboId(value) {
  const stringId = typeof value?.idstr === 'string' ? value.idstr : null;
  const numericId = typeof value?.id === 'string' ? value.id
    : Number.isSafeInteger(value?.id) ? String(value.id) : null;
  if (stringId !== null && numericId !== null && stringId !== numericId)
    throw new Error('ONECREW_WEIBO_ID_CONFLICT');
  const id = stringId ?? numericId;
  if (typeof id !== 'string' || !/^[1-9][0-9]{0,19}$/.test(id))
    throw new Error('ONECREW_WEIBO_ID_INVALID');
  return id;
}

export function projectWeiboProfile(payload, expectedId) {
  const user = payload?.data?.user;
  if (![1, true].includes(payload?.ok) || !user || nativeWeiboId(user) !== expectedId
      || typeof user.screen_name !== 'string' || !user.screen_name.trim()
      || user.screen_name.length > 200) throw new Error('ONECREW_WEIBO_PROFILE_INVALID');
  const text = (value, limit) => typeof value === 'string' ? Array.from(value).slice(0, limit).join('') : '';
  const count = user.followers_count;
  if (user.description != null && typeof user.description !== 'string') throw new Error('ONECREW_WEIBO_PROFILE_INVALID');
  if (count != null && (!Number.isSafeInteger(count) || count < 0))
    throw new Error('ONECREW_WEIBO_FOLLOWERS_INVALID');
  return {id: expectedId, name: user.screen_name, follower_count: count ?? null,
    profile_url: `https://weibo.com/u/${expectedId}`,
    bio: text(user.description, 2000), bio_truncated: typeof user.description==='string' && [...user.description].length>2000,
    bio_status: typeof user.description!=='string'||!user.description?'missing':[...user.description].length>2000?'truncated':'complete', self_declared_location: text(user.location, 200)};
}

export function projectWeiboRecent(payload, expectedId) {
  const list = payload?.data?.list;
  if (payload?.ok === false || payload?.ok === 0 || payload?.error || !Array.isArray(list)
      || list.length > 50) throw new Error('ONECREW_WEIBO_RECENT_INVALID');
  const urls = new Set();
  for (const post of list) {
    // Only the original author's posts, never the embedded repost's author.
    if (post?.retweeted_status) continue;
    if (nativeWeiboId(post?.user) !== expectedId || typeof post.mblogid !== 'string'
        || !/^[A-Za-z0-9]{1,20}$/.test(post.mblogid)) throw new Error('ONECREW_WEIBO_RECENT_ID_INVALID');
    nativeWeiboId(post);
    urls.add(`https://weibo.com/${expectedId}/${post.mblogid}`);
    if (urls.size === 3) break;
  }
  return {author_id: expectedId, source_urls: [...urls]};
}

// Fixed read-only endpoints reviewed against OpenCLI 1.8.8. Exactly one page,
// no screen-name resolution, detail lookup, date range traversal, or pagination.
export async function fetchWeiboEvidence(id, kind) {
  if (location.origin === 'https://passport.weibo.com') return {stop: 'auth'};
  if (location.origin !== 'https://weibo.com') return {stop: 'read'};
  const path = kind === 'profile' ? '/ajax/profile/info' : '/ajax/statuses/searchProfile';
  const query = kind === 'profile' ? '?uid=' + id : '?uid=' + id + '&page=1&hasori=1&hasret=0';
  const response = await fetch(path + query, {credentials: 'include'});
  const url = new URL(response.url);
  if (url.origin === 'https://passport.weibo.com' || response.status === 401) return {stop: 'auth'};
  if (response.status === 403 || response.status === 429) return {stop: 'risk'};
  if (!response.ok || url.origin !== location.origin || url.pathname !== path) return {stop: 'read'};
  return response.json();
}

export async function readWeiboEvidence(page, args, kind, ownPage) {
  if (!['profile', 'recent'].includes(kind) || typeof args.id !== 'string'
      || !/^[1-9][0-9]{0,19}$/.test(args.id)) throw new Error('ONECREW_WEIBO_INPUT');
  return ownPage(page, async owned => {
    await owned.goto('https://weibo.com/');
    const result = await owned.evaluate(`(${fetchWeiboEvidence.toString()})(${JSON.stringify(args.id)}, ${JSON.stringify(kind)})`);
    const payload = result && 'session' in Object(result) && 'data' in Object(result) ? result.data : result;
    if (payload?.stop === 'auth') throw new Error('ONECREW_WEIBO_AUTH');
    if (payload?.stop === 'risk') throw new Error('ONECREW_WEIBO_RISK');
    return [kind === 'profile' ? projectWeiboProfile(payload, args.id) : projectWeiboRecent(payload, args.id)];
  });
}

export async function fetchWeiboPost(id, readId) {
  if (location.origin==='https://passport.weibo.com') throw new Error('ONECREW_WEIBO_AUTH');
  if (location.origin!=='https://weibo.com') throw new Error('ONECREW_WEIBO_SOURCE_INVALID');
  const get = async path => {
    const response = await fetch(path, {credentials:'include',signal:AbortSignal.timeout(15000)});
    if (response.status===401 || new URL(response.url).hostname==='passport.weibo.com') throw new Error('ONECREW_WEIBO_AUTH');
    if (response.status===403 || response.status===429) throw new Error(`ONECREW_WEIBO_RISK HTTP ${response.status}`);
    if (!response.ok) throw new Error(`ONECREW_WEIBO_HTTP HTTP ${response.status}`);
    return response.json();
  };
  const post = await get('/ajax/statuses/show?id='+encodeURIComponent(id));
  const postId = readId(post), authorId = readId(post.user);
  const plain = html => new DOMParser().parseFromString(html, 'text/html').body.textContent.trim();
  let text = typeof post.text_raw==='string' ? post.text_raw : plain(post.text ?? '');
  let completeness = post.isLongText===false || post.is_long_text===false ? 'complete' : 'unknown';
  if (post.isLongText || post.is_long_text) {
    const long = await get('/ajax/statuses/longtext?id='+postId);
    if (typeof long.data?.longTextContent!=='string' || !long.data.longTextContent.trim()) throw new Error('ONECREW_WEIBO_LONGTEXT_UNAVAILABLE');
    text=plain(long.data.longTextContent);completeness='complete';
  }
  const result = {id:postId,mblogid:post.mblogid,author:post.user.screen_name,text,created_at:post.created_at,
    source:plain(post.source ?? ''),reposts:post.reposts_count,comments:post.comments_count,likes:post.attitudes_count,pic_count:post.pic_num,
    url:`https://weibo.com/${authorId}/${post.mblogid}`,text_completeness:completeness};
  if (post.retweeted_status) {
    result.retweeted_from=post.retweeted_status.user?.screen_name ?? '';
    result.retweeted_text=post.retweeted_status.text_raw ?? plain(post.retweeted_status.text ?? '');
  }
  return Object.fromEntries(Object.entries(result).filter(([,value])=>value!==undefined));
}
