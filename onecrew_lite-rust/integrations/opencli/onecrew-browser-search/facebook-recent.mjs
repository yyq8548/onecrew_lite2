import {facebookProfileTarget} from './facebook-profile.mjs';
import {restoreOriginalLanguage} from './original-language.mjs';

export function projectFacebookRecent(data,id) {
  facebookProfileTarget(id);
  if(data?.author_id!==id || !Array.isArray(data.sources) || data.sources.length<1 || data.sources.length>3)
    throw new Error('ONECREW_FB_RECENT_INVALID');
  const seen=new Set();
  return data.sources.map(source=>{
    if(typeof source!=='string' || !/^https:\/\/www\.facebook\.com\/reel\/[1-9][0-9]{0,24}$/.test(source) || seen.has(source))
      throw new Error('ONECREW_FB_RECENT_INVALID');
    seen.add(source);
    return {author_id:id,source_url:source};
  });
}

export async function readFacebookRecentPage(source,id,restore) {
  const base=`https://www.facebook.com/profile.php?id=${id}`;
  const check=()=>{
    if(location.origin!=='https://www.facebook.com') throw new Error('ONECREW_FB_RECENT_INVALID');
    if(/^\/(?:login|recover)(?:\/|\.php|$)/.test(location.pathname)) throw new Error('ONECREW_COMMENTS_AUTH');
    if(/^\/(?:checkpoint|challenge|two_step_verification)(?:\/|$)/.test(location.pathname)) throw new Error('ONECREW_COMMENTS_RISK');
    if(location.href!==source) throw new Error('ONECREW_FB_RECENT_INVALID');
  };
  for(let attempt=0;attempt<16;attempt++) {
    check();
    const mains=[...document.querySelectorAll('[role="main"]')].filter(n=>n.getClientRects().length);
    if(mains.length>1) throw new Error('ONECREW_FB_RECENT_INVALID');
    if(mains.length===1) {
      const roots=mains[0];
      const ownerLinks=[...roots.querySelectorAll('a[href]')].filter(a=>a.getClientRects().length
        && a.getAttribute('role')!=='tab' && a.href===`${base}&sk=followers`);
      if(ownerLinks.length>1) throw new Error('ONECREW_FB_RECENT_INVALID');
      if(ownerLinks.length===1) {
        await restore(); check();
        if(ownerLinks[0].href!==`${base}&sk=followers`) throw new Error('ONECREW_FB_RECENT_INVALID');
        const sources=[];
        for(const anchor of [...roots.querySelectorAll('a[href]')].slice(0,500)) {
          if(!anchor.getClientRects().length) continue;
          const url=new URL(anchor.href);
          const match=/^\/reel\/([1-9][0-9]{0,24})\/$/.exec(url.pathname);
          if(url.origin!=='https://www.facebook.com' || !match || url.searchParams.get('s')!=='fb_shorts_profile') continue;
          const canonical=`https://www.facebook.com/reel/${match[1]}`;
          if(!sources.includes(canonical)) sources.push(canonical);
          if(sources.length===3) return {author_id:id,sources};
        }
        if(sources.length) return {author_id:id,sources};
      }
    }
    if(attempt<15) await new Promise(resolve=>setTimeout(resolve,500));
  }
  throw new Error('ONECREW_FB_RECENT_UNAVAILABLE');
}

export async function readFacebookRecent(page,args,ownPage) {
  const source=`${facebookProfileTarget(args.id)}&sk=reels_tab`;
  return ownPage(page,async owned=>{
    await owned.goto(source,{settleMs:3000});
    const raw=await owned.evaluate(`(${readFacebookRecentPage.toString()})(${JSON.stringify(source)},${JSON.stringify(args.id)},${restoreOriginalLanguage.toString()})`);
    const data=raw && 'session' in Object(raw) && 'data' in Object(raw) ? raw.data : raw;
    return projectFacebookRecent(data,args.id);
  });
}
