import { equivalentSourceUrl, sourceUrlScript } from './source-url.mjs';

export function facebookProfileTarget(id) {
  if (typeof id !== 'string' || !/^[1-9][0-9]{0,19}$/.test(id)) throw new Error('ONECREW_FB_PROFILE_INPUT');
  return `https://www.facebook.com/profile.php?id=${id}`;
}

export function projectFacebookProfile(data, id) {
  const source = facebookProfileTarget(id);
  const invalid = () => {throw new Error('ONECREW_FB_PROFILE_INVALID');};
  if (data?.id !== id || data.profile_url !== source || typeof data.display_name !== 'string'
    || !data.display_name.trim() || data.display_name.includes('\0') || Array.from(data.display_name).length > 200
    || typeof data.follower_text !== 'string' || data.follower_text.includes('\0') || data.follower_text.length > 100) invalid();
  const contactLinks=data.contact_links ?? [];
  const contactUrl=`${source}&sk=about_contact_and_basic_info`;
  if(!Array.isArray(contactLinks)||contactLinks.length>2||contactLinks.some(link=>![contactUrl,`${source}&sk=directory_contact_info`,`${source}&sk=directory_links`].includes(link))) invalid();
  // An abbreviated count is not an exact observation. Preserve the text and
  // unknown count rather than inventing precision or treating absence as zero.
  const exact = /^(0|[1-9][0-9]*|[1-9][0-9]{0,2}(?:,[0-9]{3})+) followers?$/i.exec(data.follower_text.trim());
  const count = exact ? Number(exact[1].replaceAll(',', '')) : null;
  if (count !== null && !Number.isSafeInteger(count)) invalid();
  return {adapter_version:'1.0.0',id,display_name:data.display_name.trim(),
    follower_count:count,follower_text:data.follower_text.trim(),profile_url:source,
    evidence_source:'visible_numeric_profile_header',...(contactLinks.length?{contact_links:contactLinks}:{}),
    ...(typeof data.biography==='string'?{biography:data.biography,bio_truncated:data.bio_truncated,bio_status:data.bio_status}:{})};
}

export async function readFacebookProfileHeader(source, id) {
  const profileSource=`https://www.facebook.com/profile.php?id=${id}`;
  for (let attempt = 0; attempt < 16; attempt++) {
    if (!equivalentSourceUrl(location.origin,'https://www.facebook.com')) throw new Error('ONECREW_FB_PROFILE_INVALID');
    if (/^\/(?:login|recover)(?:\/|\.php|$)/.test(location.pathname)) throw new Error('ONECREW_FB_PROFILE_AUTH');
    if (/^\/(?:checkpoint|challenge|two_step_verification)(?:\/|$)/.test(location.pathname)) throw new Error('ONECREW_FB_PROFILE_RISK');
    if (!equivalentSourceUrl(location.href,source)) throw new Error('ONECREW_FB_PROFILE_INVALID');
    const mains = [...document.querySelectorAll('[role="main"]')].filter(n => n.getClientRects().length);
    if (mains.length > 1) throw new Error('ONECREW_FB_PROFILE_INVALID');
    if (mains.length === 1) {
      const links = [...mains[0].querySelectorAll('a[href]')].filter(a => a.getClientRects().length
        && a.getAttribute('role') !== 'tab' && equivalentSourceUrl(a.href,`${profileSource}&sk=followers`) && /^[0-9][0-9,.]*\s*[KMB]? followers?$/i.test(a.innerText.trim()));
      if (links.length > 1) throw new Error('ONECREW_FB_PROFILE_INVALID');
      if (links.length === 1) {
        let header = links[0];
        for (let depth = 0; header && depth < 5; depth++, header = header.parentElement) {
          const names = [...header.querySelectorAll('h1,[role="button"]')].filter(n => n.getClientRects().length && n.innerText.trim());
          if (names.length > 1) throw new Error('ONECREW_FB_PROFILE_INVALID');
          if (names.length === 1) {
            // Only an exact, observed contact tab belongs to this profile.
            // Do not treat links or mailboxes from the timeline as author contacts.
            const contactUrl=`${source}&sk=about_contact_and_basic_info`;
            const observed=[...mains[0].querySelectorAll('a[href]')].some(a=>a.getClientRects().length && a.getAttribute('role')==='tab' && equivalentSourceUrl(a.href,contactUrl));
            return {id,profile_url:profileSource,display_name:names[0].innerText.trim(),follower_text:links[0].innerText.trim(),...(observed?{contact_links:[contactUrl]}:{})};
          }
        }
      }
    }
    if (attempt < 15) await new Promise(resolve => setTimeout(resolve, 500));
  }
  throw new Error('ONECREW_FB_PROFILE_TIMEOUT');
}

