// Resolve a saved vanity profile from the native public header response.
// Names, feed actors, image URLs and viewer IDs never supply the identity.
import {equivalentSourceUrl,sourceUrlScript} from './source-url.mjs';

export function facebookVanitySource(source) {
  if (typeof source !== 'string' || !source.startsWith('https://www.facebook.com/')) throw new Error('ONECREW_FB_IDENTITY_INPUT');
  let url;
  try { url = new URL(source); } catch { throw new Error('ONECREW_FB_IDENTITY_INPUT'); }
  const reserved = new Set(['about','ads','business','challenge','checkpoint','directory','events','friends','gaming','groups','help','home','login','login.php','marketplace','messages','notifications','pages','photo','photos','privacy','profile','profile.php','recover','reel','reels','search','settings','share','stories','two_step_verification','watch']);
  const match = /^\/([A-Za-z0-9.]{1,100})\/?$/.exec(url.pathname);
  if (url.origin !== 'https://www.facebook.com' || url.username || url.password || url.search || url.hash
    || !match || reserved.has(match[1].toLowerCase()) || /^[0-9.]+$/.test(match[1])) throw new Error('ONECREW_FB_IDENTITY_INPUT');
  return `https://www.facebook.com/${match[1]}`;
}

export function projectFacebookIdentity(data, requested) {
  const source = facebookVanitySource(requested);
  if (data?.source_url !== requested || typeof data.id !== 'string' || !/^[1-9][0-9]{0,19}$/.test(data.id)
    || typeof data.display_name !== 'string' || !data.display_name.trim() || data.display_name.length > 200
    || /[\u0000-\u001f\u007f]/.test(data.display_name) || typeof data.profile_url !== 'string') throw new Error('ONECREW_FB_IDENTITY_INVALID');
  let observed;
  try { observed = facebookVanitySource(data.profile_url); } catch { throw new Error('ONECREW_FB_IDENTITY_INVALID'); }
  if (observed.toLowerCase() !== source.toLowerCase()) throw new Error('ONECREW_FB_IDENTITY_INVALID');
  return {adapter_version:'1.0.0', source_url:requested, id:data.id,
    display_name:data.display_name.trim(), profile_url:data.profile_url,
    evidence_source:'native_profile_header_response'};
}

// This is the exact public response subtree observed on the real profile.
// Stop at response bodies instead of recursively looking for names or IDs.
export function facebookHeaderNodes(payload) {
  const found=[]; let visited=0;
  function walk(node, depth) {
    if (!node || typeof node !== 'object' || depth>24 || ++visited>10000) return;
    const box=node.__bbox;
    const user=box?.result?.data?.user?.profile_header_renderer?.user;
    if (user && ['User','Page'].includes(user.__typename)) found.push({id:user.id,display_name:user.name,profile_url:user.url});
    for (const [key,value] of Object.entries(node)) {
      if (key==='__bbox' && box?.result) continue;
      if (value && typeof value==='object') walk(value, depth+1);
    }
  }
  walk(payload,0);
  return found;
}

export async function readFacebookIdentityPage(requested, source, collect) {
  for (let attempt=0;attempt<16;attempt++) {
    if (!equivalentSourceUrl(location.origin,'https://www.facebook.com')) throw new Error('ONECREW_FB_IDENTITY_INVALID');
    if (/^\/(?:login|recover)(?:\/|\.php|$)/.test(location.pathname)) throw new Error('ONECREW_FB_IDENTITY_AUTH');
    if (/^\/(?:checkpoint|challenge|two_step_verification)(?:\/|$)/.test(location.pathname)) throw new Error('ONECREW_FB_IDENTITY_RISK');
    if (!equivalentSourceUrl(location.href,source)) throw new Error('ONECREW_FB_IDENTITY_INVALID');
    const mains=[...document.querySelectorAll('[role="main"]')].filter(n=>n.getClientRects().length);
    if (mains.length>1) throw new Error('ONECREW_FB_IDENTITY_INVALID');
    const candidates=[];
    const scripts=[...document.querySelectorAll('script[type="application/json"][data-sjs]')];
    if (scripts.length>256) throw new Error('ONECREW_FB_IDENTITY_INVALID');
    let size=0;
    for (const script of scripts) {
      const text=script.textContent || '';
      size+=text.length;
      if (size>16000000) throw new Error('ONECREW_FB_IDENTITY_INVALID');
      if (text.length>3000000) continue;
      let payload; try { payload=JSON.parse(text); } catch { continue; }
      for (const node of collect(payload)) {
        if (typeof node.id!=='string' || !/^[1-9][0-9]{0,19}$/.test(node.id)
          || typeof node.profile_url!=='string' || node.profile_url.replace(/\/$/,'').toLowerCase()!==source.toLowerCase()) continue;
        candidates.push(node);
      }
    }
    if (new Set(candidates.map(n=>n.id)).size>1) throw new Error('ONECREW_FB_IDENTITY_INVALID');
    if (mains.length===1 && candidates.length) {
      const node=candidates[0];const main=mains[0];
      const visible=n=>n.getClientRects().length && !n.closest('[role="article"],[role="feed"],[aria-hidden="true"]');
      const owner=[...main.querySelectorAll('a[href]')].filter(a=>visible(a) && a.getAttribute('role')!=='tab' && a.href.replace(/\/$/,'').toLowerCase()===`${source}/followers`.toLowerCase());
      const names=[...main.querySelectorAll('h1,[role="button"]')].filter(n=>visible(n) && n.innerText.trim()===node.display_name);
      // Facebook can render desktop/mobile header buttons for the same name.
      // Identity uniqueness comes from the native ID and exact owner URL.
      if (owner.length===1 && names.length>=1) return {...node,source_url:requested};
    }
    if (attempt<15) await new Promise(resolve=>setTimeout(resolve,500));
  }
  throw new Error('ONECREW_FB_IDENTITY_TIMEOUT');
}

export async function readFacebookIdentity(page,args,ownedPage) {
  const source=facebookVanitySource(args.url);
  return ownedPage(page,async owned=>{
    await owned.goto(args.url,{settleMs:3000});
    const raw=await owned.evaluate(sourceUrlScript(`(${readFacebookIdentityPage.toString()})(${JSON.stringify(args.url)},${JSON.stringify(source)},${facebookHeaderNodes.toString()})`));
    const data=raw && 'session' in Object(raw) && 'data' in Object(raw) ? raw.data : raw;
    return projectFacebookIdentity(data,args.url);
  });
}
