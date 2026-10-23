// Project only public profile evidence; never substitute missing counts with zero.
export function projectProfile(data, username, expectedId) {
  const detail = data?.['__DEFAULT_SCOPE__']?.['webapp.user-detail'];
  const user = detail?.userInfo?.user;
  const stats = detail?.userInfo?.stats;
  const validText = (value, maximum) => typeof value === 'string'
    && new TextEncoder().encode(value).length <= maximum
    && !/[\u0000-\u0008\u000b\u000c\u000e-\u001f\u007f-\u009f]/.test(value);
  if (detail?.statusCode !== 0 || typeof user?.id !== 'string' || user.id !== expectedId
      || user.uniqueId !== username || !validText(user.nickname, 1000) || !user.nickname.trim()
      || !validText(user.signature, 16000) || typeof user.verified !== 'boolean'
      || typeof user.privateAccount !== 'boolean' || !Number.isSafeInteger(stats?.followerCount)
      || stats.followerCount < 0) throw new Error('ONECREW_TIKTOK_PROFILE_INVALID');
  return { adapter_version: '1.0.0', id: user.id, username: user.uniqueId, display_name: user.nickname,
    signature: user.signature, follower_count: stats.followerCount, verified: user.verified,
    private_account: user.privateAccount, profile_url: 'https://www.tiktok.com/@' + user.uniqueId,
    evidence_source: 'profile_bootstrap' };
}

export function readProfilePage(username, expectedId, project) {
  const current = new URL(location.href);
  if (current.origin !== 'https://www.tiktok.com' || current.pathname !== '/@' + username
      || current.username || current.password) throw new Error('ONECREW_TIKTOK_PROFILE_ORIGIN');
  const text = (document.body?.innerText ?? '').slice(0, 100000);
  if (/verify you are human|verify to continue|complete the puzzle|security verification|too many requests/i.test(text)) {
    throw new Error('ONECREW_TIKTOK_PROFILE_RISK');
  }
  if (/(?:log|sign) in to (?:view|see) this profile/i.test(text)) throw new Error('ONECREW_TIKTOK_PROFILE_AUTH');
  const nodes = document.querySelectorAll('script#__UNIVERSAL_DATA_FOR_REHYDRATION__');
  if (nodes.length !== 1) throw new Error('ONECREW_TIKTOK_PROFILE_INVALID');
  const bootstrap = nodes[0].textContent;
  if (typeof bootstrap !== 'string' || new TextEncoder().encode(bootstrap).length > 4 * 1024 * 1024) {
    throw new Error('ONECREW_TIKTOK_PROFILE_INVALID');
  }
  const data=JSON.parse(bootstrap);
  const observedId=data?.['__DEFAULT_SCOPE__']?.['webapp.user-detail']?.userInfo?.user?.id;
  if(expectedId===null && (typeof observedId!=='string'||!/^[1-9][0-9]{0,24}$/.test(observedId))) throw new Error('ONECREW_TIKTOK_PROFILE_INVALID');
  const profile=project(data, username, expectedId===null?observedId:expectedId);
  const links=[];
  for(const anchor of document.querySelectorAll('a[data-e2e="user-link"][href]')) {
    if(!anchor.getClientRects().length) continue;
    const observed=new URL(anchor.href,location.href);
    const target=observed.origin==='https://www.tiktok.com'&&observed.pathname==='/link/v2'?observed.searchParams.get('target'):observed.href;
    if(!target) throw new Error('ONECREW_TIKTOK_PROFILE_INVALID');
    const url=new URL(target);
    if(!['https:','http:'].includes(url.protocol)||url.username||url.password||url.port||url.href.length>2048) throw new Error('ONECREW_TIKTOK_PROFILE_INVALID');
    if(!links.includes(url.href)) links.push(url.href);
  }
  if(links.length>10||new TextEncoder().encode(links.join('\n')).length>6000) throw new Error('ONECREW_TIKTOK_PROFILE_INVALID');
  return [{...profile,contact_links:links}];
}

export async function readTikTokProfile(page, username, expectedId, resolveIdentity=false) {
  if (typeof username !== 'string' || !/^[A-Za-z0-9_.]{1,24}$/.test(username)
      || !resolveIdentity && (typeof expectedId !== 'string' || !/^[1-9][0-9]{0,24}$/.test(expectedId))) throw new Error('ONECREW_TIKTOK_PROFILE_INPUT');
  const identity = await page.newTab('https://www.tiktok.com/@' + username);
  if (!identity) throw new Error('ONECREW_TIKTOK_PROFILE_IDENTITY');
  try {
    await page.setActivePage(identity);
    // Numeric wait(n) may return on early DOM stability before hydration exists.
    await page.wait({ time: 3 });
    await page.evaluate(`(${restoreOriginalLanguage.toString()})()`);
    const result = await page.evaluate(`(${readProfilePage.toString()})(${JSON.stringify(username)}, ${JSON.stringify(resolveIdentity?null:expectedId)}, ${projectProfile.toString()})`);
    const rows = result && 'session' in result && 'data' in result ? result.data : result;
    if (!Array.isArray(rows) || rows.length !== 1) throw new Error('ONECREW_TIKTOK_PROFILE_INVALID');
    return rows;
  } finally {
    await page.closeTab(identity);
  }
}
import { restoreOriginalLanguage } from '../onecrew-browser-search/original-language.mjs';
