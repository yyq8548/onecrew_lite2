export function instagramRecentTarget(username, authorId) {
  if (typeof username !== 'string' || !/^[a-z0-9_.]{1,30}$/.test(username)
    || typeof authorId !== 'string' || !/^[1-9][0-9]{0,24}$/.test(authorId)) throw new Error('ONECREW_INSTAGRAM_RECENT_INPUT');
}

export function projectInstagramRecent(payload, username, authorId) {
  const invalid = reason => { throw new Error('ONECREW_INSTAGRAM_RECENT_INVALID_' + reason); };
  const numeric = value => typeof value === 'string' && /^[1-9][0-9]{0,24}$/.test(value) ? value
    : Number.isSafeInteger(value) && value > 0 ? String(value) : null;
  if (payload?.status !== 'ok' || !Array.isArray(payload.items) || payload.items.length > 20) invalid('ENVELOPE');
  const seen = new Set();
  const codes = new Set();
  return payload.items.slice(0, 3).map(item => {
    const owner = item?.user;
    if (numeric(owner?.pk) !== authorId || owner?.username?.toLowerCase() !== username) invalid('OWNER');
    const composite = typeof item.id === 'string' ? item.id.split('_') : [];
    const postId = numeric(item.pk) ?? (composite.length === 2 && composite[1] === authorId ? numeric(composite[0]) : null);
    if (!postId || (item.pk != null && numeric(item.pk) !== postId)
      || (item.id != null && item.id !== `${postId}_${authorId}`)
      || typeof item.code !== 'string' || !/^[A-Za-z0-9_-]{1,32}$/.test(item.code)
      || seen.has(postId) || codes.has(item.code)) invalid('POST');
    seen.add(postId);
    codes.add(item.code);
    const caption = item.caption;
    if (caption != null && (typeof caption.text !== 'string' || caption.text.includes('\0')
      || (caption.user != null && numeric(caption.user.pk) !== authorId))) invalid('CAPTION');
    const original = caption?.text ?? '';
    let text = '', size = 0;
    for (const char of original) {
      const bytes = new TextEncoder().encode(char).length;
      if (size + bytes > 8000) break;
      text += char; size += bytes;
    }
    return {post_id:postId,author_id:authorId,source_url:`https://www.instagram.com/${item.product_type === 'clips' ? 'reel' : 'p'}/${item.code}/`,
      text,text_truncated:text !== original};
  });
}

export async function fetchInstagramRecent(username) {
  if (location.origin !== 'https://www.instagram.com') return {stop:'source_page_origin'};
  if (/^\/accounts\/login/.test(location.pathname)) return {stop:'auth'};
  if (/^\/(challenge|checkpoint)\//.test(location.pathname)) return {stop:'risk'};
  const path = '/api/v1/feed/user/' + encodeURIComponent(username) + '/username/';
  let response;
  try {
    response = await fetch(path + '?count=3', {credentials:'include',headers:{'X-IG-App-ID':'936619743392459'},signal:AbortSignal.timeout(8000)});
  } catch (error) { return {stop:error?.name === 'TimeoutError' ? 'timeout' : 'read'}; }
  const url = new URL(response.url);
  if (url.origin !== location.origin) return {stop:'source_response_origin'};
  if (response.status === 401 || /^\/accounts\/login/.test(url.pathname)) return {stop:'auth'};
  if ([403,429].includes(response.status) || /^\/(challenge|checkpoint)\//.test(url.pathname)) return {stop:'risk'};
  if (!response.ok) return {stop:'http_' + response.status};
  if (url.pathname !== path) {
    const known = new Set(['api','v1','feed','user','users','accounts','onetap','login','username','info','web_profile_info']);
    const shape = url.pathname.split('/').filter(Boolean).map(segment => known.has(segment) ? segment : /^[0-9]+$/.test(segment) ? 'numeric' : 'other').join('_') || 'root';
    return {stop:url.pathname === path.slice(0,-1) ? 'source_response_slash' : 'source_response_path_' + shape};
  }
  try {
    const payload = await response.json();
    if (payload?.status === 'fail') return {stop:payload.message === 'login_required' ? 'auth' : 'read'};
    return {payload};
  } catch { return {stop:'json'}; }
}

export async function readInstagramRecent(page,args,ownPage) {
  instagramRecentTarget(args.username,args.author_id);
  return ownPage(page,async owned => {
    await owned.goto('https://www.instagram.com/');
    const response = await owned.evaluate(`(async()=>{
      const result = await (${fetchInstagramRecent.toString()})(${JSON.stringify(args.username)});
      return result.stop ? result : {rows:(${projectInstagramRecent.toString()})(result.payload,${JSON.stringify(args.username)},${JSON.stringify(args.author_id)})};
    })()`);
    const result = response && 'session' in Object(response) && 'data' in Object(response) ? response.data : response;
    if (result?.stop) throw new Error('ONECREW_INSTAGRAM_RECENT_' + result.stop.toUpperCase());
    if (!Array.isArray(result?.rows)) throw new Error('ONECREW_INSTAGRAM_RECENT_INVALID');
    return result.rows;
  });
}
