import {readProfilePage, projectProfile} from '../onecrew-tiktok-profile/profile.mjs';

export async function readRecentPage(username, authorId, readProfile, project) {
  readProfile(username, authorId, project); // Bind the visible list to the known post author.
  for (let attempt = 0; attempt < 20; attempt++) {
    const bodyText = document.body?.innerText ?? '';
    if (/verify you are human|complete the puzzle|security verification|too many requests/i.test(bodyText)) throw new Error('ONECREW_TIKTOK_PROFILE_RISK');
    if (/(?:log|sign) in to (?:view|see)/i.test(bodyText)) throw new Error('ONECREW_TIKTOK_PROFILE_AUTH');
    // Return before navigation: clicking Refresh inside this pending evaluation
    // can unload its execution context and strand the RPC until timeout.
    if (/Something went wrong/.test(bodyText) && /Please try again later/.test(bodyText)) {
      const button = [...document.querySelectorAll('button')].find(n=>n.textContent.trim()==='Refresh' && n.getClientRects().length);
      if (button) return {retry_list:true};
    }
    const roots = document.querySelectorAll('[data-e2e="user-post-item-list"]');
    if (roots.length > 1) throw new Error('ONECREW_TIKTOK_RECENT_INVALID');
    if (roots.length === 1) {
      const urls = [];
      for (const link of [...roots[0].querySelectorAll('a[href]')].slice(0,100)) {
        const url = new URL(link.href, location.href);
        const prefix = `/@${username}/video/`;
        if (!url.pathname.startsWith(prefix)) continue;
        const id = url.pathname.slice(prefix.length);
        if (url.origin !== 'https://www.tiktok.com' || url.username || url.password || url.search || url.hash
            || !/^[1-9][0-9]{0,24}$/.test(id)) throw new Error('ONECREW_TIKTOK_RECENT_INVALID');
        if (!urls.includes(url.href)) urls.push(url.href);
        if (urls.length === 3) break;
      }
      if (urls.length) return urls.map(source_url => ({source_url}));
    }
    if (attempt < 19) await new Promise(resolve => setTimeout(resolve,500));
  }
  const login = /log in to (?:view|see)|sign in to (?:view|see)/i.test(document.body?.innerText ?? '');
  throw new Error(`ONECREW_TIKTOK_RECENT_UNAVAILABLE: lists=${document.querySelectorAll('[data-e2e="user-post-item-list"]').length} video_links=${document.querySelectorAll('a[href*="/video/"]').length} login_gate=${login}`);
}

export async function readTikTokRecent(page, username, authorId, ownPage) {
  if (!/^[A-Za-z0-9_.]{1,24}$/.test(username) || !/^[1-9][0-9]{0,24}$/.test(authorId)) throw new Error('ONECREW_TIKTOK_RECENT_INPUT');
  return ownPage(page, async owned => {
    for (let attempt=0;attempt<2;attempt++) {
      await owned.goto(`https://www.tiktok.com/@${username}`,{settleMs:3000});
      const result = await owned.evaluate(`(${readRecentPage.toString()})(${JSON.stringify(username)},${JSON.stringify(authorId)},${readProfilePage.toString()},${projectProfile.toString()})`);
      const value = result && 'session' in Object(result) && 'data' in Object(result) ? result.data : result;
      if (value?.retry_list !== true) return value;
    }
    throw new Error('ONECREW_TIKTOK_RECENT_UNAVAILABLE: failed_list');
  });
}