export async function readFacebookProfile(page, args, ownedPage) {
  const source = facebookProfileTarget(args.id);
  return ownedPage(page, async owned => {
    await owned.goto(source, {settleMs:3000});
    const raw = await owned.evaluate(sourceUrlScript(`(${readFacebookProfileHeader.toString()})(${JSON.stringify(source)},${JSON.stringify(args.id)})`));
    const data = raw && 'session' in Object(raw) && 'data' in Object(raw) ? raw.data : raw;
    const aboutRaw=await owned.evaluate(sourceUrlScript(`(()=>{const links=[...document.querySelectorAll('[role="main"] a[role="tab"][href]')].filter(a=>a.getClientRects().length&&equivalentSourceUrl(a.href,${JSON.stringify(`${source}&sk=about`)}));return links.length===1?links[0].href:null;})()`));
    const about=aboutRaw&&'session' in Object(aboutRaw)&&'data' in Object(aboutRaw)?aboutRaw.data:aboutRaw;
    if(about!==null && !equivalentSourceUrl(about,`${source}&sk=about`)) throw new Error('ONECREW_FB_PROFILE_INVALID');
    if (about) {
      await owned.click(`a[role="tab"][href="${about}"]`);
      const detailsRaw=await owned.evaluate(sourceUrlScript(`(${observeFacebookAbout.toString()})(${JSON.stringify(about)},${JSON.stringify(source)})`));
      const details=detailsRaw&&'session' in Object(detailsRaw)&&'data' in Object(detailsRaw)?detailsRaw.data:detailsRaw;
      return projectFacebookProfile({...data,...details},args.id);
    }
    return projectFacebookProfile(data, args.id);
  });
}

export async function observeFacebookAbout(expected, profile) {
  for(let attempt=0;attempt<12;attempt++) {
    if(!equivalentSourceUrl(location.href,expected)) throw new Error('ONECREW_FB_PROFILE_INVALID');
    const visible=n=>n.getClientRects().length&&!n.closest('[role="article"],[role="feed"],[aria-hidden="true"]');
    const main=[...document.querySelectorAll('[role="main"]')].find(visible);
    const tabs=main?[...main.querySelectorAll('a[role="tab"][href]')].filter(visible):[];
    const targets=profile.includes('/profile.php?')?[`${profile}&sk=directory_contact_info`,`${profile}&sk=directory_links`,`${profile}&sk=about_contact_and_basic_info`]:[`${profile}/directory_contact_info`,`${profile}/directory_links`];
    const contact_links=targets.filter(target=>tabs.some(a=>equivalentSourceUrl(a.href,target)));
    const sections=main?[...main.querySelectorAll('section')].filter(n=>visible(n)&&!n.querySelector('section')):[];
    if(sections.length && ![...main.querySelectorAll('[role="progressbar"],[aria-busy="true"]')].some(visible)) {
      const bio=sections.filter(section=>{const h=section.querySelector('h1,h2,h3,[role="heading"]');return h&&/^(?:Bio|Intro|Description|简介|簡介|介紹|介绍)$/i.test(h.innerText.trim());});
      const original=bio.map(section=>section.innerText.split('\n').slice(1).join('\n').trim()).filter(Boolean).join('\n');
      const biography=[...original].slice(0,2000).join('');
      const truncated=biography!==original||bio.some(section=>[...section.querySelectorAll('[role="button"],button')].some(node=>visible(node)&&/see more|show more|展开|展開/i.test(node.innerText)));
      return {biography,bio_truncated:truncated,bio_status:bio.length?truncated?'truncated':'complete':'missing',contact_links:[...new Set(contact_links)]};
    }
    if(attempt<11) await new Promise(resolve=>setTimeout(resolve,400));
  }
  throw new Error('ONECREW_FB_PROFILE_TIMEOUT');
}

export function facebookContactTarget(id, source) {
  const profile=facebookProfileTarget(id),target=source??`${profile}&sk=about_contact_and_basic_info`;
  if(!['about_contact_and_basic_info','directory_contact_info','directory_links'].some(tab=>target===`${profile}&sk=${tab}`)) throw new Error('ONECREW_FB_PROFILE_INPUT');
  return target;
}

export function projectFacebookContact(data,id,requested) {
  const source=facebookContactTarget(id,requested);
  const invalid=()=>{throw new Error('ONECREW_FB_CONTACT_INVALID');};
  if(data?.id!==id||data.source_url!==source||typeof data.title!=='string'||data.title.length>200
    ||!data.title.trim()||/[\u0000-\u001f\u007f]/.test(data.title)
    ||typeof data.contact_text!=='string'||data.contact_text.length>12000||data.contact_text.includes('\0')
    ||!Array.isArray(data.contact_links)||data.contact_links.length>10||typeof data.truncated!=='boolean') invalid();
  for(const link of data.contact_links) {
    if(typeof link!=='string'||link.length>2048||/[\u0000-\u001f\u007f]/.test(link)) invalid();
    try {const url=new URL(link);if(!['https:','http:'].includes(url.protocol)||url.username||url.password) invalid();}
    catch {invalid();}
  }
  return {adapter_version:'1.0.0',id,source_url:source,title:data.title,contact_text:data.contact_text,contact_links:data.contact_links,truncated:data.truncated};
}

