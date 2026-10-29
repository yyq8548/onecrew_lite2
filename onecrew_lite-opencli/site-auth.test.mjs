import assert from 'node:assert/strict';
import { test } from 'node:test';
import vm from 'node:vm';
import { openProbePage, classifyIdentity, observeCurrentAccount, readSiteLogin } from './onecrew-site-auth/probe.mjs';

test('X verifies visible own navigation and account switcher, never feed authors', async () => {
  for (const scenario of ['signed-in', 'public-author', 'hidden', 'wrong-origin', 'signed-out']) {
    const result = await vm.runInNewContext(`(${observeCurrentAccount.toString()})('twitter', false)`, {
      URL,
      location: { origin: scenario === 'wrong-origin' ? 'https://x.com.evil.example' : 'https://x.com', pathname: scenario === 'signed-out' ? '/i/flow/login' : '/home' },
      document: {
        querySelectorAll: selector => {
          if (scenario === 'signed-out') return selector.includes('autocomplete="username"') ? [{getClientRects:()=>[{}]}] : [];
          if (selector.includes('AppTabBar_Profile_Link')) return scenario === 'public-author' ? [] : [{href:'https://x.com/private_self',getClientRects:()=>scenario === 'hidden' ? [] : [{}]}];
          if (selector.includes('SideNav_AccountSwitcher_Button')) return [{getClientRects:()=>[{}]}];
          return [];
        },
      },
    });
    assert.equal(classifyIdentity(result), scenario === 'signed-in' ? 'logged_in' : scenario === 'signed-out' ? 'not_logged_in' : 'unknown', scenario);
    assert.equal(JSON.stringify(result).includes('private_self'), false);
  }
});

test('login polling allows delayed navigation hydration despite early DOM stability', async () => {
  let elapsed = 0;
  const page = {
    newTab: async () => 'probe', setActivePage: async () => {},
    wait: async () => { elapsed += 0.5; },
    sleep: async seconds => { elapsed += seconds; },
    evaluate: async () => elapsed >= 6 ? { authenticated: true } : {},
  };
  assert.equal(await readSiteLogin(page, 'tiktok'), 'logged_in');
  assert.equal(elapsed, 6);
});

test('TikTok uses a bound new tab and verifies the actual current-session navigation', async () => {
  const calls = [];
  let bound = false;
  const context = {
    location: { origin: 'https://www.tiktok.com', pathname: '/explore' }, URL,
    document: {
      querySelector: selector => selector.includes('__UNIVERSAL_DATA_FOR_REHYDRATION__') ? {
        textContent: JSON.stringify({ __DEFAULT_SCOPE__: { 'webapp.app-context': { user: { uid: '123', uniqueId: 'private_self' } } } }),
      } : null,
      querySelectorAll: selector => selector.includes('nav-profile') ? [{ href: 'https://www.tiktok.com/@private_self', getClientRects: () => [{}] }] : [],
    },
  };
  const page = {
    newTab: async url => { calls.push(url); return 'exact-target'; },
    setActivePage: async id => { calls.push(id); await Promise.resolve(); bound = true; },
    goto: () => { throw new Error('Navigation rejected.'); },
    sleep: async () => {},
    evaluate: async script => { assert.equal(bound, true); return { session: 'private', data: await vm.runInNewContext(script, context) }; },
  };
  assert.equal(await readSiteLogin(page, 'tiktok'), 'logged_in');
  assert.deepEqual(calls, ['https://www.tiktok.com/explore', 'exact-target', 'exact-target']);
});

test('TikTok public authors, stale cookies and unmatched profiles never prove current-account login', async () => {
  for (const scenario of ['current', 'public', 'zero', 'mismatch', 'missing', 'malformed', 'hidden', 'wrong-origin', 'captcha']) {
    const user = scenario === 'public' ? undefined : { uid: scenario === 'zero' ? '0' : '123', uniqueId: 'private_self' };
    const context = {
      URL, location: { origin: scenario === 'wrong-origin' ? 'https://www.tiktok.com.evil.example' : 'https://www.tiktok.com', pathname: '/explore' },
      document: {
        querySelector: () => scenario === 'missing' ? null : { textContent: scenario === 'malformed' ? '{' : JSON.stringify({ __DEFAULT_SCOPE__: { 'webapp.app-context': { user }, 'webapp.user-detail': { userInfo: { user: { id: '321', uniqueId: 'private_self' } } } } }) },
        querySelectorAll: selector => scenario === 'captcha' && selector.includes('captcha') ? [{ getClientRects: () => [{}] }]
          : selector.includes('nav-profile') ? [{ href: `https://www.tiktok.com/@${scenario === 'mismatch' ? 'another_person' : 'private_self'}`, getClientRects: () => scenario === 'hidden' ? [] : [{}] }] : [],
      },
    };
    const result = await vm.runInNewContext(`(${observeCurrentAccount.toString()})('tiktok', false)`, context);
    assert.equal(classifyIdentity(result), scenario === 'current' ? 'logged_in' : 'unknown', scenario);
    assert.equal(JSON.stringify(result).includes('private_self'), false);
    assert.equal(JSON.stringify(result).includes('123'), false);
  }
});

