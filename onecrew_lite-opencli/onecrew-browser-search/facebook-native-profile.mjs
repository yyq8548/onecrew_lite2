import {facebookVanitySource,facebookHeaderNodes} from './facebook-identity.mjs';
import {facebookProfileTarget,observeFacebookAbout} from './facebook-profile.mjs';
import {equivalentSourceUrl,sourceUrlScript} from './source-url.mjs';

export function facebookNativeContactTarget(base,source) {
  const profile=facebookVanitySource(base);
  if(![`${profile}/directory_contact_info`,`${profile}/directory_links`].includes(source)) throw new Error('ONECREW_FB_CONTACT_INVALID');
  return source;
}

// One public header subtree, the exact saved URL, and a visible owner relationship
// must agree. Never use a feed actor or the signed-in viewer as the profile ID.
export async function readFacebookNativeHeader(expected,id,base,collect) {
  for(let attempt=0;attempt<12;attempt++) {
    if(!equivalentSourceUrl(location.origin,'https://www.facebook.com')) throw new Error('ONECREW_FB_PROFILE_INVALID');
    if(/^\/(?:login|recover)(?:\/|\.php|$)/.test(location.pathname)) throw new Error('ONECREW_FB_PROFILE_AUTH');
    if(/^\/(?:checkpoint|challenge|two_step_verification)(?:\/|$)/.test(location.pathname)) throw new Error('ONECREW_FB_PROFILE_RISK');
    if(!equivalentSourceUrl(location.href,expected)) throw new Error('ONECREW_FB_PROFILE_INVALID');
    const mains=[...document.querySelectorAll('[role="main"]')].filter(n=>n.getClientRects().length);
    if(mains.length>1) throw new Error('ONECREW_FB_PROFILE_INVALID');
    const scripts=[...document.querySelectorAll('script[type="application/json"][data-sjs]')];
    if(scripts.length>512) throw new Error('ONECREW_FB_PROFILE_INVALID');
    let size=0;const owners=[];
    for(const script of scripts) {
      const text=script.textContent||'';size+=text.length;
      if(size>16000000) throw new Error('ONECREW_FB_PROFILE_INVALID');
      if(text.length>3000000) continue;
      let value;try{value=JSON.parse(text);}catch{continue;}
      for(const row of collect(value)) if(typeof row.profile_url==='string' && row.profile_url.replace(/\/$/,'').toLowerCase()===base.toLowerCase()) owners.push(row);
    }
    const ids=new Set(owners.map(n=>n.id));
    if(ids.size>1 || (ids.size===1 && !ids.has(id))) throw new Error('ONECREW_FB_PROFILE_INVALID');
    if(mains.length===1 && owners.length) {
      const owner=owners[0],main=mains[0];
      const visible=n=>n.getClientRects().length && !n.closest('[role="article"],[role="feed"],[aria-hidden="true"]');
      const followers=[...main.querySelectorAll('a[href]')].filter(n=>visible(n)&&n.getAttribute('role')!=='tab'&&n.href.replace(/\/$/,'').toLowerCase()===`${base}/followers`.toLowerCase()&&/^[0-9][0-9,.]*\s*[KMB]? followers?$/i.test(n.innerText.trim()));
      const names=[...main.querySelectorAll('h1,[role="button"]')].filter(n=>visible(n)&&n.innerText.trim()===owner.display_name);
      if(followers.length===1 && names.length>=1) return {id,display_name:owner.display_name,profile_url:base,follower_text:followers[0].innerText.trim()};
    }
    if(attempt<11) await new Promise(resolve=>setTimeout(resolve,500));
  }
  throw new Error('ONECREW_FB_PROFILE_TIMEOUT');
}

export function projectFacebookNativeProfile(data,id,base) {
  const profile=facebookVanitySource(base),canonical=facebookProfileTarget(id);
  if(data?.id!==id||data.observed_profile_url!==profile||data.profile_url!==canonical
    ||typeof data.display_name!=='string'||!data.display_name.trim()||Array.from(data.display_name).length>200||/[\u0000-\u001f\u007f]/.test(data.display_name)
    ||typeof data.follower_text!=='string'||data.follower_text.length>100||/[\u0000-\u001f\u007f]/.test(data.follower_text)
    ||!Array.isArray(data.contact_links)||data.contact_links.length>2||new Set(data.contact_links).size!==data.contact_links.length) throw new Error('ONECREW_FB_PROFILE_INVALID');
  for(const link of data.contact_links) facebookNativeContactTarget(profile,link);
  const match=/^(0|[1-9][0-9]*|[1-9][0-9]{0,2}(?:,[0-9]{3})+) followers?$/i.exec(data.follower_text.trim());
  const count=match?Number(match[1].replaceAll(',','')):null;
  if(count!==null&&!Number.isSafeInteger(count)) throw new Error('ONECREW_FB_PROFILE_INVALID');
  return {adapter_version:'2.0.0',id,display_name:data.display_name.trim(),follower_count:count,follower_text:data.follower_text.trim(),
    profile_url:canonical,observed_profile_url:profile,contact_links:data.contact_links,evidence_source:'native_profile_header_response',
    ...(typeof data.biography==='string'?{biography:data.biography,bio_truncated:data.bio_truncated,bio_status:data.bio_status}:{})};
}