export async function readFacebookContactPanel(source,id,ownerName) {
  for(let attempt=0;attempt<12;attempt++) {
    if(!equivalentSourceUrl(location.origin,'https://www.facebook.com')) throw new Error('ONECREW_FB_CONTACT_INVALID');
    if(/^\/(?:login|recover)(?:\/|\.php|$)/.test(location.pathname)) throw new Error('ONECREW_FB_PROFILE_AUTH');
    if(/^\/(?:checkpoint|challenge|two_step_verification)(?:\/|$)/.test(location.pathname)) throw new Error('ONECREW_FB_PROFILE_RISK');
    if(!equivalentSourceUrl(location.href,source)) throw new Error('ONECREW_FB_CONTACT_INVALID');
    const mains=[...document.querySelectorAll('[role="main"]')].filter(node=>node.getClientRects().length);
    if(mains.length>1) throw new Error('ONECREW_FB_CONTACT_INVALID');
    if(mains.length===1) {
      const main=mains[0];
      const excluded='[role="article"],[role="feed"],[role="navigation"],nav,script,style,[aria-hidden="true"]';
      const visible=node=>node.getClientRects().length && !node.closest(excluded);
      const headings=[...main.querySelectorAll('h1')].filter(visible);
      if(headings.length>1) throw new Error('ONECREW_FB_CONTACT_INVALID');
      const selected=[...main.querySelectorAll('a[role="tab"][aria-selected="true"]')].filter(anchor=>visible(anchor)&&equivalentSourceUrl(anchor.href,source));
      const loading=[...main.querySelectorAll('[aria-busy="true"],[role="progressbar"]')].some(visible);
      if((headings.length===1||ownerName) && selected.length===1 && !loading) {
        const scope=selected[0].closest('[role="tablist"]')?.parentElement;
        if(!scope||scope.querySelector('[role="feed"],[role="article"]')) throw new Error('ONECREW_FB_CONTACT_INVALID');
        const parts=[];const links=[];let size=0,truncated=false;
        const walk=document.createTreeWalker(scope,NodeFilter.SHOW_TEXT);
        while(walk.nextNode()) {
          const node=walk.currentNode;
          if(!node.parentElement||!visible(node.parentElement)) continue;
          const text=node.textContent.trim();if(!text) continue;
          if(size+text.length+1>12000){truncated=true;break;}
          parts.push(text);size+=text.length+1;
        }
        for(const anchor of scope.querySelectorAll('a[href]')) {
          if(!visible(anchor)) continue;
          try {
            let url=new URL(anchor.href);
            // Expand only the destination encoded in an observed Facebook redirect.
            if(url.origin==='https://www.facebook.com'&&url.pathname==='/l.php') {
              const destination=url.searchParams.get('u');
              if(destination) url=new URL(destination);
            }
            if(url.protocol==='mailto:') {
              const email=decodeURIComponent(url.pathname);
              if(email.length<=320&&!/[\u0000-\u001f\u007f]/.test(email)&&size+email.length+1<=12000){parts.push(email);size+=email.length+1;}
            } else if(url.href===`https://www.facebook.com/messages/t/${id}/`) {
              if(!links.includes(url.href)&&links.length<10) links.push(url.href);
            } else if(['https:','http:'].includes(url.protocol)&&url.hostname!=='facebook.com'&&!url.hostname.endsWith('.facebook.com')&&!url.username&&!url.password&&url.href.length<=2048) {
              if(!links.includes(url.href)&&links.length<10) links.push(url.href);
            }
          } catch {}
        }
        return {id,source_url:source,title:(ownerName||headings[0].textContent.trim()).slice(0,200),contact_text:parts.join('\n'),contact_links:links,truncated};
      }
    }
    if(attempt<11) await new Promise(resolve=>setTimeout(resolve,500));
  }
  throw new Error('ONECREW_FB_PROFILE_TIMEOUT');
}

export async function readFacebookContact(page,args,ownedPage) {
  const source=facebookContactTarget(args.id,args.source);
  return ownedPage(page,async owned=>{
    await owned.goto(source,{settleMs:3000});
    const raw=await owned.evaluate(sourceUrlScript(`(async()=>{const owner=await (${readFacebookProfileHeader.toString()})(${JSON.stringify(source)},${JSON.stringify(args.id)});return (${readFacebookContactPanel.toString()})(${JSON.stringify(source)},${JSON.stringify(args.id)},owner.display_name);})()`));
    const data=raw && 'session' in Object(raw) && 'data' in Object(raw)?raw.data:raw;
    return projectFacebookContact(data,args.id,source);
  });
}
