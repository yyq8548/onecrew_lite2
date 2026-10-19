import {facebookVanitySource,facebookHeaderNodes} from './facebook-identity.mjs';
import {readFacebookNativeHeader} from './facebook-native-profile.mjs';
import {facebookProfileTarget} from './facebook-profile.mjs';
import {facebookReelTarget} from './facebook-reel-comments.mjs';

// Only the public story at an exact native response boundary binds caption to
// its video and actor. Feed actors, viewers and translated messages are excluded.
export function facebookOriginalNodes(value,post) {
  const rows=[];let remaining=30000;
  const walk=(v,depth)=>{
    if(!v||typeof v!=='object'||depth>24||remaining--<=0) return;
    const s=v.__bbox?.result?.data;
    if(s?.attachments?.length===1&&s.attachments[0]?.media?.__typename==='Video'&&s.attachments[0].media.id===post&&s.actors?.length===1) {
      const a=s.actors[0],media=s.attachments[0].media;
      if(['User','Page'].includes(a.__typename)&&media.owner?.id===a.id&&media.owner?.url===a.url)
        rows.push({post_id:post,author_id:a.id,author_url:a.url,display_name:a.name,is_reshare:s.is_reshare,text:s.message?.text??null});
    }
    for(const item of Object.values(v)) if(item&&typeof item==='object')walk(item,depth+1);
  };
  walk(value,0);return rows;
}

export async function readFacebookNativeOriginalPage(source,id,base,collect) {
  const post=source.split('/').at(-1);
  for(let attempt=0;attempt<12;attempt++) {
    if(location.origin!=='https://www.facebook.com')throw new Error('ONECREW_FACEBOOK_SOURCE_INVALID');
    if(/^\/(?:login|recover)(?:\/|\.php|$)/.test(location.pathname))throw new Error('ONECREW_COMMENTS_AUTH');
    if(/^\/(?:checkpoint|challenge|two_step_verification)(?:\/|$)/.test(location.pathname))throw new Error('ONECREW_COMMENTS_RISK');
    if(location.href.replace(/\/$/,'')!==source||location.search||location.hash)throw new Error('ONECREW_FACEBOOK_SOURCE_INVALID');
    const visible=n=>{const r=n.getBoundingClientRect();return r.width>0&&r.height>0&&r.top<innerHeight&&r.bottom>0&&r.left<innerWidth&&r.right>0;};
    const players=[...document.querySelectorAll('[role="main"] [role="group"][aria-label="Video player"]')].filter(visible);
    if(players.length>1)throw new Error('ONECREW_FACEBOOK_SOURCE_AMBIGUOUS');
    const scripts=[...document.querySelectorAll('script[type="application/json"][data-sjs]')];
    if(scripts.length>512)throw new Error('ONECREW_FACEBOOK_SOURCE_INVALID');
    let size=0;const rows=[];
    for(const script of scripts){const text=script.textContent||'';size+=text.length;if(size>16000000)throw new Error('ONECREW_FACEBOOK_SOURCE_INVALID');
      if(text.length>3000000)continue;let v;try{v=JSON.parse(text);}catch{continue;}rows.push(...collect(v,post));}
    if(rows.length) {
      if(rows.some(r=>r.author_id!==id||typeof r.author_url!=='string'||r.author_url.replace(/\/$/,'').toLowerCase()!==base.toLowerCase()||r.is_reshare!==false)
        ||new Set(rows.map(r=>r.text)).size!==1)throw new Error('ONECREW_FACEBOOK_SOURCE_INVALID');
      const row=rows[0];
      if(typeof row.text!=='string'||!row.text.trim())throw new Error('ONECREW_FB_ORIGINAL_UNAVAILABLE');
      if(players.length===1){const links=[...players[0].querySelectorAll('h2 a[href]')];
        if(links.length===1){const u=new URL(links[0].href),ids=u.searchParams.getAll('id');
          const matches=u.origin==='https://www.facebook.com'&&u.pathname==='/profile.php'&&ids.length===1&&ids[0]===id;
          if(!matches||links[0].innerText.trim()!==row.display_name)throw new Error('ONECREW_FACEBOOK_SOURCE_INVALID');
          const chars=Array.from(row.text);
          return {post_id:post,author_id:id,source_url:source,text:chars.slice(0,8000).join(''),text_truncated:chars.length>8000||/…\s*See more\s*$/u.test(row.text)};
        }
      }
    }
    if(attempt<11)await new Promise(resolve=>setTimeout(resolve,500));
  }
  throw new Error('ONECREW_FB_ORIGINAL_UNAVAILABLE');
}

export async function readFacebookNativeOriginal(page,args,ownPage) {
  facebookProfileTarget(args.id);const base=facebookVanitySource(args.url),source=facebookReelTarget(args.source).source;
  return ownPage(page,async owned=>{await owned.goto(source,{settleMs:3000});
    const raw=await owned.evaluate(`(${readFacebookNativeOriginalPage.toString()})(${JSON.stringify(source)},${JSON.stringify(args.id)},${JSON.stringify(base)},${facebookOriginalNodes.toString()})`);
    return raw&&'session' in Object(raw)&&'data' in Object(raw)?raw.data:raw;
  });
}

export async function readFacebookNativeTargets(page,args,ownPage) {
  facebookProfileTarget(args.id);const base=facebookVanitySource(args.url),source=`${base}/reels_tab`;
  return ownPage(page,async owned=>{await owned.goto(source,{settleMs:3000});
    const raw=await owned.evaluate(String.raw`(async()=>{const allowed=[${JSON.stringify(source)},${JSON.stringify(base+'/reels/')},${JSON.stringify(base+'/reels')}];
      if(!allowed.includes(location.href))throw new Error('ONECREW_FB_RECENT_INVALID');
      await (${readFacebookNativeHeader.toString()})(location.href,${JSON.stringify(args.id)},${JSON.stringify(base)},${facebookHeaderNodes.toString()});
      const main=[...document.querySelectorAll('[role="main"]')].find(n=>n.getClientRects().length);
      if(!main)throw new Error('ONECREW_FB_RECENT_INVALID');
      const tabs=[...main.querySelectorAll('[role="tab"][aria-selected="true"]')].filter(n=>n.getClientRects().length&&n.href===${JSON.stringify(source)});
      if(tabs.length!==1)throw new Error('ONECREW_FB_RECENT_INVALID');
      const originals=[...main.querySelectorAll('[role="tab"][aria-selected="true"]')].filter(n=>n.getClientRects().length&&n.href===${JSON.stringify(base+'/owner_reels')});
      if(originals.length!==1)throw new Error('ONECREW_FB_RECENT_INVALID');
      const urls=[];for(const a of [...main.querySelectorAll('a[href]')].slice(0,500)){if(!a.getClientRects().length)continue;const u=new URL(a.href);
        const m=/^\/reel\/([1-9][0-9]{0,24})\/$/.exec(u.pathname);if(u.origin!=='https://www.facebook.com'||!m||u.searchParams.get('s')!=='fb_shorts_profile')continue;
        const url='https://www.facebook.com/reel/'+m[1];if(!urls.includes(url))urls.push(url);if(urls.length===3)break;}
      if(!urls.length)throw new Error('ONECREW_FB_RECENT_UNAVAILABLE');return urls.map(source_url=>({author_id:${JSON.stringify(args.id)},source_url}));})()`);
    return raw&&'session' in Object(raw)&&'data' in Object(raw)?raw.data:raw;
  });
}