export async function readFacebookNativeProfile(page,args,ownPage) {
  const base=facebookVanitySource(args.url),canonical=facebookProfileTarget(args.id),source=`${base}/about`;
  return ownPage(page,async owned=>{
    await owned.goto(source,{settleMs:3000});
    const raw=await owned.evaluate(sourceUrlScript(String.raw`(async()=>{const owner=await (${readFacebookNativeHeader.toString()})(${JSON.stringify(source)},${JSON.stringify(args.id)},${JSON.stringify(base)},${facebookHeaderNodes.toString()});
      const main=[...document.querySelectorAll('[role="main"]')].find(n=>n.getClientRects().length);
      const links=[...main.querySelectorAll('a[role="tab"][href]')].filter(n=>n.getClientRects().length && !n.closest('[role="article"],[role="feed"]')).map(n=>n.href);
      const about=await (${observeFacebookAbout.toString()})(${JSON.stringify(source)},${JSON.stringify(base)});
      return {...owner,...about,profile_url:${JSON.stringify(canonical)},observed_profile_url:${JSON.stringify(base)},contact_links:[${JSON.stringify(`${base}/directory_contact_info`)},${JSON.stringify(`${base}/directory_links`)}].filter(target=>links.some(url=>equivalentSourceUrl(url,target)))};})()`));
    const data=raw&&'session' in Object(raw)&&'data' in Object(raw)?raw.data:raw;
    return projectFacebookNativeProfile(data,args.id,base);
  });
}

export function projectFacebookNativeContact(data,id,base,source) {
  facebookProfileTarget(id);facebookNativeContactTarget(base,source);
  if(data?.id!==id||data.source_url!==source||data.observed_profile_url!==facebookVanitySource(base)
    ||typeof data.title!=='string'||!data.title.trim()||Array.from(data.title).length>200||/[\u0000-\u001f\u007f]/.test(data.title)
    ||typeof data.contact_text!=='string'||data.contact_text.length>12000||data.contact_text.includes('\0')
    ||!Array.isArray(data.contact_links)||data.contact_links.length>10) throw new Error('ONECREW_FB_CONTACT_INVALID');
  for(const link of data.contact_links) {
    let url;try{url=new URL(link);}catch{throw new Error('ONECREW_FB_CONTACT_INVALID');}
    if(!['http:','https:'].includes(url.protocol)||url.username||url.password||link.length>2048||/[\u0000-\u001f\u007f]/.test(link)) throw new Error('ONECREW_FB_CONTACT_INVALID');
  }
  return {adapter_version:'2.0.0',id,source_url:source,observed_profile_url:facebookVanitySource(base),title:data.title,contact_text:data.contact_text,contact_links:data.contact_links,truncated:true};
}

export async function readFacebookNativeContact(page,args,ownPage) {
  const base=facebookVanitySource(args.url),source=facebookNativeContactTarget(base,args.source);facebookProfileTarget(args.id);
  return ownPage(page,async owned=>{
    await owned.goto(source,{settleMs:3000});
    const raw=await owned.evaluate(sourceUrlScript(String.raw`(async()=>{const owner=await (${readFacebookNativeHeader.toString()})(${JSON.stringify(source)},${JSON.stringify(args.id)},${JSON.stringify(base)},${facebookHeaderNodes.toString()});
      const main=[...document.querySelectorAll('[role="main"]')].find(n=>n.getClientRects().length);
      const visible=n=>n.getClientRects().length && !n.closest('[role="article"],[role="feed"],[role="navigation"],nav,script,style,[aria-hidden="true"]');
      const selected=[...main.querySelectorAll('a[role="tab"][aria-selected="true"]')].filter(n=>visible(n)&&equivalentSourceUrl(n.href,${JSON.stringify(source)}));
      if(selected.length!==1||[...main.querySelectorAll('[aria-busy="true"],[role="progressbar"]')].some(visible)) throw new Error('ONECREW_FB_CONTACT_INVALID');
      const sections=[...main.querySelectorAll('section')].filter(n=>visible(n)&&!n.querySelector('section')&&!n.closest('[role="article"],[role="feed"]'));
      const parts=[],links=[];
      for(const section of sections) {
        const headings=[...section.querySelectorAll('h1,h2,h3,[role="heading"]')].filter(visible);
        if(headings.length!==1) continue;
        // A section with its own heading is the native About field container.
        // Reviews, unrelated modules and whole-page text never become contacts.
        const title=headings[0].innerText.trim();
        if(/^(Reviews|Ratings|评论|評論|评价|評價)$/i.test(title)) continue;
        const text=section.innerText.trim();if(text.length>12000) continue;
        if(${JSON.stringify(source.endsWith('/directory_contact_info'))}) {
          if(!/^(Email|E-mail|Phone|Mobile|Messenger|WhatsApp|Website|Social links|电子邮箱|電子郵箱|邮箱|郵箱|电话|電話|網站|网站|Correo electrónico|Adresse e-mail|メール|メールアドレス|이메일|Электронная почта|E-posta|البريد الإلكتروني|אימייל|ईमेल|อีเมล)$/i.test(title)) continue;
          parts.push(text);
        }
        for(const anchor of section.querySelectorAll('a[href]')) {
          if(!visible(anchor)) continue;
          try {let url=new URL(anchor.href);if(url.origin==='https://www.facebook.com'&&url.pathname==='/l.php'){const dest=url.searchParams.get('u');if(dest)url=new URL(dest);}
            const messenger=url.href==='https://www.facebook.com/messages/t/'+owner.id+'/';
            if((messenger||['https:','http:'].includes(url.protocol)&&url.hostname!=='facebook.com'&&!url.hostname.endsWith('.facebook.com')&&!url.username&&!url.password&&url.href.length<=2048)&&!links.includes(url.href)&&links.length<10)links.push(url.href);
          }catch{}
        }
      }
      return {id:owner.id,source_url:${JSON.stringify(source)},observed_profile_url:${JSON.stringify(base)},title:owner.display_name,contact_text:[...new Set(parts)].join('\n'),contact_links:links};})()`));
    const data=raw&&'session' in Object(raw)&&'data' in Object(raw)?raw.data:raw;
    return projectFacebookNativeContact(data,args.id,base,source);
  });
}
