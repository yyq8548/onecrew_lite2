import { restoreOriginalLanguage } from './original-language.mjs';

// One owned target per navigation. Never retry another profile or close user tabs.
export async function withOwnedSearchPage(page, task) {
  let target, cancelled=false, creationUnconfirmed=false;
  const targets=new Set();
  const cancel=()=>{cancelled=true;};
  const checkpoint=()=>{if(cancelled) throw new Error('ONECREW_BROWSER_CANCEL_REQUESTED');};
  const step=async work=>{checkpoint();const result=await work();checkpoint();return result;};
  process.once('SIGTERM',cancel);
  const wrapped = new Proxy(page, {get(object, key) {
    if (key === 'goto') return async (url, options = {}) => {
      const next = await step(async()=>{
        creationUnconfirmed=true;
        const created=await object.newTab(url);
        creationUnconfirmed=false;
        if(created) targets.add(created);
        return created;
      });
      if (!next) throw new Error('Search target unavailable');
      const previous = target;
      target = next;
      await step(()=>object.setActivePage(next));
      if (previous) await step(async()=>{await object.closeTab(previous);targets.delete(previous);});
      if (options.settleMs !== 0) await step(()=>object.sleep(Math.max(1, (options.settleMs || 0) / 1000)));
      await step(()=>object.setActivePage(target));
      await step(()=>object.evaluate('(' + waitForSearchDocument.toString() + ')(' + JSON.stringify(options.settleMs === 0) + ')'));
      const destination = new URL(url);
      if (destination.hostname === 'www.youtube.com' && destination.pathname === '/results') {
        await step(()=>object.evaluate('(' + waitForYoutubeSearch.toString() + ')(' + JSON.stringify(destination.searchParams.get('search_query')) + ')'));
      }
      if (destination.hostname === 'www.facebook.com' && destination.pathname.startsWith('/search/')) {
        await step(()=>object.evaluate('(' + waitForFacebookSearch.toString() + ')()'));
      }
    };
    if (key === 'evaluate') return async (...args) => {
      if (target) await step(()=>object.setActivePage(target));
      await step(()=>object.evaluate('(' + restoreOriginalLanguage.toString() + ')()'));
      return step(()=>object.evaluate(...args));
    };
    if (key === 'click') return async (...args) => {
      if (target) await step(()=>object.setActivePage(target));
      return step(()=>object.click(...args));
    };
    const value = Reflect.get(object, key);
    return typeof value === 'function' ? (...args)=>step(()=>value.apply(object,args)) : value;
  }});
  try { return await task(wrapped); }
  finally {
    try {
      for(const owned of targets) await page.closeTab(owned);
      if(cancelled&&!creationUnconfirmed) process.stderr.write('ONECREW_BROWSER_CANCELLED_CLOSED\n');
    } finally { process.removeListener('SIGTERM',cancel); }
  }
}

// newTab can return before a document exists. Native extractors attach DOM
// observers immediately; fixed sleep alone is not a readiness signal.
export async function waitForSearchDocument(waitForNavigation = false) {
  for (let poll = 0; poll < 40; poll++) {
    if (document.body && document.readyState !== 'loading' && (!waitForNavigation || location.href !== 'about:blank')) return;
    if (poll < 39) await new Promise(resolve => setTimeout(resolve, 250));
  }
  throw new Error('ONECREW_SEARCH_DOCUMENT_TIMEOUT');
}

export async function waitForYoutubeSearch(query) {
  for (let poll = 0; poll < 40; poll++) {
    const current = new URL(location.href);
    if (current.origin !== 'https://www.youtube.com' || current.pathname !== '/results'
      || current.searchParams.get('search_query') !== query) throw new Error('ONECREW_YOUTUBE_SEARCH_SOURCE_INVALID');
    const config = globalThis.ytcfg?.data_;
    if (config?.INNERTUBE_API_KEY && config.INNERTUBE_CONTEXT && globalThis.ytInitialData?.contents) return;
    if (poll < 39) await new Promise(resolve => setTimeout(resolve, 250));
  }
  throw new Error('ONECREW_YOUTUBE_SEARCH_BOOTSTRAP_TIMEOUT');
}

