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

export function parseInstagramSearchCapture(entry, query) {
  const url = new URL(entry.url);
  if (url.origin !== 'https://www.instagram.com' || url.pathname !== '/api/graphql' || entry.method !== 'POST') return null;
  if (entry.requestBodyTruncated) throw new Error('ONECREW_INSTAGRAM_SEARCH_INVALID');
  const form = new URLSearchParams(entry.requestBodyPreview);
  if (form.get('fb_api_req_friendly_name') !== 'PolarisSearchBoxRefetchableQuery') return null;
  const variables = JSON.parse(form.get('variables'));
  if (variables?.data?.query !== query) return null;
  if (variables.hasQuery !== true || variables.data.context !== 'blended'
    || variables.data.search_surface !== 'web_top_search') throw new Error('ONECREW_INSTAGRAM_SEARCH_INVALID');
  if (entry.responseStatus === 401) throw new Error('ONECREW_INSTAGRAM_SEARCH_AUTH');
  if ([403, 429].includes(entry.responseStatus)) throw new Error('ONECREW_INSTAGRAM_SEARCH_RISK');
  if (entry.responseStatus === 404) throw new Error('ONECREW_INSTAGRAM_SEARCH_READ_HTTP_404');
  if (entry.responseStatus !== 200) throw new Error('ONECREW_INSTAGRAM_SEARCH_READ_HTTP');
  if (entry.responseBodyTruncated || typeof entry.responsePreview !== 'string') throw new Error('ONECREW_INSTAGRAM_SEARCH_INVALID');
  const text = entry.responsePreview.startsWith('base64:')
    ? Buffer.from(entry.responsePreview.slice(7), 'base64').toString('utf8') : entry.responsePreview;
  const payload = JSON.parse(text);
  const result = payload?.data?.xdt_api__v1__fbsearch__topsearch_connection;
  if (payload.errors?.length || !Array.isArray(result?.users)) throw new Error('ONECREW_INSTAGRAM_SEARCH_INVALID');
  return result;
}

export function instagramSearchPageState(query = null, startedAt = 0) {
  if (location.origin !== 'https://www.instagram.com') return { stop: 'read_redirect' };
  if (/^\/accounts\/login/.test(location.pathname)) return { stop: 'auth' };
  if (/^\/(challenge|checkpoint)\//.test(location.pathname)) return { stop: 'risk' };
  if (location.pathname !== '/explore/') return { stop: 'read_redirect' };
  const input = document.querySelector('input[aria-label="Search input"], input[aria-label="搜索输入"]');
  if (!input || !input.getClientRects().length) return { ready: false };
  if (query === null) return { ready: true };
  const keyword = [...document.querySelectorAll('a[href]')].some(anchor => {
    const url = new URL(anchor.href, location.href);
    return anchor.getClientRects().length && url.origin === location.origin
      && url.pathname === '/explore/search/keyword/' && url.searchParams.get('q') === query;
  });
  const busy = [...document.querySelectorAll('[role="progressbar"], [aria-busy="true"]')]
    .some(element => element.getClientRects().length);
  const completed = performance.getEntriesByType('resource').some(entry => {
    const url = new URL(entry.name, location.href);
    return entry.startTime >= startedAt && entry.responseEnd > 0
      && ['fetch', 'xmlhttprequest'].includes(entry.initiatorType)
      && url.origin === location.origin && url.pathname === '/api/graphql';
  });
  return { ready: input.value === query && keyword && !busy && completed };
}

async function waitForInstagramSearch(owned, query = null, startedAt = 0) {
  for (let poll = 0; poll < 40; poll++) {
    const raw = await owned.evaluate(`(${instagramSearchPageState.toString()})(${JSON.stringify(query)}, ${startedAt})`);
    const state = raw && 'session' in Object(raw) && 'data' in Object(raw) ? raw.data : raw;
    if (state?.stop) throw new Error('ONECREW_INSTAGRAM_SEARCH_' + state.stop.toUpperCase());
    if (state?.ready) return;
    if (poll < 39) await owned.sleep(0.25);
  }
  throw new Error('ONECREW_INSTAGRAM_SEARCH_TIMEOUT');
}

export async function readInstagramSearch(page, args, ownPage) {
  const limit = args.limit ?? 20, query = args.query;
  if (typeof query !== 'string' || !query.trim() || query.length > 1000 || query.includes('\0')
    || !Number.isInteger(limit) || limit < 1 || limit > 20) throw new Error('ONECREW_INSTAGRAM_SEARCH_INPUT');
  return ownPage(page, async owned => {
    await owned.goto('https://www.instagram.com/explore/');
    await waitForInstagramSearch(owned);
    if (!await owned.startNetworkCapture('/api/graphql')) throw new Error('ONECREW_INSTAGRAM_SEARCH_INVALID');
    const inputSelector = 'input[aria-label="Search input"], input[aria-label="搜索输入"]';
    await owned.click(inputSelector);
    const rawTiming = await owned.evaluate('({ startedAt: performance.now() })');
    const timing = rawTiming && 'session' in Object(rawTiming) && 'data' in Object(rawTiming) ? rawTiming.data : rawTiming;
    if (!Number.isFinite(timing?.startedAt) || timing.startedAt <= 0) throw new Error('ONECREW_INSTAGRAM_SEARCH_INVALID');
    const filled = await owned.fillText(inputSelector, query);
    if (filled?.verified !== true) throw new Error('ONECREW_INSTAGRAM_SEARCH_INVALID');
    await waitForInstagramSearch(owned, query, timing.startedAt);
    // 页面渲染完本次结果后再读取一次捕获，避免清空仍在接收正文的请求。
    const entries = await owned.readNetworkCapture();
    if (!Array.isArray(entries)) throw new Error('ONECREW_INSTAGRAM_SEARCH_INVALID');
    for (const entry of entries) {
      const payload = parseInstagramSearchCapture(entry, query);
      if (payload) return projectInstagramSearch(payload, limit);
    }
    throw new Error('ONECREW_INSTAGRAM_SEARCH_INVALID');
  });
}
