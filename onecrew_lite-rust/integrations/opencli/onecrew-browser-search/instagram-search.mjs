export function projectInstagramSearch(payload, limit) {
  const invalid = () => { throw new Error('ONECREW_INSTAGRAM_SEARCH_INVALID'); };
  if (!Number.isInteger(limit) || limit < 1 || limit > 20 || !Array.isArray(payload?.users) || payload.users.length > 100) invalid();
  return payload.users.filter(entry => entry?.user?.is_private !== true).slice(0, limit).map(entry => {
    const user = entry?.user;
    const id = value => typeof value === 'string' && /^[1-9][0-9]{0,24}$/.test(value) ? value
      : Number.isSafeInteger(value) && value > 0 ? String(value) : null;
    const nativeId = id(user?.pk) ?? id(user?.id);
    if (!nativeId || (user.pk != null && user.id != null && id(user.pk) !== id(user.id))
      || typeof user.username !== 'string' || !/^[A-Za-z0-9_.]{1,30}$/.test(user.username)
      || typeof user.full_name !== 'string' || user.full_name.includes('\0')) invalid();
    const username = user.username.toLowerCase();
    return { provider: 'onecrew-instagram-native-v1', author_id: nativeId, username,
      name: Array.from(user.full_name).slice(0, 200).join(''), url: `https://www.instagram.com/${username}/` };
  });
}

export async function fetchInstagramSearch(query) {
  if (location.origin !== 'https://www.instagram.com') return { stop: 'read' };
  if (/^\/accounts\/login/.test(location.pathname)) return { stop: 'auth' };
  if (/^\/(challenge|checkpoint)\//.test(location.pathname)) return { stop: 'risk' };
  let response;
  try {
    response = await fetch('/web/search/topsearch/?query=' + encodeURIComponent(query) + '&context=user', {
      credentials: 'include', headers: { 'X-IG-App-ID': '936619743392459' }, signal: AbortSignal.timeout(8000),
    });
  } catch (error) { return { stop: error?.name === 'TimeoutError' ? 'timeout' : 'read' }; }
  const url = new URL(response.url);
  if (url.origin !== location.origin) return { stop: 'read' };
  if (response.status === 401 || /^\/accounts\/login/.test(url.pathname)) return { stop: 'auth' };
  if ([403, 429].includes(response.status) || /^\/(challenge|checkpoint)\//.test(url.pathname)) return { stop: 'risk' };
  if (!response.ok || url.pathname !== '/web/search/topsearch/'
    || url.searchParams.get('query') !== query || url.searchParams.get('context') !== 'user') return { stop: 'read' };
  try { return { payload: await response.json() }; } catch { return { stop: 'read' }; }
}

export async function readInstagramSearch(page, args, ownPage) {
  const limit = args.limit ?? 20, query = args.query;
  if (typeof query !== 'string' || !query.trim() || query.length > 1000 || query.includes('\0')
    || !Number.isInteger(limit) || limit < 1 || limit > 20) throw new Error('ONECREW_INSTAGRAM_SEARCH_INPUT');
  return ownPage(page, async owned => {
    await owned.goto('https://www.instagram.com/');
    const response = await owned.evaluate(`(async()=>{
      const result = await (${fetchInstagramSearch.toString()})(${JSON.stringify(query)});
      return result.stop ? result : { rows: (${projectInstagramSearch.toString()})(result.payload, ${limit}) };
    })()`);
    const result = response && 'session' in Object(response) && 'data' in Object(response) ? response.data : response;
    if (result?.stop) throw new Error('ONECREW_INSTAGRAM_SEARCH_' + result.stop.toUpperCase());
    if (!Array.isArray(result?.rows)) throw new Error('ONECREW_INSTAGRAM_SEARCH_INVALID');
    return result.rows;
  });
}
