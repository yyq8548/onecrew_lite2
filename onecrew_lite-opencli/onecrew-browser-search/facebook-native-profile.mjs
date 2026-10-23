import {facebookVanitySource,facebookHeaderNodes} from './facebook-identity.mjs';
import {facebookProfileTarget,observeFacebookAbout} from './facebook-profile.mjs';
import {equivalentSourceUrl,sourceUrlScript} from './source-url.mjs';
import {readSearchStop} from './owned-page.mjs';

export function facebookNativeContactTarget(base,source) {
  const profile=facebookVanitySource(base);
  if(![`${profile}/directory_contact_info`,`${profile}/directory_links`].includes(source)) throw new Error('ONECREW_FB_CONTACT_INVALID_FIELD_REQUESTED_SOURCE');
  return source;
}

// One public header subtree, the exact saved URL, and a visible owner relationship
// must agree. Never use a feed actor or the signed-in viewer as the profile ID.
export async function readFacebookNativeHeader(expected,id,base,collect) {
  for(let attempt=0;attempt<12;attempt++) {
    if(!equivalentSourceUrl(location.origin,'https://www.facebook.com')) throw new Error('ONECREW_FB_PROFILE_INVALID_FIELD_ORIGIN');
    if(/^\/(?:login|recover)(?:\/|\.php|$)/.test(location.pathname)) throw new Error('ONECREW_FB_PROFILE_AUTH');
    if(/^\/(?:checkpoint|challenge|two_step_verification)(?:\/|$)/.test(location.pathname)) throw new Error('ONECREW_FB_PROFILE_RISK');
    if(!equivalentSourceUrl(location.href,expected)) throw new Error('ONECREW_FB_PROFILE_INVALID_FIELD_ROUTE');
    const mains=[...document.querySelectorAll('[role="main"]')].filter(n=>n.getClientRects().length);
    if(mains.length>1) throw new Error('ONECREW_FB_PROFILE_INVALID_FIELD_MAIN');
    const scripts=[...document.querySelectorAll('script[type="application/json"][data-sjs]')];
    if(scripts.length>512) throw new Error('ONECREW_FB_PROFILE_INVALID_FIELD_HEADER_COUNT');
    let size=0;const owners=[];
    for(const script of scripts) {
      const text=script.textContent||'';size+=text.length;
      if(size>16000000) throw new Error('ONECREW_FB_PROFILE_INVALID_FIELD_HEADER_SIZE');
      if(text.length>3000000) continue;
      let value;try{value=JSON.parse(text);}catch{continue;}
      for(const row of collect(value)) if(typeof row.profile_url==='string' && row.profile_url.replace(/\/$/,'').toLowerCase()===base.toLowerCase()) owners.push(row);
    }
    const ids=new Set(owners.map(n=>n.id));
    if(ids.size>1 || (ids.size===1 && !ids.has(id))) throw new Error('ONECREW_FB_PROFILE_OWNER_CONFLICT field=id');
    if(mains.length===1 && owners.length) {
      const owner=owners[0],main=mains[0];
      const visible=n=>n.getClientRects().length && !n.closest('[role="article"],[role="feed"],[aria-hidden="true"]');
      const followers=[...main.querySelectorAll('a[href]')].filter(n=>visible(n)&&n.getAttribute('role')!=='tab'&&n.href.replace(/\/$/,'').toLowerCase()===`${base}/followers`.toLowerCase()&&/^[0-9][0-9,.]*\s*[KMB]? followers?$/i.test(n.innerText.trim()));
      const nativeName=owner.display_name.replace(/\s+/g,' ').trim();
      const names=[...main.querySelectorAll('h1,[role="button"]')].filter(n=>{
        if(!visible(n))return false;
        const label=n.innerText.replace(/\s+/g,' ').trim();
        return label===nativeName||label.startsWith(nativeName)&&/^[（(].+[)）]$/.test(label.slice(nativeName.length).trim());
      });
      const name=names.find(n=>n.tagName==='H1')??names[0];
      if(name) {
        const header={id,display_name:name.innerText.replace(/\s+/g,' ').trim(),profile_url:base};
        if(followers.length) {
          if(followers.length>1) {
            const heading=name.getBoundingClientRect();
            const distance=link=>{const rect=link.getBoundingClientRect();return Math.hypot(rect.left-heading.left,rect.top-heading.top);};
            followers.sort((a,b)=>distance(a)-distance(b));
          }
          return {...header,follower_text:followers[0].innerText.trim()};
        }
        const loading=[...main.querySelectorAll('[aria-busy="true"],[role="progressbar"]')].some(visible);
        if(document.readyState==='complete'&&!loading)return {...header,follower_text:''};
      }
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
    await owned.goto(source,{settleMs:0});
    const raw=await owned.evaluate(sourceUrlScript(String.raw`(async()=>{const owner=await (${readFacebookNativeHeader.toString()})(${JSON.stringify(source)},${JSON.stringify(args.id)},${JSON.stringify(base)},${facebookHeaderNodes.toString()});
      if(!owner.follower_text)return {...owner,profile_url:${JSON.stringify(canonical)},observed_profile_url:${JSON.stringify(base)},contact_links:[]};
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
  const invalid=field=>{throw new Error(`ONECREW_FB_CONTACT_INVALID_FIELD_${field}`);};
  if(!data||typeof data!=='object'||Array.isArray(data)) invalid('RESPONSE');
  if(typeof data.id!=='string'||!/^[1-9][0-9]{0,19}$/.test(data.id)) invalid('ID');
  if(data.id!==id) throw new Error('ONECREW_FB_CONTACT_OWNER_CONFLICT field=id');
  if(data.observed_profile_url!==facebookVanitySource(base)) throw new Error('ONECREW_FB_CONTACT_OWNER_CONFLICT field=observed_profile_url');
  if(data.source_url!==source) invalid('SOURCE_URL');
  if(typeof data.title!=='string'||!data.title.trim()||Array.from(data.title).length>200||/[\u0000-\u001f\u007f]/.test(data.title)) invalid('TITLE');
  if(typeof data.contact_text!=='string'||data.contact_text.length>12000||data.contact_text.includes('\0')) invalid('CONTACT_TEXT');
  if(!Array.isArray(data.contact_links)||data.contact_links.length>10) invalid('CONTACT_LINKS');
  for(const link of data.contact_links) {
    if(typeof link!=='string'||!URL.canParse(link)) invalid('CONTACT_LINK_URL');
    const url=new URL(link);
    if(!['http:','https:'].includes(url.protocol)||url.username||url.password||link.length>2048||/[\u0000-\u001f\u007f]/.test(link)) invalid('CONTACT_LINK_URL');
  }
  return {adapter_version:'2.0.0',id,source_url:source,observed_profile_url:facebookVanitySource(base),title:data.title,contact_text:data.contact_text,contact_links:data.contact_links,truncated:true};
}

export async function observeFacebookNativeContact(source,id,base,collect,readHeader,readStop) {
  const routes=[`${base}/directory_contact_info`,`${base}/directory_links`,`${base}/about`,`${base}/about_contact_and_basic_info`];
  for(let attempt=0;attempt<12;attempt++) {
    if(/^\/(?:login|recover)(?:\/|\.php|$)/.test(location.pathname)) throw new Error('ONECREW_FB_PROFILE_AUTH');
    if(/^\/(?:checkpoint|challenge|two_step_verification)(?:\/|$)/.test(location.pathname)) throw new Error('ONECREW_FB_PROFILE_RISK');
    const stop=readStop();
    if(stop==='auth') throw new Error('ONECREW_FB_PROFILE_AUTH');
    if(stop==='risk') throw new Error('ONECREW_FB_PROFILE_RISK');
    if(!routes.some(route=>equivalentSourceUrl(location.href,route))) throw new Error('ONECREW_FB_CONTACT_INVALID_FIELD_ROUTE');
    const owner=await readHeader(location.href,id,base,collect);
    const mains=[...document.querySelectorAll('[role="main"]')].filter(node=>node.getClientRects().length);
    if(mains.length!==1) throw new Error('ONECREW_FB_CONTACT_INVALID_FIELD_MAIN');
    const main=mains[0],excluded='[role="article"],[role="feed"],[role="navigation"],[role="complementary"],nav,script,style,[aria-hidden="true"]';
    const visible=node=>node.getClientRects().length&&!node.closest(excluded)&&getComputedStyle(node).visibility!=='hidden';
    if([...main.querySelectorAll('[aria-busy="true"],[role="progressbar"]')].some(visible)) {
      if(attempt===11) throw new Error('ONECREW_FB_CONTACT_INVALID_FIELD_LOADING');
      await new Promise(resolve=>setTimeout(resolve,500));continue;
    }
    const label=value=>String(value||'').replace(/\s+/g,' ').replace(/[:：]\s*$/,'').trim();
    const field=/^(Email|E-mail|Phone|Mobile|Messenger|WhatsApp|Website|Social links|电子邮箱|電子郵箱|邮箱|郵箱|电话|電話|網站|网站|Correo electrónico|Adresse e-mail|メール|メールアドレス|이메일|Электронная почта|E-posta|البريد الإلكتروني|אימייל|ईमेल|อีเมล)$/i;
    const group=/^(About|Contact|Contact info(?:rmation)?|Basic info(?:rmation)?|Contact and basic info|Links|Websites?(?: and social links)?|Social links|简介|簡介|关于|關於|联系(?:方式|信息|資訊)?|聯絡(?:方式|資訊)?|基本(?:信息|資料|资料)|网站和社交链接|網站與社交連結|网站与社交链接)$/i;
    const unrelated=/^(Reviews|Ratings|Recommendations|Related pages|People also (?:like|follow)|评论|評論|评价|評價|推荐|推薦|相关主页|相關專頁)$/i;
    const regions=[...main.querySelectorAll('section,[role="region"],[role="tabpanel"],dl,li,[role="listitem"],tr,[role="row"]')].filter(visible);
    const headings=node=>[...node.querySelectorAll('h1,h2,h3,h4,[role="heading"],dt')].filter(visible);
    const blocked=regions.filter(region=>{const heading=headings(region)[0];return heading&&unrelated.test(label(heading.innerText));});
    const eligible=node=>visible(node)&&!blocked.some(region=>region.contains(node));
    const scopes=new Set();
    for(const region of regions) {
      const heading=headings(region).find(eligible);
      if(eligible(region)&&heading&&(field.test(label(heading.innerText))||group.test(label(heading.innerText)))) scopes.add(region);
    }
    for(const node of main.querySelectorAll('h1,h2,h3,h4,[role="heading"],dt,span')) {
      if(!eligible(node)||!field.test(label(node.innerText))) continue;
      for(let parent=node.parentElement,depth=0;parent&&parent!==main&&depth<4;parent=parent.parentElement,depth++) {
        if(!eligible(parent)||headings(parent).some(heading=>!field.test(label(heading.innerText))&&!group.test(label(heading.innerText)))) break;
        if(label(parent.innerText)!==label(node.innerText)){scopes.add(parent);break;}
      }
    }
    for(const anchor of main.querySelectorAll('a[href]')) {
      if(eligible(anchor)&&/^(?:mailto|tel):/i.test(anchor.href)) scopes.add(anchor);
    }
    const parts=[],links=[];let size=0;
    const addText=value=>{const text=value.trim();if(text&&!parts.includes(text)&&size+text.length+1<=12000){parts.push(text);size+=text.length+1;}};
    for(const scope of scopes) {
      const walk=document.createTreeWalker(scope,NodeFilter.SHOW_TEXT);
      while(walk.nextNode()) if(walk.currentNode.parentElement&&eligible(walk.currentNode.parentElement)) addText(walk.currentNode.textContent);
      const anchors=scope.matches('a[href]')?[scope]:scope.querySelectorAll('a[href]');
      for(const anchor of anchors) {
        if(!eligible(anchor)||!URL.canParse(anchor.href)) continue;
        let url=new URL(anchor.href);
        if(url.hostname==='www.facebook.com'&&url.pathname==='/l.php') {
          const destination=url.searchParams.get('u');
          if(!destination||!URL.canParse(destination)) continue;
          url=new URL(destination);
        }
        if(['mailto:','tel:'].includes(url.protocol)) {
          const value=new URLSearchParams('value='+url.pathname.replaceAll('+','%2B')).get('value');
          if(value&&value.length<=320&&!/[\u0000-\u001f\u007f\ufffd]/.test(value)) addText(value);
        } else {
          const messenger=url.href===`https://www.facebook.com/messages/t/${owner.id}/`;
          if((messenger||['http:','https:'].includes(url.protocol)&&url.hostname!=='facebook.com'&&!url.hostname.endsWith('.facebook.com'))&&!url.username&&!url.password&&url.href.length<=2048&&!links.includes(url.href)&&links.length<10) links.push(url.href);
        }
      }
    }
    return {id:owner.id,source_url:source,observed_profile_url:base,title:owner.display_name,contact_text:parts.join('\n'),contact_links:links};
  }
}

export async function readFacebookNativeContact(page,args,ownPage) {
  const base=facebookVanitySource(args.url),source=facebookNativeContactTarget(base,args.source);facebookProfileTarget(args.id);
  return ownPage(page,async owned=>{
    await owned.goto(source,{settleMs:0});
    const raw=await owned.evaluate(sourceUrlScript(`(${observeFacebookNativeContact.toString()})(${JSON.stringify(source)},${JSON.stringify(args.id)},${JSON.stringify(base)},${facebookHeaderNodes.toString()},${readFacebookNativeHeader.toString()},${readSearchStop.toString()})`));
    const data=raw&&'session' in Object(raw)&&'data' in Object(raw)?raw.data:raw;
    return projectFacebookNativeContact(data,args.id,base,source);
  });
}
