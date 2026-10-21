import {observeInstagramVisibleProfile,observeInstagramVisiblePosts} from './instagram-visible.mjs';
import {equivalentSourceUrl,sourceUrlScript} from './source-url.mjs';

export function instagramProfileTarget(username) {
  if (typeof username !== 'string' || !/^[A-Za-z0-9_.]{1,30}$/.test(username)) throw new Error('ONECREW_PROFILE_INPUT');
  return username.toLowerCase();
}

export function projectInstagramProfile(user, username) {
  const invalid=()=>{throw new Error('ONECREW_INSTAGRAM_PROFILE_INVALID');};
  const id=value=>typeof value==='string' && /^[1-9][0-9]{0,24}$/.test(value) ? value
    : Number.isSafeInteger(value) && value>0 ? String(value) : null;
  if (!user || typeof user.username!=='string' || user.username.toLowerCase()!==username) invalid();
  const nativeId=id(user.id) ?? id(user.pk);
  if (!nativeId || (user.id!=null && user.pk!=null && id(user.id)!==id(user.pk))) invalid();
  const count=(first,second)=>{
    const value=first ?? second;
    if (value==null) return null;
    if (!Number.isSafeInteger(value)||value<0) invalid();
    return value;
  };
  const text=(value,max)=>{
    if(value==null) return '';
    if(typeof value!=='string'||value.includes('\0')) invalid();
    return Array.from(value).slice(0,max).join('');
  };
  if(typeof user.is_private!=='boolean'||typeof user.is_verified!=='boolean') invalid();
  // Keep public contact fields separate from the author's original biography.
  const contactLinks=[];
  for(const value of [user.external_url,...(Array.isArray(user.bio_links)?user.bio_links.map(link=>link?.url):[])]) {
    if(typeof value!=='string'||value.length>2048||/[\u0000-\u001f\u007f]/.test(value)) continue;
    try {
      const url=new URL(value);
      if(!['https:','http:'].includes(url.protocol)||url.username||url.password) continue;
      if(!contactLinks.includes(url.href)) contactLinks.push(url.href);
      if(contactLinks.length===10) break;
    } catch {}
  }
  const email=typeof user.public_email==='string'?user.public_email.trim():'';
  const publicEmail=email.length<=320 && /^[^\s@<>]+@[^\s@<>]+\.[^\s@<>]+$/.test(email)?email:null;
  return {adapter_version:'1.0.0',id:nativeId,username,
    public_email:publicEmail,contact_links:contactLinks,
    display_name:text(user.full_name,200),biography:text(user.biography,2000),
    follower_count:count(user.edge_followed_by?.count,user.follower_count),
    private_account:user.is_private,verified:user.is_verified,
    profile_url:`https://www.instagram.com/${username}/`,evidence_source:'native_profile_response'};
}