test('Weibo current-session store proves login without exporting UID; public account data does not', async () => {
  for (const currentSession of [true, false]) {
    const result = await vm.runInNewContext(`(${observeCurrentAccount.toString()})('weibo', false)`, {
      location: { origin: 'https://weibo.com', pathname: '/' },
      document: { querySelectorAll: () => [], querySelector: () => currentSession ? { __vue_app__: { config: { globalProperties: { $store: { state: { config: { config: { uid: '123456789' } } } } } } } } : { uid: '123456789' } },
    });
    assert.equal(classifyIdentity(result), currentSession ? 'logged_in' : 'unknown');
    assert.equal(JSON.stringify(result).includes('123456789'), false);
  }
});

test('navigation identity and logout must coexist; public author links cannot prove a session', async () => {
  for (const [site, profile] of [['medium', '/@self'], ['douban', '/people/self-name/'], ['nowcoder', '/users/123/']]) {
    for (const signedIn of [false, true]) {
      const links = [profile, ...(signedIn ? ['/logout'] : [])].map(href => ({ href, getAttribute: () => href, textContent: href === '/logout' ? '退出登录' : 'Author', getClientRects: () => [{}] }));
      const nav = { querySelectorAll: () => links };
      const result = await vm.runInNewContext(`(${observeCurrentAccount.toString()})('${site}', false)`, {
        URL, location: { origin: site === 'medium' ? 'https://medium.com' : `https://www.${site}.com`, pathname: '/' },
        document: { querySelectorAll: selector => selector.startsWith('header,') ? [nav] : [], querySelector: () => null },
      });
      assert.equal(classifyIdentity(result), signedIn ? 'logged_in' : 'unknown');
    }
  }
});

test('auth opens one owned tab without a second navigation that Chrome rejects', async () => {
  const calls = [];
  const page = {
    newTab: async url => { calls.push(url); return 'owned-target'; },
    setActivePage: id => calls.push(id),
    goto: () => { throw new Error('Navigation rejected.'); },
  };
  await openProbePage(page, 'https://www.instagram.com/');
  assert.deepEqual(calls, ['https://www.instagram.com/', 'owned-target']);
});

test('fixed-origin self endpoints qualify only actual self identity and never emit it', async () => {
  const payloads = {
    instagram: { status: 'ok', user: { pk: 'private' } },
  };
  for (const site of Object.keys(payloads)) {
    const context = {
      location: { origin: `https://www.${site}.com`, pathname: '/' },
      document: { querySelectorAll: () => [] },
      fetch: async () => ({ ok: true, headers: { get: () => 'application/json' }, json: async () => payloads[site] }),
    };
    const result = await vm.runInNewContext(`(${observeCurrentAccount.toString()})('${site}')`, context);
    assert.equal(classifyIdentity(result), 'logged_in');
    assert.equal(JSON.stringify(result).includes('private'), false);
    context.location.origin = `https://creator.${site}.com`;
    assert.equal(classifyIdentity(await vm.runInNewContext(`(${observeCurrentAccount.toString()})('${site}')`, context)), 'unknown');
  }
});

test('Douyin verifies private self controls and rejects identical controls on a public author page', async () => {
  for (const pathname of ['/user/self', '/user/someone']) {
    const result = await vm.runInNewContext(`(${observeCurrentAccount.toString()})('douyin')`, {
      location: { origin: 'https://www.douyin.com', pathname },
      document: { querySelectorAll: selector => selector === 'h1' ? [{ getClientRects: () => [{}] }] : selector === '[role="tab"]' ? ['观看历史', '私密作品'].map(textContent => ({ textContent, getClientRects: () => [{}] })) : [] },
    });
    assert.equal(classifyIdentity(result), pathname === '/user/self' ? 'logged_in' : 'unknown');
  }
});

