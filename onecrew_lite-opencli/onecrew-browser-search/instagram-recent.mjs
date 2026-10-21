import {observeInstagramVisibleProfile,observeInstagramVisiblePosts,observeInstagramVisibleCaption} from './instagram-visible.mjs';
import {sourceUrlScript} from './source-url.mjs';

export function selectInstagramVisiblePost(posts, query = '') {
  const terms = [...new Set(query.normalize('NFKC').toLowerCase().match(/[\p{L}\p{N}]{3,}/gu) || [])].filter(term => !['blogger','creator','influencer','account','official'].includes(term));
  const score = post => terms.reduce((sum, term) => sum + (post.preview?.normalize('NFKC').toLowerCase().includes(term) ? 1 : 0), 0);
  return posts.reduce((best, post) => !best || score(post) > score(best) ? post : best, null);
}

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
      text,text_truncated:text !== original,
      observations:{published_at:Number.isSafeInteger(item.taken_at)&&item.taken_at>0?new Date(item.taken_at*1000).toISOString():null,
        like_count:Number.isSafeInteger(item.like_count)&&item.like_count>=0?item.like_count:null,
        comment_count:Number.isSafeInteger(item.comment_count)&&item.comment_count>=0?item.comment_count:null,
        view_count:Number.isSafeInteger(item.play_count)&&item.play_count>=0?item.play_count:null,
        text_completeness:text!==original?'truncated':'complete'}};
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
    const unwrap = raw => raw && 'session' in Object(raw) && 'data' in Object(raw) ? raw.data : raw;
    let post;
    if (args.source_url) {
      const match = /^https:\/\/www\.instagram\.com\/([^/]+)\/(p|reel)\/([A-Za-z0-9_-]{1,32})\/$/.exec(args.source_url);
      if (!match || match[1] !== args.username) throw new Error('ONECREW_INSTAGRAM_RECENT_INPUT');
      post = {url:args.source_url,kind:match[2],code:match[3]};
    } else {
    await owned.goto(`https://www.instagram.com/${args.username}/`);
    const profile = unwrap(await owned.evaluate(sourceUrlScript(`(${observeInstagramVisibleProfile.toString()})(${JSON.stringify(args.username)},${JSON.stringify(args.author_id)})`)));
    if (profile?.stop) throw new Error('ONECREW_INSTAGRAM_RECENT_' + profile.stop.toUpperCase());
    if (profile?.profile?.private_account === true) return [];
    let observed;
    for (let poll = 0; poll < 24; poll++) {
      observed = unwrap(await owned.evaluate(sourceUrlScript(`(${observeInstagramVisiblePosts.toString()})(${JSON.stringify(args.username)})`)));
      if (observed?.stop) throw new Error('ONECREW_INSTAGRAM_RECENT_' + observed.stop.toUpperCase());
      if (observed?.posts?.length) break;
      if (poll < 23) await owned.sleep(0.25);
    }
    if (!Array.isArray(observed?.posts) || !observed.posts.length) throw new Error('ONECREW_INSTAGRAM_RECENT_POST_LINK_TIMEOUT');
    post = selectInstagramVisiblePost(observed.posts, args.query || '');
    }
    await owned.goto(post.url);
    const caption = unwrap(await owned.evaluate(`(${observeInstagramVisibleCaption.toString()})(${JSON.stringify(args.username)},${JSON.stringify(post.code)})`));
    if (caption?.stop) throw new Error('ONECREW_INSTAGRAM_RECENT_' + caption.stop.toUpperCase());
    if (typeof caption?.text !== 'string' || typeof caption.text_truncated !== 'boolean') throw new Error('ONECREW_INSTAGRAM_RECENT_INVALID');
    return [{post_id:post.code,author_id:args.author_id,source_url:`https://www.instagram.com/${post.kind}/${post.code}/`,...caption}];
  });
}
