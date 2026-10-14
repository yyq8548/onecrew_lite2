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
  if(!Array.isArray(contactLinks)||contactLinks.length>1||contactLinks.some(link=>link!==contactUrl)) invalid();
  // An abbreviated count is not an exact observation. Preserve the text and
  // unknown count rather than inventing precision or treating absence as zero.
  const exact = /^(0|[1-9][0-9]*|[1-9][0-9]{0,2}(?:,[0-9]{3})+) followers?$/i.exec(data.follower_text.trim());
  const count = exact ? Number(exact[1].replaceAll(',', '')) : null;
  if (count !== null && !Number.isSafeInteger(count)) invalid();
  return {adapter_version:'1.0.0',id,display_name:data.display_name.trim(),
    follower_count:count,follower_text:data.follower_text.trim(),profile_url:source,
    evidence_source:'visible_numeric_profile_header',...(contactLinks.length?{contact_links:contactLinks}:{})};
}

export async function readFacebookProfileHeader(source, id) {
  for (let attempt = 0; attempt < 16; attempt++) {
    if (location.origin !== 'https://www.facebook.com') throw new Error('ONECREW_FB_PROFILE_INVALID');
    if (/^\/(?:login|recover)(?:\/|\.php|$)/.test(location.pathname)) throw new Error('ONECREW_FB_PROFILE_AUTH');
    if (/^\/(?:checkpoint|challenge|two_step_verification)(?:\/|$)/.test(location.pathname)) throw new Error('ONECREW_FB_PROFILE_RISK');
    if (location.href !== source) throw new Error('ONECREW_FB_PROFILE_INVALID');
    const mains = [...document.querySelectorAll('[role="main"]')].filter(n => n.getClientRects().length);
    if (mains.length > 1) throw new Error('ONECREW_FB_PROFILE_INVALID');
    if (mains.length === 1) {
      const links = [...mains[0].querySelectorAll('a[href]')].filter(a => a.getClientRects().length
        && a.getAttribute('role') !== 'tab' && a.href === `${source}&sk=followers`);
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
            const observed=[...mains[0].querySelectorAll('a[href]')].some(a=>a.getClientRects().length && a.getAttribute('role')==='tab' && a.href===contactUrl);
            return {id,profile_url:source,display_name:names[0].innerText.trim(),follower_text:links[0].innerText.trim(),...(observed?{contact_links:[contactUrl]}:{})};
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
    const raw = await owned.evaluate(`(${readFacebookProfileHeader.toString()})(${JSON.stringify(source)},${JSON.stringify(args.id)})`);
    const data = raw && 'session' in Object(raw) && 'data' in Object(raw) ? raw.data : raw;
    return projectFacebookProfile(data, args.id);
  });
}

export function facebookContactTarget(id) {
  return `${facebookProfileTarget(id)}&sk=about_contact_and_basic_info`;
}

export function projectFacebookContact(data,id) {
  const source=facebookContactTarget(id);
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

export async function readFacebookContactPanel(source,id) {
  for(let attempt=0;attempt<12;attempt++) {
    if(location.origin!=='https://www.facebook.com') throw new Error('ONECREW_FB_CONTACT_INVALID');
    if(/^\/(?:login|recover)(?:\/|\.php|$)/.test(location.pathname)) throw new Error('ONECREW_FB_PROFILE_AUTH');
    if(/^\/(?:checkpoint|challenge|two_step_verification)(?:\/|$)/.test(location.pathname)) throw new Error('ONECREW_FB_PROFILE_RISK');
    if(location.href!==source) throw new Error('ONECREW_FB_CONTACT_INVALID');
    const mains=[...document.querySelectorAll('[role="main"]')].filter(node=>node.getClientRects().length);
    if(mains.length>1) throw new Error('ONECREW_FB_CONTACT_INVALID');
    if(mains.length===1) {
      const main=mains[0];
      const excluded='[role="article"],[role="feed"],[role="navigation"],nav,script,style,[aria-hidden="true"]';
      const visible=node=>node.getClientRects().length && !node.closest(excluded);
      const headings=[...main.querySelectorAll('h1')].filter(visible);
      if(headings.length>1) throw new Error('ONECREW_FB_CONTACT_INVALID');
      const selected=[...main.querySelectorAll('a[role="tab"][aria-selected="true"]')].filter(anchor=>visible(anchor)&&anchor.href===source);
      const loading=[...main.querySelectorAll('[aria-busy="true"],[role="progressbar"]')].some(visible);
      if(headings.length===1 && headings[0].textContent.trim() && selected.length===1 && !loading) {
        const parts=[];const links=[];let size=0;
        const walk=document.createTreeWalker(main,NodeFilter.SHOW_TEXT);
        while(walk.nextNode()) {
          const node=walk.currentNode;
          if(!node.parentElement||!visible(node.parentElement)) continue;
          const text=node.textContent.trim();if(!text) continue;
          if(size+text.length+1>12000){break;}
          parts.push(text);size+=text.length+1;
        }
        for(const anchor of main.querySelectorAll('a[href]')) {
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
            } else if(['https:','http:'].includes(url.protocol)&&url.hostname!=='facebook.com'&&!url.hostname.endsWith('.facebook.com')&&!url.username&&!url.password&&url.href.length<=2048) {
              if(!links.includes(url.href)&&links.length<10) links.push(url.href);
            }
          } catch {}
        }
        // Selected About is an author-bound partial observation, never proof that
        // all asynchronous contact fields have arrived or that no email exists.
        return {id,source_url:source,title:headings[0].textContent.trim().slice(0,200),contact_text:parts.join('\n'),contact_links:links,truncated:true};
      }
    }
    if(attempt<11) await new Promise(resolve=>setTimeout(resolve,500));
  }
  throw new Error('ONECREW_FB_PROFILE_TIMEOUT');
}

export async function readFacebookContact(page,args,ownedPage) {
  const source=facebookContactTarget(args.id);
  return ownedPage(page,async owned=>{
    await owned.goto(source,{settleMs:3000});
    const raw=await owned.evaluate(`(${readFacebookContactPanel.toString()})(${JSON.stringify(source)},${JSON.stringify(args.id)})`);
    const data=raw && 'session' in Object(raw) && 'data' in Object(raw)?raw.data:raw;
    return projectFacebookContact(data,args.id);
  });
}
