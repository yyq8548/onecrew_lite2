// One fixed-site, current-account probe. No identity or cookie leaves this module.
export async function openProbePage(page, url) {
  const target = await page.newTab(url);
  if (!target) throw new Error('Authentication probe page was not created');
  await page.setActivePage(target);
  return target;
}

export function classifyIdentity(result) {
  if (result?.authenticated === true) return 'logged_in';
  if (result?.authenticated === false) return 'not_logged_in';
  if (result?.http_status === 429) return 'rate_limited';
  return 'unknown';
}

// Executed in the fixed provider origin. Return a boolean, never account details.
export async function observeCurrentAccount(site, allowRequest = true) {
  if (site === 'linkedin-learning') site = 'linkedin';
  const origins = { bilibili: 'https://www.bilibili.com', tiktok: 'https://www.tiktok.com', instagram: 'https://www.instagram.com', facebook: 'https://www.facebook.com', zhihu: 'https://www.zhihu.com', douyin: 'https://www.douyin.com', linkedin: 'https://www.linkedin.com', weibo: 'https://weibo.com', tieba: 'https://tieba.baidu.com', medium: 'https://medium.com', nowcoder: 'https://www.nowcoder.com', douban: 'https://www.douban.com', pixiv: 'https://www.pixiv.net' };
  origins.twitter = 'https://x.com';
  origins.youtube = 'https://www.youtube.com';
  if (/\/challenge\/|\/checkpoint\/|captcha|security\/check/.test(location.pathname)) return {};
  const visible = selector => Array.from(document.querySelectorAll(selector)).some(node => node.getClientRects().length > 0);
  const visibleText = (selector, pattern) => Array.from(document.querySelectorAll(selector)).some(node => node.getClientRects().length > 0 && pattern.test((node.textContent || '').trim()));
  const validId = value => (typeof value === 'string' && /^[1-9]\d{0,31}$/.test(value)) || (typeof value === 'number' && Number.isSafeInteger(value) && value > 0);
  const captchaSelector = 'iframe[src*="captcha"], #captcha, .captcha-container';
  if (visible(captchaSelector)) return {};
  if (site === 'douban' && location.origin === 'https://accounts.douban.com' && location.pathname.startsWith('/passport/login') && visible('input[name="username"], input[name="phone"]')) return { authenticated: false };
  if (location.origin !== origins[site]) return {};
  if (site === 'youtube') {
    // 当前访问者配置和顶部账号按钮共同证明登录，不读取视频作者资料。
    const loggedIn = globalThis.ytcfg?.get?.('LOGGED_IN');
    const account = visible('ytd-masthead #avatar-btn');
    const login = visible('ytd-masthead a[href*="ServiceLogin"]');
    if (loggedIn === true && account && !login) return { authenticated: true };
    if (loggedIn === false && login && !account) return { authenticated: false };
    return {};
  }
  if (site === 'twitter') {
    if (location.pathname === '/i/flow/login' && visible('input[autocomplete="username"]')) return { authenticated: false };
    const own = Array.from(document.querySelectorAll('a[data-testid="AppTabBar_Profile_Link"]')).some(node => {
      if (!node.getClientRects().length) return false;
      try {
        const url = new URL(node.href, location.origin);
        return url.origin === origins.twitter && /^\/[A-Za-z0-9_]{1,15}\/?$/.test(url.pathname)
          && !/^\/(home|explore|notifications|messages|settings|login|signup)\/?$/.test(url.pathname);
      } catch { return false; }
    });
    return own && visible('[data-testid="SideNav_AccountSwitcher_Button"]') ? { authenticated: true } : {};
  }
  if (site === 'tiktok') {
    // The app context represents the signed-in viewer, unlike feed authors and
    // public user-detail payloads. Require its matching own-profile navigation.
    const raw = document.querySelector('script[id="__UNIVERSAL_DATA_FOR_REHYDRATION__"]')?.textContent;
    if (!raw || raw.length > 4 * 1024 * 1024) return {};
    let user;
    try { user = JSON.parse(raw)?.__DEFAULT_SCOPE__?.['webapp.app-context']?.user; } catch { return {}; }
    if (!validId(user?.uid) || typeof user?.uniqueId !== 'string' || !/^[A-Za-z0-9_.]{1,24}$/.test(user.uniqueId)) return {};
    const own = Array.from(document.querySelectorAll('a[data-e2e="nav-profile"]')).some(a => {
      if (a.getClientRects().length === 0) return false;
      try {
        const url = new URL(a.href, location.origin);
        return url.origin === origins.tiktok && url.pathname.replace(/\/$/, '') === '/@' + user.uniqueId;
      } catch { return false; }
    });
    return { authenticated: own ? true : undefined };
  }
  if (site === 'douban' || site === 'nowcoder' || site === 'medium') {
    // Account controls must belong to the site navigation, never a feed author.
    const navs = Array.from(document.querySelectorAll('header, [role="banner"], .nav-user-account, .top-nav-info'));
    const links = navs.flatMap(nav => Array.from(nav.querySelectorAll('a'))).filter(node => node.getClientRects().length > 0);
    const own = links.some(node => (site === 'medium' ? /^\/@[A-Za-z0-9._-]+\/?$/ : site === 'douban' ? /^\/people\/[A-Za-z0-9._-]+\/?$/ : /^\/users\/\d+\/?$/).test(new URL(node.href, location.origin).pathname));
    const logout = links.some(node => /^(Sign out|Log out|退出|退出登录)$/.test((node.textContent || '').trim()) && /logout|signout/i.test(node.getAttribute('href') || ''));
    if (own && logout) return { authenticated: true };
    if (site === 'medium' && /^\/m\/signin/.test(location.pathname) && visible('input[type="email"]')) return { authenticated: false };
    if (site === 'medium' && location.pathname === '/' && links.some(node => /\/m\/signin/.test(node.getAttribute('href') || '')) && visible('h2')) return { authenticated: false };
    if (site === 'medium' && location.pathname === '/' && visible('a[href*="/m/signin"]') && visibleText('h1, h2', /Human stories\s*&\s*ideas/)) return { authenticated: false };
    if (site === 'nowcoder' && /\/login/.test(location.pathname) && visible('input[type="password"]')) return { authenticated: false };
    if (site === 'nowcoder' && location.pathname === '/' && visibleText('button', /^登录\s*\/\s*注册$/)) return { authenticated: false };
    if (site === 'douban' && visible('iframe[src*="accounts.douban.com/passport/login"]')) return { authenticated: false };
    return {};
  }
  if (site === 'pixiv') {
    // The probe opens the homepage. Never use a visited artist's public data.
    if (!/^\/(?:[a-z]{2}(?:-[a-z]{2})?\/)?$/.test(location.pathname)) return {};
    const candidates = [globalThis.pixiv?.context?.userData, globalThis.globalInitData?.userData, globalThis.__PIXIV_CONTEXT__?.userData];
    const parse = raw => {
      if (typeof raw !== 'string' || raw.length > 4 * 1024 * 1024) return null;
      try { return JSON.parse(raw); } catch { return null; }
    };
    const raw = document.querySelector('meta[name="global-data"]')?.getAttribute('content');
    candidates.push(parse(raw)?.userData);
    // Current homepage bootstraps the viewer separately from artwork authors.
    // PixivNow's web-client reference documents this serialized Next.js state.
    const next = parse(document.querySelector('script[id="__NEXT_DATA__"]')?.textContent);
    const serialized = next?.props?.pageProps?.serverSerializedPreloadedState;
    candidates.push(parse(serialized)?.userData?.self);
    const ids = new Set();
    for (const candidate of candidates) {
      if (!candidate || typeof candidate !== 'object' || Array.isArray(candidate)) continue;
      const values = [candidate.id, candidate.userId].filter(validId).map(String);
      if (new Set(values).size > 1) return {};
      for (const id of values) ids.add(id);
    }
    if (ids.size > 1) return {}; // Conflicting current-account observations.
    if (ids.size === 1) return { authenticated: true };
    if (visible('a[href*="accounts.pixiv.net/login"]')) return { authenticated: false };
    return {};
  }
  if (site === 'facebook') {
    // /me is a server-owned redirect to the current account, not a visited author.
    const path = location.pathname;
    if (/^\/login(?:\.php)?/.test(path) && visible('input[name="email"], input[name="pass"]')) return { authenticated: false };
    // Saved-account landing pages require another sign-in step, even when an
    // account name is displayed and no password input has been rendered yet.
    if (path === '/' && visibleText('button, [role="button"]', /^(Continue|继续)$/)) {
      const deviceLogin = Array.from(document.querySelectorAll('form[action]')).some(form => {
        const action = new URL(form.getAttribute('action'), location.origin);
        return action.origin === origins.facebook && action.pathname === '/login/device-based/login/caa/';
      });
      if (deviceLogin && visibleText('button, [role="button"]', /^(Use another profile|使用其他个人主页|使用其他账户)$/)) return { authenticated: false };
    }
    const profile = /^\/profile\.php$/.test(path) && new URLSearchParams(location.search).has('id');
    const vanity = /^\/[A-Za-z0-9._-]+\/?$/.test(path) && !/^\/(me|login|checkpoint|recover|help)\/?$/.test(path);
    const account = visible('[aria-label="Your profile"], [aria-label="Account"], [aria-label="你的个人主页"], [aria-label="帐户"], [aria-label="账户"]');
    return { authenticated: (profile || vanity) && account ? true : undefined };
  }
  if (site === 'instagram') {
    if (/^\/accounts\/login/.test(location.pathname) && visible('input[name="username"]')) return { authenticated: false };
    // Own-profile navigation is different from avatars attached to feed posts.
    const nav = Array.from(document.querySelectorAll('nav a, [role="navigation"] a, a')).find(a => {
      const label = a.querySelector('svg[aria-label="Profile"], svg[aria-label="个人主页"], svg[aria-label="个人资料"]');
      const text = (a.textContent || '').trim();
      const image = a.querySelector('img[alt*="profile picture"], img[alt*="头像"]');
      let ownNavigation = false;
      if (image) {
        let parent = a.parentElement;
        for (let level = 0; parent && level < 6; level += 1, parent = parent.parentElement) {
          if (parent.querySelectorAll('a').length > 20) break;
          if (parent.querySelector('a[href="/direct/inbox/"]') && parent.querySelector('a[href="/reels/"]')) {
            ownNavigation = true;
            break;
          }
        }
      }
      return a.getClientRects().length > 0 && (label || ownNavigation || (image && /^(Profile|个人主页|个人资料)$/.test(text))) && /^\/[A-Za-z0-9._]+\/$/.test(new URL(a.href).pathname);
    });
    if (nav) return { authenticated: true };
  }
  if (site === 'douyin') {
    // Public author pages cannot expose these current-account-only tabs.
    if (location.pathname !== '/user/self') return {};
    const tabs = Array.from(document.querySelectorAll('[role="tab"]')).filter(node => node.getClientRects().length > 0).map(node => (node.textContent || '').trim());
    if (visible('h1') && tabs.includes('观看历史') && tabs.includes('私密作品')) return { authenticated: true };
    return {};
  }
  if (site === 'weibo') {
    // Same current-session store used by the provider's own self-account reader.
    // A feed author's UID is not read from post cards or public profile links.
    const uid = document.querySelector?.('#app')?.__vue_app__?.config?.globalProperties?.$store?.state?.config?.config?.uid;
    if (validId(uid)) return { authenticated: true };
  }
  if (!allowRequest) return {};
  if (site === 'bilibili') {
    const response = await fetch('https://api.bilibili.com/x/web-interface/nav', {credentials: 'include', headers: {Accept: 'application/json'}});
    if (!response.ok) return {http_status: response.status};
    if (!(response.headers.get('content-type') || '').includes('json')) return {};
    const data = await response.json();
    if (data.code === -101 || (data.code === 0 && data.data?.isLogin === false)) return {authenticated: false};
    return {authenticated: data.code === 0 && data.data?.isLogin === true && validId(data.data?.mid) ? true : undefined};
  }
  if (site === 'tieba' && visibleText('h1, h2, h3, button, [role="dialog"]', /^(登录贴吧|登录 探索更多内容)$/)) return { authenticated: false };
  const paths = { instagram: '/api/v1/accounts/current_user/?edit=true', zhihu: '/api/v4/me?include=url_token', linkedin: '/voyager/api/me', weibo: '/ajax/config/get_config', tieba: '/dc/common/tbs' };
  const headers = site === 'instagram' ? { 'X-IG-App-ID': '936619743392459', Accept: 'application/json' } : { Accept: 'application/json' };
  if (site === 'linkedin') {
    // Existing same-origin request token stays inside the provider page.
    const cookie = (document.cookie.split('; ').find(value => value.startsWith('JSESSIONID=')) || '').split('=')[1] || '';
    const csrf = cookie.replace(/^"|"$/g, '');
    if (!csrf) return {};
    headers['csrf-token'] = csrf;
  }
  const response = await fetch(paths[site], { credentials: 'include', headers });
  if (!response.ok) return { http_status: response.status, ...(response.status === 401 ? { authenticated: false } : {}) };
  if (!(response.headers.get('content-type') || '').includes('json')) return {};
  const data = await response.json();
  if (site === 'instagram') return { authenticated: data.status === 'ok' && !!data.user?.pk ? true : undefined };
  if (site === 'zhihu') return { authenticated: !!data.url_token && !!(data.uid || data.id) ? true : undefined };
  if (site === 'linkedin') return { authenticated: !!data.miniProfile?.publicIdentifier ? true : undefined };
  // This fixed config endpoint returns the current session UID; an explicit
  // anonymous flag overrides it. Public profile responses are never queried.
  if (site === 'weibo') return { authenticated: data.ok === 1 && data.data?.login === false ? false : data.ok === 1 && validId(data.data?.uid) ? true : undefined };
  if (site === 'tieba') return { authenticated: data.is_login === 1 && typeof data.tbs === 'string' && !!data.tbs ? true : data.is_login === 0 ? false : undefined };
  return {};
}