// Reuse the upstream read endpoints, but preserve native identity and unknown
// counts. Never return session/bootstrap payloads or default absent counts to 0.
export async function fetchInstagramProfile(username) {
  if(location.origin!=='https://www.instagram.com') return {stop:'source'};
  if(/^\/accounts\/login/.test(location.pathname)) return {stop:'auth'};
  if(/^\/(challenge|checkpoint)\//.test(location.pathname)) return {stop:'risk'};
  const read=async path=>{
    let response;
    try {
      response=await fetch(path,{credentials:'include',headers:{'X-IG-App-ID':'936619743392459'},signal:AbortSignal.timeout(8000)});
    } catch(error) {
      return {stop:error?.name==='TimeoutError'?'timeout':'read'};
    }
    const url=new URL(response.url);
    if(url.origin!==location.origin) return {stop:'source'};
    if(/^\/accounts\/login/.test(url.pathname)||response.status===401) return {stop:'auth'};
    if(response.status===403||response.status===429||/^\/(challenge|checkpoint)\//.test(url.pathname)) return {stop:'risk',status:response.status};
    if(url.pathname!==new URL(path,location.origin).pathname) return {stop:'source'};
    if(!response.ok) return {stop:'http',status:response.status};
    const data=await response.json();
    if(data?.status==='fail') return {stop:data.message==='login_required'?'auth':'read'};
    return {data};
  };
  const web=await read('/api/v1/users/web_profile_info/?username='+encodeURIComponent(username));
  if(!web.stop) return {user:web.data?.data?.user};
  if(web.status!==400) return web;
  // Some professional accounts explicitly reject the first endpoint with 400.
  const feed=await read('/api/v1/feed/user/'+encodeURIComponent(username)+'/username/?count=1');
  if(feed.stop) return feed;
  const owner=feed.data?.user;
  const raw=owner?.pk;
  const pk=typeof raw==='string'?raw:Number.isSafeInteger(raw)?String(raw):'';
  if(!/^[1-9][0-9]{0,24}$/.test(pk)||owner?.username?.toLowerCase()!==username) return {stop:'identity'};
  const info=await read('/api/v1/users/'+pk+'/info/');
  if(info.stop) return info;
  const user=info.data?.user;
  if(String(user?.pk ?? user?.id)!==pk) return {stop:'identity'};
  return {user};
}

export async function readInstagramProfile(page,args,ownPage) {
  const username=instagramProfileTarget(args.username);
  return ownPage(page,async owned=>{
    await owned.goto(`https://www.instagram.com/${username}/`,{settleMs:3000});
    const response=await owned.evaluate(`(async()=>{const result=await (${fetchInstagramProfile.toString()})(${JSON.stringify(username)});return result.stop?{stop:result.stop,status:result.status}:{profile:(${projectInstagramProfile.toString()})(result.user,${JSON.stringify(username)})};})()`);
    const data=response && 'session' in Object(response) && 'data' in Object(response)?response.data:response;
    if(data?.stop) {
      const error=new Error('ONECREW_PROFILE_'+data.stop.toUpperCase());
      if(Number.isInteger(data.status)) error.httpStatus=data.status;
      throw error;
    }
    if(!data?.profile) throw new Error('ONECREW_INSTAGRAM_PROFILE_INVALID');
    return data.profile;
  });
}

// Public DOM only. Account ID comes from the saved search binding, not this page.
export function observeInstagramProfile(username, authorId) {
  if (!equivalentSourceUrl(location.origin,'https://www.instagram.com')) return {stop:'source'};
  if (/^\/accounts\/login/.test(location.pathname)) return {stop:'auth'};
  if (/^\/(challenge|checkpoint)\//.test(location.pathname)) return {stop:'risk'};
  if (!equivalentSourceUrl(location.href,`https://www.instagram.com/${username}/`)) return {stop:'identity'};
  const main = document.querySelector('main');
  const lines = (main?.innerText || '').split('\n').map(s=>s.trim()).filter(Boolean);
  if (lines[0]?.toLowerCase() !== username) return {stop:'identity'};
  const exactCount = label => {
    const line = lines.find(s=>new RegExp('^[0-9,]+ '+label+'$').test(s));
    if (!line) return null;
    const n = Number(line.split(' ')[0].replaceAll(',',''));
    return Number.isSafeInteger(n) ? n : null;
  };
  // Instagram renders large counts as K/M but exposes the exact value in the
  // title inside this author's followers link. Never read following/post totals.
  const followerCount = () => {
    const values = new Set();
    const parse = value => {
      if (typeof value !== 'string' || !/^(?:[0-9]+|[0-9]{1,3}(?:[,\u00a0\u202f ][0-9]{3})+)$/.test(value.trim())) return;
      const n = Number(value.replace(/[,\u00a0\u202f ]/g, ''));
      if (Number.isSafeInteger(n) && n >= 0) values.add(n);
    };
    for (const a of main?.querySelectorAll('a[href]') || []) {
      const href = new URL(a.getAttribute('href'), location.origin);
      if (href.origin !== location.origin || href.pathname.toLowerCase() !== `/${username}/followers/`) continue;
      parse(a.getAttribute('title'));
      for (const node of a.querySelectorAll('[title]')) parse(node.getAttribute('title'));
    }
    for (const node of main?.querySelectorAll('[title]') || []) {
      const label = node.parentElement?.textContent?.trim();
      if (!label || !/^[0-9.,KMB\s]+ followers$/i.test(label)) continue;
      parse(node.getAttribute('title'));
    }
    if (values.size > 1) return null;
    return values.size === 1 ? [...values][0] : exactCount('followers');
  };
  const following = lines.findIndex(s=>/^[0-9.,KMB]+ following$/i.test(s));
  const privateIndex = lines.findIndex(s=>/^This (?:profile|account) is private[.]?$/i.test(s));
  const isPrivate = privateIndex >= 0;
  const biography = following < 0 ? '' : lines.slice(following+1, isPrivate ? privateIndex : undefined)
    .join('\n').split(/\n?(?:Followed by |Follow\n|Following\n|Message\n)/)[0].slice(0,2000);
  const previews = [], seen = new Set();
  for (const a of isPrivate ? [] : main.querySelectorAll('a[href]')) {
    const url = new URL(a.getAttribute('href'),location.origin);
    if (url.origin !== location.origin || !new RegExp(`^/${username.replaceAll('.','\\.')}/(?:p|reel)/[A-Za-z0-9_-]+/$`).test(url.pathname)
      || seen.has(url.pathname)) continue;
    seen.add(url.pathname);
    const text = a.querySelector('img')?.getAttribute('alt')?.trim();
    if (text) previews.push(`${url.href}\n${text.slice(0,350)}`);
    if (previews.length === 6) break;
  }
  const name = lines[1] || '';
  const displayName = /^[0-9.,KMB\s]+ (?:posts?|followers|following)$/i.test(name) ? username : name;
  return {profile:{adapter_version:'1.0.0',id:authorId,username,
    display_name:displayName.slice(0,200),biography,
    follower_count:isPrivate ? exactCount('followers') : followerCount(),private_account:isPrivate ? true : null,verified:null,
    profile_url:`https://www.instagram.com/${username}/`,
    evidence_source:'public_profile_dom_search_bound',
    web_text:isPrivate ? '' : ('Public profile preview; image alt text, not verified original captions.\n'+previews.join('\n\n')).slice(0,4000)}};
}

export async function readInstagramWebProfile(page,args,ownPage) {
  const username=instagramProfileTarget(args.username);
  if (!/^[1-9][0-9]{0,24}$/.test(args.author_id || '')&&args.author_id!==`https://www.instagram.com/${username}/`) throw new Error('ONECREW_PROFILE_INPUT');
  return ownPage(page,async owned=>{
    await owned.goto(`https://www.instagram.com/${username}/`,{settleMs:3000});
    await owned.snapshot({source:'dom',interactive:true,compact:true});
    const rawTarget=await owned.evaluate(sourceUrlScript(`(${instagramVisibleExpansion.toString()})(${JSON.stringify(username)})`));
    const expansion=rawTarget&&'session' in Object(rawTarget)&&'data' in Object(rawTarget)?rawTarget.data:rawTarget;
    if(expansion?.stop) throw new Error('ONECREW_PROFILE_'+expansion.stop.toUpperCase());
    if(expansion?.selector) {
      await owned.click(expansion.selector);
      await owned.snapshot({source:'dom',interactive:true,compact:true});
    }
    const raw=await owned.evaluate(sourceUrlScript(`(${observeInstagramVisibleProfile.toString()})(${JSON.stringify(username)},${JSON.stringify(args.author_id)})`));
    const data=raw && 'session' in Object(raw) && 'data' in Object(raw)?raw.data:raw;
    if (!data?.profile) throw new Error('ONECREW_PROFILE_'+(data?.stop || 'read').toUpperCase());
    if (data.profile.private_account !== true) {
      const observed = await owned.evaluate(sourceUrlScript(`(${observeInstagramVisiblePosts.toString()})(${JSON.stringify(username)})`));
      const posts = observed && 'session' in Object(observed) && 'data' in Object(observed) ? observed.data : observed;
      if (posts?.stop) throw new Error('ONECREW_PROFILE_'+posts.stop.toUpperCase());
      if (!Array.isArray(posts?.posts)) throw new Error('ONECREW_INSTAGRAM_PROFILE_INVALID');
      data.profile.visible_posts = posts.posts;
      const preview='Visible profile post previews; bounded excerpts, not verified full original captions.\n'+posts.posts.map(post=>`${post.url}\n${post.preview}`).join('\n\n');
      let bytes=0;const characters=[];
      for(const character of preview) {
        const size=new TextEncoder().encode(character).length;
        if(bytes+size>3900) break;
        characters.push(character);bytes+=size;
      }
      data.profile.web_text=posts.posts.length?characters.join(''):'';
    }
    return data.profile;
  });
}

export function instagramVisibleExpansion(username) {
  if(!equivalentSourceUrl(location.href,`https://www.instagram.com/${username}/`)) return {stop:'identity'};
  const main=document.querySelector('main');
  if(!main) return {stop:'read'};
  const buttons=[...main.querySelectorAll('button,[role="button"]')].filter(node=>node.getClientRects().length&&/^(更多|more)$/i.test(node.innerText.trim()));
  if(buttons.length>1) return {stop:'read'};
  if(!buttons.length) return {selector:null};
  let node=buttons[0];const path=[];
  while(node!==main) {
    const parent=node.parentElement;
    if(!parent||path.length>=40) return {stop:'read'};
    const siblings=[...parent.children].filter(child=>child.tagName===node.tagName);
    path.unshift(`${node.tagName.toLowerCase()}:nth-of-type(${siblings.indexOf(node)+1})`);
    node=parent;
  }
  return {selector:`main > ${path.join(' > ')}`};
}