// Facebook can finish navigation while the result feed is still a skeleton.
// Wait on this page only; do not resubmit the search or change browser identity.
export async function waitForFacebookSearch() {
  for (let poll = 0; poll < 24; poll++) {
    if (/^\/(login|checkpoint)(\/|$|\.php)/.test(location.pathname)) return;
    const text = document.body?.innerText || '';
    if (/You must log in to continue|verify you are human|No results found|We didn't find any results|未找到结果|没有找到结果/i.test(text)) return;
    const links = document.querySelectorAll('[role="feed"] a[href], [role="main"] a[href]');
    if ([...links].some(link => {
      if (!link.getClientRects().length || !link.textContent?.trim()) return false;
      try {
        const target = new URL(link.href);
        return target.hostname === 'www.facebook.com'
          && target.pathname !== '/' && !target.pathname.startsWith('/search')
          && !/^\/(login|checkpoint|help|settings|privacy|policies)(\/|$|\.php)/.test(target.pathname);
      } catch { return false; }
    })) return;
    if (poll < 23) await new Promise(resolve => setTimeout(resolve, 500));
  }
}

export function readYahooEmptyResult(query) {
  if (location.origin!=='https://search.yahoo.com' || location.pathname!=='/search' || new URL(location.href).searchParams.get('p')!==query || document.readyState==='loading') return false;
  const web=document.querySelector('#web');
  if(!web || !web.querySelector('h2') || !document.querySelector('#ft,.compFooter,footer') || web.querySelector('.algo,h3 a[href]')) return false;
  return !!web.querySelector('ol') && /search results/i.test(web.querySelector('h2').textContent);
}

export function readSearchStop() {
  const visible = node => node.getClientRects().length > 0 && getComputedStyle(node).visibility !== 'hidden';
  const text = document.body?.innerText || '';
  if (location.hostname === 'passport.weibo.com') {
    if (location.pathname === '/sso/signin') return 'auth';
    if (location.pathname === '/visitor/visitor') return 'loading_auth';
  }
  if (/Verifying you['’]re not a bot|verify you are human|Unfortunately, bots use DuckDuckGo too|Checking your browser|我们需要确认您是人类|安全验证|访问频次异常|请求太频繁/i.test(text)
      || location.pathname.startsWith('/cdn-cgi/challenge-platform/')
      || [...document.querySelectorAll('.captcha-container,.verify-dialog,#amzn-captcha-verify-button,#challenge-running,#challenge-stage,iframe[src*="challenges.cloudflare.com"],iframe[src*="hcaptcha.com"]')].some(visible)) return 'risk';
  if (/You must log in to continue|登录后查看搜索结果/.test(text)
      || [...document.querySelectorAll('.login-container,.login-modal,.login-main')]
        .some(node => visible(node) && /登录|扫码/.test(node.innerText))) return 'auth';
  return 'unknown';
}

// Google includes PAA questions, linkless snippets and ad redirect locators.
// They are not publishing sources; preserve strict validation for actual results.
export function normalizeBrowserRows(site, rows, limit) {
  if (site === 'facebook') {
    if (!Array.isArray(rows) || rows.length > 50 || !Number.isInteger(limit) || limit < 1 || limit > 50)
      throw new Error('ONECREW_FACEBOOK_SEARCH_INVALID');
    const seen = new Set();
    return rows.filter(row => {
      let url;
      try { url = new URL(row.url); } catch { throw new Error('ONECREW_FACEBOOK_SEARCH_INVALID'); }
      if (url.origin !== 'https://www.facebook.com' || url.username || url.password || url.hash)
        throw new Error('ONECREW_FACEBOOK_SEARCH_INVALID');
      const path = url.pathname;
      const numeric = path === '/profile.php' && /^\?id=[1-9]\d*$/.test(url.search);
      const vanity = /^\/[A-Za-z0-9.\-]{2,}\/?$/.test(path) && !url.search
        && !/^\/(search|groups|events|watch|reel|reels|pages|photo|photo\.php|permalink\.php|story\.php|profile\.php|login|checkpoint|help|policies|privacy|settings|bookmarks|messages|notifications|marketplace|gaming|friends|requests|saved|me)\/?$/i.test(path);
      if (!numeric && !vanity) return false;
      // Native extraction can label an avatar with its presence indicator.
      // This contains no account-name evidence and must not consume a slot.
      if (/^Online status indicator\s*Active(?: now)?$/i.test(String(row.title ?? '').trim())) return false;
      const key = url.href.replace(/\/$/, '');
      if (seen.has(key)) return false;
      seen.add(key);
      return true;
    }).slice(0, limit);
  }
  if (site !== 'google' || !Array.isArray(rows)) return rows;
  if (rows.length > 200) throw new Error('Search result size exceeded');
  return rows.filter(row => {
    if (row.type === 'paa' || (row.type === 'snippet' && !row.url)) return false;
    try {
      const url = new URL(row.url);
      if (['google.com','www.google.com'].includes(url.hostname) && ['/url','/goto','/aclk'].includes(url.pathname)) return false;
    } catch { /* Malformed source rows still fail native validation. */ }
    return true;
  }).slice(0, limit);
}

// 在同一个搜索页面滚动，重复视口不增加候选，也不重新提交查询。
export async function readFacebookCreatorResults(page, args) {
  if (typeof args.query !== 'string' || !args.query.trim()
    || !Number.isInteger(args.limit) || args.limit < 1 || args.limit > 50)
    throw new Error('ONECREW_FACEBOOK_SEARCH_INPUT');
  await page.goto('https://www.facebook.com/search/top/?q='+encodeURIComponent(args.query),{settleMs:0});
  const rows=[],seen=new Set();let scroll=false;
  while(rows.length<args.limit) {
    const raw=await page.evaluate(`(${observeFacebookSearchPage.toString()})(${JSON.stringify(args.query)},${JSON.stringify([...seen])},${scroll},${observeFacebookAccounts.toString()},${readSearchStop.toString()})`);
    const observed=raw && 'session' in Object(raw) && 'data' in Object(raw) ? raw.data : raw;
    const batch=normalizeBrowserRows('facebook',observed,50),previous=rows.length;
    for(const row of batch) {
      const key=row.url.replace(/\/$/,'').toLowerCase();
      if(seen.has(key)) continue;
      seen.add(key);rows.push(row);
      if(rows.length===args.limit) break;
    }
    if(scroll&&rows.length===previous) break;
    scroll=true;
  }
  return rows;
}

export async function observeFacebookSearchPage(query,known,scroll,observe,readStop) {
  const check=()=>{
    if(/^\/(?:login|recover)(?:\/|\.php|$)/.test(location.pathname)) throw new Error('ONECREW_FACEBOOK_SEARCH_AUTH');
    if(/^\/(?:checkpoint|challenge|two_step_verification)(?:\/|$)/.test(location.pathname)) throw new Error('ONECREW_FACEBOOK_SEARCH_RISK');
    const stop=readStop();
    if(stop==='auth') throw new Error('ONECREW_FACEBOOK_SEARCH_AUTH');
    if(stop==='risk') throw new Error('ONECREW_FACEBOOK_SEARCH_RISK');
  };
  check();
  const initial=observe(query);
  const seen=new Set(known);
  if(!scroll||initial.some(row=>!seen.has(row.url.replace(/\/$/,'').toLowerCase()))) return initial;
  const root=document.querySelector('[role="feed"]') || document.querySelector('[role="main"]');
  let target=root;
  while(target&&target!==document.documentElement) {
    if(target.scrollHeight>target.clientHeight+8&&/auto|scroll/.test(getComputedStyle(target).overflowY)) break;
    target=target.parentElement;
  }
  target=target&&target!==document.documentElement?target:document.scrollingElement||document.documentElement;
  if(!target||typeof target.scrollBy!=='function') throw new Error('ONECREW_FACEBOOK_SEARCH_INVALID_FIELD_SCROLL');
  target.scrollBy(0,Math.max(400,target.clientHeight*.8));
  for(let poll=0;poll<12;poll++) {
    await new Promise(resolve=>setTimeout(resolve,250));
    check();
    const rows=observe(query);
    if(rows.some(row=>!seen.has(row.url.replace(/\/$/,'').toLowerCase()))) return rows;
    const currentRoot=document.querySelector('[role="feed"]') || document.querySelector('[role="main"]');
    const loading=[...currentRoot.querySelectorAll('[role="progressbar"],[aria-busy="true"]')].some(node=>node.getClientRects().length&&getComputedStyle(node).visibility!=='hidden');
    if(poll>=3&&!loading) return rows;
    if(poll===11&&loading) throw new Error('ONECREW_FACEBOOK_SEARCH_INVALID_FIELD_LOADING');
  }
  return observe(query);
}

export function observeFacebookAccounts(query) {
  const current = new URL(location.href);
  if (current.origin !== 'https://www.facebook.com' || !/^\/search\/top\/?$/.test(current.pathname)
    || current.searchParams.get('q') !== query) throw new Error('ONECREW_FACEBOOK_SEARCH_INVALID_FIELD_SOURCE');
  // Never fall back to document.body: it includes navigation/chat contacts.
  const root = document.querySelector('[role="feed"]') || document.querySelector('[role="main"]');
  if (!root) throw new Error('ONECREW_FACEBOOK_SEARCH_INVALID_FIELD_ROOT');
  const clean = value => String(value || '').replace(/[\u200b-\u200f\u202a-\u202e\u2060\ufeff]/g,'').replace(/\s+/g,' ').trim();
  const rows = [], seen = new Set();
  for (const anchor of root.querySelectorAll('a[href]')) {
    if (!anchor.getClientRects().length || getComputedStyle(anchor).visibility === 'hidden'
      || anchor.closest('[role="navigation"], [role="complementary"], [role="dialog"]')) continue;
    const title = clean(anchor.innerText);
    if (!title || title.length > 200 || /^Online status indicator\s*Active(?: now)?$/i.test(title)
      || /^\d{8,}$/.test(title) || /^(?:\S ){4,}\S$/.test(title)) continue;
    let url;
    try { url = new URL(anchor.href); } catch { continue; }
    if (url.origin !== current.origin || url.username || url.password) continue;
    const numeric = url.pathname === '/profile.php' && /^[1-9]\d*$/.test(url.searchParams.get('id') || '');
    const vanity = /^\/[A-Za-z0-9.\-]{2,}\/?$/.test(url.pathname)
      && !/^\/(search|groups|events|watch|reel|reels|pages|photo|photo\.php|permalink\.php|story\.php|profile\.php|login|checkpoint|help|policies|privacy|settings|bookmarks|messages|notifications|marketplace|gaming|friends|requests|saved|me)\/?$/i.test(url.pathname);
    if (!numeric && !vanity) continue;
    const key = current.origin + url.pathname.replace(/\/$/,'') + (numeric ? '?id=' + url.searchParams.get('id') : '');
    if (seen.has(key)) continue;
    seen.add(key);
    // Keep account-local text only; whole feed ancestors mix other authors.
    rows.push({index:rows.length+1,title,text:title,url:key});
    if (rows.length === 50) break;
  }
  return rows;
}