export async function readSiteLogin(page, site) {
  const urls = { bilibili: 'https://www.bilibili.com/', tiktok: 'https://www.tiktok.com/explore', instagram: 'https://www.instagram.com/', facebook: 'https://www.facebook.com/me', zhihu: 'https://www.zhihu.com/', douyin: 'https://www.douyin.com/user/self', linkedin: 'https://www.linkedin.com/feed/', 'linkedin-learning': 'https://www.linkedin.com/learning/', weibo: 'https://weibo.com/', tieba: 'https://tieba.baidu.com/', medium: 'https://medium.com/', nowcoder: 'https://www.nowcoder.com/', douban: 'https://www.douban.com/mine/', pixiv: 'https://www.pixiv.net/' };
  urls.twitter = 'https://x.com/home';
  urls.youtube = 'https://www.youtube.com/';
  if (!urls[site]) throw new Error('Unsupported authentication site');
  const target = await openProbePage(page, urls[site]);
  const attempts = site === 'tiktok' ? 5 : 3;
  for (let attempt = 0; attempt < attempts; attempt += 1) {
    // wait(n) can return as soon as the DOM is quiet, before account navigation
    // hydrates. Poll at fixed intervals so those early returns cannot exhaust it.
    await page.sleep(2);
    // Every observation is bound to the tab created by this probe; another
    // bridge operation or user navigation must not select the previous site.
    await page.setActivePage(target);
    let result = await page.evaluate(`(${observeCurrentAccount.toString()})(${JSON.stringify(site)}, ${attempt === attempts - 1})`);
    if (result && typeof result === 'object' && 'session' in result && 'data' in result) result = result.data;
    const status = classifyIdentity(result);
    if (status !== 'unknown') return status;
  }
  return 'unknown';
}