test('Instagram avatar-only account navigation works; feed avatars cannot qualify', async () => {
  for (const navigation of [true, false]) {
    const parent = {
      querySelectorAll: () => new Array(navigation ? 9 : 25).fill({}),
      querySelector: () => ({}), parentElement: null,
    };
    const avatar = {
      href: 'https://www.instagram.com/self/', textContent: '', parentElement: parent,
      getClientRects: () => [{}],
      querySelector: selector => selector.includes('img') ? {} : null,
    };
    const result = await vm.runInNewContext(`(${observeCurrentAccount.toString()})('instagram', false)`, {
      location: { origin: 'https://www.instagram.com', pathname: '/' }, URL,
      document: { querySelectorAll: selector => selector.includes('nav a') ? [avatar] : [] },
    });
    assert.equal(classifyIdentity(result), navigation ? 'logged_in' : 'unknown');
  }
});

test('Facebook /me redirect needs authenticated account navigation, not merely a public profile', async () => {
  for (const accountVisible of [false, true]) {
    const result = await vm.runInNewContext(`(${observeCurrentAccount.toString()})('facebook')`, {
      location: { origin: 'https://www.facebook.com', pathname: '/private.name/', search: '' },
      URLSearchParams,
      document: { querySelectorAll: selector => accountVisible && selector.includes('Your profile') ? [{ getClientRects: () => [{}] }] : [] },
    });
    assert.equal(classifyIdentity(result), accountVisible ? 'logged_in' : 'unknown');
  }
});

test('positive self identity is required; rate limits and missing signals never mean logout', () => {
  assert.equal(classifyIdentity({ authenticated: true }), 'logged_in');
  assert.equal(classifyIdentity({ authenticated: false }), 'not_logged_in');
  assert.equal(classifyIdentity({ http_status: 429 }), 'rate_limited');
  assert.equal(classifyIdentity({ http_status: 403 }), 'unknown');
  assert.equal(classifyIdentity({ user: { id: 'somebody' } }), 'unknown');
  assert.equal(classifyIdentity({}), 'unknown');
});

test('LinkedIn verifies current-account API, keeps csrf internal, rejects missing identity and checkpoint', async () => {
  for (const scenario of ['self', 'missing', 'checkpoint', 'forbidden']) {
    const context = {
      location: { origin: 'https://www.linkedin.com', pathname: scenario === 'checkpoint' ? '/checkpoint/challenge/' : '/feed/' },
      document: { cookie: 'JSESSIONID="local-token"', querySelectorAll: () => [] },
      fetch: async (path, options) => {
        assert.equal(path, '/voyager/api/me');
        assert.equal(options.headers['csrf-token'], 'local-token');
        return { ok: scenario !== 'forbidden', status: scenario === 'forbidden' ? 403 : 200,
          headers: { get: () => 'application/json' },
          json: async () => scenario === 'self' ? { miniProfile: { publicIdentifier: 'private-self' } } : {} };
      },
    };
    const result = await vm.runInNewContext(`(${observeCurrentAccount.toString()})('linkedin')`, context);
    assert.equal(classifyIdentity(result), scenario === 'self' ? 'logged_in' : 'unknown');
    assert.equal(JSON.stringify(result).includes('private-self'), false);
    assert.equal(JSON.stringify(result).includes('local-token'), false);
  }
});

test('Tieba and Weibo require explicit server login status, not public account data', async () => {
  for (const [site, origin, payload] of [
    ['tieba', 'https://tieba.baidu.com', { is_login: 1, tbs: 'private-token' }],
    ['weibo', 'https://weibo.com', { ok: 1, data: { login: true, uid: '123456789' } }],
  ]) {
    for (const authenticated of [true, false]) {
      const data = authenticated ? payload : site === 'tieba' ? { is_login: 0 } : { ok: 1, data: { login: false, uid: 'public-user' } };
      const result = await vm.runInNewContext(`(${observeCurrentAccount.toString()})('${site}')`, {
        location: { origin, pathname: '/' }, document: { querySelectorAll: () => [] },
        fetch: async () => ({ ok: true, headers: { get: () => 'application/json' }, json: async () => data }),
      });
      assert.equal(classifyIdentity(result), authenticated ? 'logged_in' : 'not_logged_in');
      assert.equal(JSON.stringify(result).includes('private'), false);
    }
  }
});

test('anonymous zero account sentinels and captcha never qualify as login', async () => {
  for (const uid of [0, '0', '', null]) {
    const result = await vm.runInNewContext(`(${observeCurrentAccount.toString()})('weibo')`, {
      location: { origin: 'https://weibo.com', pathname: '/' },
      document: { querySelectorAll: () => [], querySelector: () => ({ __vue_app__: { config: { globalProperties: { $store: { state: { config: { config: { uid } } } } } } } }) },
      fetch: async () => ({ ok: true, headers: { get: () => 'application/json' }, json: async () => ({ ok: 1, data: { uid } }) }),
    });
    assert.equal(classifyIdentity(result), 'unknown');
  }
  const result = await vm.runInNewContext(`(${observeCurrentAccount.toString()})('douban')`, {
    location: { origin: 'https://accounts.douban.com', pathname: '/passport/login' },
    document: { querySelectorAll: () => [{ getClientRects: () => [{}] }] },
  });
  assert.equal(classifyIdentity(result), 'unknown');
});

