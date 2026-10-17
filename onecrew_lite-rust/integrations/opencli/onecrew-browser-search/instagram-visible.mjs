// 只读取正常网页显示的资料，不调用 Instagram 数据接口。
export async function observeInstagramVisibleProfile(username, authorId) {
  const visible = node => node.getClientRects().length > 0;
  for (let poll = 0; poll < 32; poll++) {
    if (location.origin !== 'https://www.instagram.com') return {stop:'source'};
    if (/^\/accounts\/login/.test(location.pathname)) return {stop:'auth'};
    if (/^\/(challenge|checkpoint)\//.test(location.pathname)) return {stop:'risk'};
    if (location.pathname.toLowerCase() !== `/${username}/`) return {stop:'identity'};
    const main = document.querySelector('main');
    const lines = (main?.innerText || '').split('\n').map(s => s.trim()).filter(Boolean);
    if (/Try again later|请求太频繁|稍后重试|操作过于频繁/i.test(lines.join('\n'))) return {stop:'risk'};
    const heading = main?.querySelector('h1,h2');
    const isPrivate = value => /This (?:profile|account) is private|这是私密(?:账户|帐号|账号)|此(?:帐户|账户|帐号|账号)(?:是|为)私密/i.test(value);
    const privateAccount = lines.some(isPrivate);
    const follower = lines.find(s => /^(?:[0-9][0-9,.\s]*[KMB万亿千]?\s*)(?:followers|粉丝|位粉丝)$/i.test(s));
    if (heading?.textContent?.trim().toLowerCase() !== username || !follower && !privateAccount) {
      if (poll < 31) await new Promise(resolve => setTimeout(resolve,250));
      continue;
    }
    const more = [...main.querySelectorAll('button,[role="button"]')].find(n => visible(n) && /^(更多|more)$/i.test(n.innerText.trim()));
    if (more) { more.click(); await new Promise(resolve => setTimeout(resolve,100)); }
    const updated = main.innerText.split('\n').map(s => s.trim()).filter(Boolean);
    const following = updated.findIndex(s => /^[0-9][0-9,.\s]*[KMB万亿千]?\s*(?:following|关注)$/i.test(s));
    const bioLines = following < 0 ? [] : updated.slice(following+1);
    const end = bioLines.findIndex(s => /^(?:关注|已关注|发消息|Follow|Following|Message|这是私密账户|This account is private)$/i.test(s));
    const biography = (end < 0 ? [] : bioLines.slice(0,end)).filter(s => !/^(?:更多|more|\.\.\.)$/i.test(s)).join('\n').slice(0,2000);
    const emails = [...new Set(biography.match(/[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}/g) || [])];
    const exact = (follower || '').replace(/(?:followers|位粉丝|粉丝)/i,'').trim();
    const numeric = /^(?:[0-9]+|[0-9]{1,3}(?:,[0-9]{3})+)$/.test(exact) ? Number(exact.replaceAll(',','')) : null;
    const links = [...main.querySelectorAll('a[href]')].filter(visible).map(a => a.href).filter(href => {
      const url = new URL(href);
      return ['http:','https:'].includes(url.protocol) && !['www.instagram.com','instagram.com','l.instagram.com'].includes(url.hostname) && !url.username && !url.password;
    });
    return {profile:{adapter_version:'1.0.0',id:authorId,username,
      display_name:(updated[1] || username).slice(0,200),biography,
      follower_count:Number.isSafeInteger(numeric) ? numeric : null,...(follower ? {follower_text:follower} : {}),
      private_account:privateAccount ? true : null,verified:null,
      public_email:emails.length === 1 ? emails[0] : null,contact_links:[...new Set(links)].slice(0,10),
      profile_url:`https://www.instagram.com/${username}/`,evidence_source:'public_profile_dom_search_bound',web_text:''}};
  }
  return {stop:'timeout'};
}

export async function observeInstagramVisiblePosts(username) {
  if (location.origin !== 'https://www.instagram.com' || location.pathname.toLowerCase() !== `/${username}/`) return {stop:'source'};
  const main = document.querySelector('main');
  if (!main) return {stop:'read'};
  if (/This (?:profile|account) is private|这是私密账户|此帐户是私密帐户/.test(main.innerText)) return {stop:'private'};
  const posts = [];
  for (const a of main.querySelectorAll('a[href]')) {
    if (!a.getClientRects().length) continue;
    const url = new URL(a.href);
    const match = /^\/([^/]+)\/(p|reel)\/([A-Za-z0-9_-]{1,32})\/$/.exec(url.pathname);
    if (url.origin !== location.origin || !match || match[1].toLowerCase() !== username) continue;
    const preview = Array.from([a.innerText, a.getAttribute('aria-label'), ...[...a.querySelectorAll('img[alt]')].map(img => img.alt)].filter(Boolean).join('\n')).slice(0,500).join('');
    if (!posts.some(p => p.code === match[3])) posts.push({code:match[3],url:url.href,kind:match[2],preview});
    if (posts.length === 6) break;
  }
  return {posts};
}

export async function observeInstagramVisibleCaption(username, code) {
  for (let poll = 0; poll < 32; poll++) {
    if (location.origin !== 'https://www.instagram.com') return {stop:'source'};
    if (/^\/accounts\/login/.test(location.pathname)) return {stop:'auth'};
    if (/^\/(challenge|checkpoint)\//.test(location.pathname)) return {stop:'risk'};
    const current = /\/(?:p|reel)\/([A-Za-z0-9_-]+)\/$/.exec(location.pathname);
    if (current?.[1] !== code) return {stop:'source'};
    if (/Try again later|请求太频繁|稍后重试|操作过于频繁/i.test(document.body.innerText)) return {stop:'risk'};
    const containerFor = n => {
      const desktop = n.parentElement?.parentElement;
      if (desktop?.querySelector(`h2 a[href="/${username}/"]`)) return desktop;
      const mobile = desktop?.parentElement;
      const author = mobile?.querySelector(`a[href="/${username}/"]`);
      return mobile?.querySelectorAll('h1').length === 1 && author?.innerText?.trim().toLowerCase() === username ? mobile : null;
    };
    const captions = [...document.querySelectorAll('h1')].filter(n => n.getClientRects().length && containerFor(n));
    if (!captions.length) {
      const main = document.querySelector('main');
      for (const marker of main?.querySelectorAll('time') || []) {
        if (!marker.getClientRects().length || marker.closest('a[href]')) continue;
        let node = marker.parentElement;
        for (let depth = 0; depth < 6 && node && node !== main; depth++, node = node.parentElement) {
          const children = [...node.children];
          if (children.length !== 2 || !children[0].contains(marker) || children[1].contains(marker)) continue;
          const author = children[0].querySelector(`a[href="/${username}/"]`);
          if (author?.innerText?.trim().toLowerCase() !== username || !children[1].innerText?.trim()) continue;
          if (!captions.includes(children[1])) captions.push(children[1]);
          break;
        }
      }
    }
    if (captions.length > 1) return {stop:'identity'};
    if (captions.length === 1) {
      const more = [...(containerFor(captions[0]) || captions[0]).querySelectorAll('button,[role="button"]')].find(n => n.getClientRects().length && /^(更多|more)$/i.test(n.innerText.trim()));
      if (more) { more.click(); await new Promise(resolve => setTimeout(resolve,100)); if (poll < 31) continue; return {stop:'timeout'}; }
      const original = captions[0].innerText;
      let text = '', size = 0;
      for (const char of original) { const bytes = new TextEncoder().encode(char).length; if (size+bytes > 8000) break; text += char; size += bytes; }
      return {text,text_truncated:text !== original};
    }
    if (poll < 31) await new Promise(resolve => setTimeout(resolve,250));
  }
  return {stop:'timeout'};
}