test('Bilibili verifies live self-account API rather than cookie presence or public profile', async () => {
  for (const [data, expected] of [[{code: 0, data: {isLogin: true, mid: 123}}, 'logged_in'], [{code: 0, data: {isLogin: false}}, 'not_logged_in'], [{code: -101}, 'not_logged_in'], [{code: -352}, 'unknown'], [{code: 0, data: {mid: 123}}, 'unknown']]) {
    const requests = [];
    const result = await vm.runInNewContext(`(${observeCurrentAccount.toString()})('bilibili', true)`, {
      location: {origin: 'https://www.bilibili.com', pathname: '/'},
      document: {querySelectorAll: () => [], cookie: 'stale_cookie=1'},
      fetch: async (url, options) => { requests.push([url, options.credentials]); return {ok: true, headers: {get: () => 'application/json'}, json: async () => data}; },
    });
    assert.equal(classifyIdentity(result), expected);
    assert.deepEqual(requests, [['https://api.bilibili.com/x/web-interface/nav', 'include']]);
    assert.equal(JSON.stringify(result).includes('123'), false);
  }
});

test('batch navigation drift rebinds exact probe before every observation',async()=>{
 let active=false;let n=0;
 const page={newTab:async()=> 'probe-tiktok',setActivePage:async id=>{assert.equal(id,'probe-tiktok');active=true;},sleep:async()=>{active=false;},evaluate:async()=>{assert.equal(active,true);n++;return n===4 ? {authenticated:true}:{};}};
 assert.equal(await readSiteLogin(page,'tiktok'),'logged_in');
 assert.equal(n,4);
});

test('Pixiv legacy and Next.js viewer state work without returning account or session data', async () => {
  for (const shape of ['legacy', 'next', 'both']) {
    const userData = {id:'123456789', name:'private-account'};
    const state = JSON.stringify({api:{token:'private-token'}, userData:{self:userData}, users:{'987':{id:'987'}}});
    const context = {location:{origin:'https://www.pixiv.net',pathname:'/en/'},
      document:{querySelectorAll:()=>[], querySelector: selector => selector.includes('global-data') && shape !== 'next'
        ? {getAttribute:()=>JSON.stringify({userData})}
        : selector.includes('__NEXT_DATA__') && shape !== 'legacy' ? {textContent:JSON.stringify({props:{pageProps:{serverSerializedPreloadedState:state}}})} : null}};
    const result = await vm.runInNewContext(`(${observeCurrentAccount.toString()})('pixiv', false)`, context);
    assert.equal(classifyIdentity(result),'logged_in',shape);
    assert.deepEqual(Object.keys(result),['authenticated']);
    assert.equal(/123456789|private|987/.test(JSON.stringify(result)),false);
  }
});

test('Pixiv anonymous, public artists, malformed data and conflicting viewers cannot prove login', async () => {
  for (const scenario of ['anonymous','public','malformed','too-large','conflict','wrong-origin','artist-path','captcha','login']) {
    const userData = scenario === 'public' ? {users:{'123':{id:'123'}}} : {self:{id:scenario === 'anonymous' ? '0' : '123'}};
    const state = scenario === 'malformed' ? '{' : scenario === 'too-large' ? 'x'.repeat(4*1024*1024+1) : JSON.stringify({userData});
    const context = {location:{origin:scenario === 'wrong-origin' ? 'https://www.pixiv.net.evil.example' : 'https://www.pixiv.net',pathname:scenario === 'artist-path' ? '/users/123' : '/'},
      document:{querySelectorAll:selector => scenario === 'captcha' && selector.includes('captcha') || scenario === 'login' && selector.includes('accounts.pixiv.net/login')
        ? [{getClientRects:()=>[{}]}] : [],
        querySelector:selector=>selector.includes('global-data') && scenario === 'conflict' ? {getAttribute:()=>JSON.stringify({userData:{id:'999'}})}
          : selector.includes('__NEXT_DATA__') && scenario !== 'login' ? {textContent:JSON.stringify({props:{pageProps:{serverSerializedPreloadedState:state}}})} : null}};
    const result = await vm.runInNewContext(`(${observeCurrentAccount.toString()})('pixiv', false)`,context);
    assert.equal(classifyIdentity(result),scenario === 'login' ? 'not_logged_in':'unknown',scenario);
  }
});
