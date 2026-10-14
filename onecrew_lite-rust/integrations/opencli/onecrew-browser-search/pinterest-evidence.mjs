// Project a native UserResource response. Search summaries cannot certify identity.
const fail = (profileField) => {
  const stage=['OWNER','ID','FOLLOWERS','BIO','NAME','WEBSITE','WEBSITE_FORMAT','WEBSITE_PROTOCOL','WEBSITE_PRIVATE','WEBSITE_TYPE'].includes(profileField) ? `_PROFILE_${profileField}` : '';
  throw new Error(`ONECREW_PINTEREST_INVALID${stage}`);
};
function throwResourceStop(result) {
  if (!result?.stop) return;
  if (result.stop === 'auth' || result.stop === 'risk') throw new Error(`ONECREW_PINTEREST_${result.stop.toUpperCase()}`);
  const stage=['input','source','session','response_http','response_stream','response_size','response_encoding','resource'].includes(result.stage) ? `_${result.stage.toUpperCase()}` : '';
  throw new Error(`ONECREW_PINTEREST_READ${stage}`);
}
const bounded = (value, max) => {
  if (typeof value !== 'string' || value.includes('\0') || Buffer.byteLength(value) > max) return fail();
  return value;
};
const profileText=(value,max,stage)=>{try{return bounded(value,max);}catch{return fail(stage);}};
function isBarePublicHost(value) {
  const host=value.replace(/\/$/,'').toLowerCase();
  return host.length<=253 && /^(?:[a-z0-9](?:[a-z0-9-]{0,61}[a-z0-9])?\.)+[a-z]{2,63}$/.test(host)
    && !['localhost','local','internal','test','invalid','onion','home.arpa'].some(suffix=>host===suffix||host.endsWith(`.${suffix}`));
}
export function nativeId(value) {
  const id = typeof value === 'string' ? value : Number.isSafeInteger(value) ? String(value) : '';
  return /^[1-9][0-9]{0,19}$/.test(id) ? id : fail();
}
export function nativeUsername(value) {
  return typeof value === 'string' && /^[A-Za-z0-9_]{1,30}$/.test(value) ? value : fail();
}
export function projectProfile(user, expectedUsername) {
  nativeUsername(expectedUsername);
  if (!user || typeof user !== 'object' || Array.isArray(user)
      || user.username !== expectedUsername || (user.type !== undefined && user.type !== 'user')) return fail('OWNER');
  let authorId;try{authorId=nativeId(user.id);}catch{return fail('ID');}
  const followers = user.follower_count;
  if (followers !== undefined && followers !== null && (!Number.isSafeInteger(followers) || followers < 0)) return fail('FOLLOWERS');
  const bio = user.about === undefined || user.about === null ? '' : profileText(user.about, 8000,'BIO');
  const name = user.full_name === undefined || user.full_name === null ? '' : profileText(user.full_name, 512,'NAME');
  const publicLinks = [];
  if (typeof user.domain_url === 'string' && user.domain_url) {
    const raw = profileText(user.domain_url, 2048,'WEBSITE');
    let website;
    try { website = new URL(raw); } catch {
      // The provider can publish a bare domain. It is neither an actionable
      // URL nor a contradiction of native identity. Do not invent its scheme.
      if(!isBarePublicHost(raw))return fail('WEBSITE_FORMAT');
    }
    if(website && !['https:','http:'].includes(website.protocol))return fail('WEBSITE_PROTOCOL');
    if (website && (website.username || website.password || website.port
        || !website.hostname.includes('.') || /^[0-9.]+$/.test(website.hostname) || /^(localhost|\[)/i.test(website.hostname)
        || /\.(local|localhost|internal)$/i.test(website.hostname))) return fail('WEBSITE_PRIVATE');
    // Only HTTPS is eligible for the public-page reader. An unsupported public
    // HTTP link supplies no contact claim, but does not contradict native identity.
    if(website?.protocol==='https:')publicLinks.push(website.href);
  } else if (user.domain_url !== undefined && user.domain_url !== null && user.domain_url !== '') return fail('WEBSITE_TYPE');
  return [{author_id: authorId, username: expectedUsername, author_label: name || expectedUsername,
    profile_url: `https://www.pinterest.com/${expectedUsername}/`,
    follower_count: followers ?? null, bio, public_links: publicLinks}];
}

// Runs in the owned browser target. A resource get is read-only despite POST.
export async function fetchNativeResource(kind, target, searchLimit = 20) {
  const isProfile = kind === 'profile' && typeof target === 'string' && /^[A-Za-z0-9_]{1,30}$/.test(target);
  const isPins = kind === 'pins' && typeof target === 'string' && /^[A-Za-z0-9_]{1,30}$/.test(target);
  const isPin = kind === 'pin' && typeof target === 'string' && /^[1-9][0-9]{0,19}$/.test(target);
  const isSearch = ['search','search-pins'].includes(kind) && typeof target === 'string' && target.trim() && target.length <= 160 && !/[\x00-\x1f]/.test(target)
    && Number.isInteger(searchLimit) && searchLimit >= 1 && searchLimit <= 20;
  if (!isProfile && !isSearch && !isPins && !isPin) return {stop:'read',stage:'input'};
  const searchPath=kind==='search-pins'?'/search/pins/':'/search/users/';
  const source = isProfile || isPins ? `/${target}/` : isPin ? `/pin/${target}/` : `${searchPath}?q=${encodeURIComponent(target)}`;
  // Pinterest redirects a numeric Pin URL to a title slug ending in the same
  // native ID. The slug supplies no identity; PinResource and its projection
  // must still bind the reserved Pin and its actually observed pinner.
  const pagePin = isPin ? location.pathname.match(/^\/pin\/(?:[^/]+--)?([1-9][0-9]{0,19})\/$/) : null;
  const samePage = isPin ? pagePin?.[1] === target : location.pathname === (isSearch ? searchPath : source);
  if (location.origin !== 'https://www.pinterest.com') return {stop:'read',stage:'source'};
  const text = document.body?.innerText || '';
  if (/verify you are human|security check|captcha|安全验证|人机验证/i.test(text)) return {stop:'risk'};
  if (/^\/login\/?$/.test(location.pathname)) return {stop:'auth'};
  if (!samePage || (isSearch && new URLSearchParams(location.search).get('q') !== target)) return {stop:'read',stage:'source'};
  const csrf = (document.cookie.match(/(?:^|;\s*)csrftoken=([^;]+)/) || [])[1];
  if (!csrf) return {stop:'read',stage:'session'};
  const options = isProfile ? {username:target,field_set_key:'profile'} : isPins ? {username:target,field_set_key:'grid_item',page_size:5} : isPin ? {id:target,field_set_key:'detailed'} : {query:target,scope:'users',page_size:searchLimit};
  if(kind==='search-pins')Object.assign(options,{scope:'pins',appliedProductFilters:'---',auto_correction_disabled:true});
  const body = new URLSearchParams({source_url:source, data:JSON.stringify({options,context:{}})});
  const response = await fetch(isProfile ? '/resource/UserResource/get/' : isPins ? '/resource/UserPinsResource/get/' : isPin ? '/resource/PinResource/get/' : '/resource/BaseSearchResource/get/', {method:'POST', credentials:'include', redirect:'error',
    headers:{'Content-Type':'application/x-www-form-urlencoded','X-CSRFToken':csrf,'X-Pinterest-PWS-Handler':'www/[username]/[slug].js'},
    body:body.toString(), signal:AbortSignal.timeout(15000)});
  if (response.status === 401) return {stop:'auth'};
  if (response.status === 403 || response.status === 429) return {stop:'risk'};
  if (!response.ok) return {stop:'read',stage:'response_http'};
  if (!response.body) return {stop:'read',stage:'response_stream'};
  // A search page includes nested public user metadata. Profile/Pin bounds stay
  // unchanged; search remains one page with at most twenty requested items.
  const maximumBytes=isSearch ? 1000000 : 200000;
  const reader = response.body.getReader(); const chunks = []; let size = 0;
  while (true) {
    const {done,value} = await reader.read(); if (done) break;
    size += value.byteLength;
    if (size > maximumBytes) { await reader.cancel(); return {stop:'read',stage:'response_size'}; }
    chunks.push(value);
  }
  const bytes = new Uint8Array(size); let offset = 0;
  for (const chunk of chunks) {bytes.set(chunk,offset); offset += chunk.length;}
  let raw;try {raw=JSON.parse(new TextDecoder('utf-8',{fatal:true}).decode(bytes));} catch {return {stop:'read',stage:'response_encoding'};}
  if (raw?.resource_response?.error) return {stop:'read',stage:'resource'};
  return raw?.resource_response?.data;
}

export async function readProfile(page, username, ownPage) {
  nativeUsername(username);
  return ownPage(page, async owned => {
    await owned.goto(`https://www.pinterest.com/${username}/`);
    const raw = await owned.evaluate(`(${fetchNativeResource.toString()})("profile",${JSON.stringify(username)})`);
    const result = raw && 'session' in Object(raw) && 'data' in Object(raw) ? raw.data : raw;
    throwResourceStop(result);
    return projectProfile(result, username);
  });
}

export function projectSearch(payload, limit) {
  if (!Number.isInteger(limit) || limit < 1 || limit > 20) return fail();
  const results = Array.isArray(payload) ? payload : payload?.results;
  if (!Array.isArray(results) || results.length > 100) return fail();
  const rows = [];
  for (const user of results.slice(0,20)) {
    if (user?.type !== 'user') continue;
    const username = nativeUsername(user.username), id = nativeId(user.id);
    const label = user.full_name === undefined || user.full_name === null ? username : bounded(user.full_name,512) || username;
    const source = `https://www.pinterest.com/${username}/`;
    rows.push({author_id:id,username,author_label:label,author_url:source,source_url:source,title:label});
    if (rows.length === limit) break;
  }
  return rows;
}

export async function readSearch(page, query, limit, ownPage) {
  return readSearchKind(page,query,limit,ownPage,'search');
}

export async function readPinSearch(page, query, limit, ownPage) {
  return readSearchKind(page,query,limit,ownPage,'search-pins');
}

async function readSearchKind(page, query, limit, ownPage, kind) {
  if (typeof query !== 'string' || !query.trim() || query.length > 160 || /[\x00-\x1f]/.test(query)
      || !Number.isInteger(limit) || limit < 1 || limit > 20) return fail();
  return ownPage(page,async owned => {
    const path=kind==='search-pins'?'pins':'users';
    await owned.goto(`https://www.pinterest.com/search/${path}/?q=${encodeURIComponent(query)}`);
    const raw = await owned.evaluate(`(${fetchNativeResource.toString()})(${JSON.stringify(kind)},${JSON.stringify(query)},${limit})`);
    const result = raw && 'session' in Object(raw) && 'data' in Object(raw) ? raw.data : raw;
    throwResourceStop(result);
    return kind==='search-pins'?projectPinSearch(result,limit):projectSearch(result,limit);
  });
}

// A Pin is the search evidence. Its pinner is a native account observation,
// not proof that the account created the image or passes creator qualification.
export function projectPinSearch(payload, limit) {
  if(!Number.isInteger(limit)||limit<1||limit>20)return fail();
  const results=Array.isArray(payload)?payload:payload?.results;
  if(!Array.isArray(results)||results.length>100)return fail();
  const rows=[],seen=new Map();
  for(const pin of results.slice(0,20)) {
    if(pin?.type!=='pin'||pin.is_promoted===true||pin.promoted_is_ad===true||pin.is_repin===true)continue;
    const pinId=nativeId(pin.id),authorId=nativeId(pin.pinner?.id),username=nativeUsername(pin.pinner?.username);
    const binding=`${authorId}/${username}`;
    if(seen.has(pinId)) {if(seen.get(pinId)!==binding)return fail();continue;}
    seen.set(pinId,binding);
    const title=bounded(pin.title??pin.grid_title??'',512);
    const description=bounded(pin.description??'',15000);
    // Reserve space for caption evidence even when the title is keyword-stuffed.
    // Keep the original order and identity; this is not a relevance filter.
    const heading=Array.from(title).slice(0,description.trim()?200:500).join('');
    const preview=[heading,description].filter(Boolean).join('\n').trim();
    // An image-only Pin supplies no text evidence. Keep other valid results
    // from this same bounded page; do not turn absence into a parser failure.
    if(!preview)continue;
    const label=bounded(pin.pinner.full_name??username,512)||username;
    rows.push({provider:'onecrew-pinterest-pin-search-v1',pin_id:pinId,author_id:authorId,username,author_label:label,
      author_url:`https://www.pinterest.com/${username}/`,source_url:`https://www.pinterest.com/pin/${pinId}/`,
      title:Array.from(preview).slice(0,500).join('')});
    if(rows.length===limit)break;
  }
  return rows;
}


// Serialized projections leave provider/bootstrap objects in the owned browser.
export function projectPinTarget(payload, authorId, username) {
  if (typeof authorId !== 'string' || typeof username !== 'string'
      || !/^[1-9][0-9]{0,19}$/.test(authorId) || !/^[A-Za-z0-9_]{1,30}$/.test(username)) throw new Error('ONECREW_PINTEREST_INVALID');
  const pins = Array.isArray(payload) ? payload : payload?.pins;
  if (!Array.isArray(pins) || pins.length > 100) throw new Error('ONECREW_PINTEREST_INVALID');
  const skipped = {type:0, author:0, repin:0, ad:0};
  for (const pin of pins.slice(0,5)) {
    // A saved third-party pin, display-name match or promoted card is not evidence
    // that this account published the caption. Do not follow its external URL.
    if (pin?.type !== 'pin') { skipped.type++; continue; }
    if (pin?.pinner?.id !== authorId || pin.pinner.username !== username) { skipped.author++; continue; }
    if (pin.is_repin === true) { skipped.repin++; continue; }
    if (pin.promoted_is_ad === true) { skipped.ad++; continue; }
    if (typeof pin.id !== 'string' || !/^[1-9][0-9]{0,19}$/.test(pin.id)) throw new Error('ONECREW_PINTEREST_INVALID');
    return {pin_id:pin.id,author_id:authorId,author_username:username,
      source_url:`https://www.pinterest.com/pin/${pin.id}/`,profile_url:`https://www.pinterest.com/${username}/`};
  }
  // Fixed labels and counts only: no provider text, IDs, cookies or raw payload.
  // Each inspected item has one reason; never inspect a sixth item to fill gaps.
  throw new Error(`ONECREW_PINTEREST_READ_CAPTION_UNAVAILABLE inspected=${Math.min(pins.length,5)} type=${skipped.type} author=${skipped.author} repin=${skipped.repin} ad=${skipped.ad}`);
}
export function projectPinContent(pin, pinId, authorId, username) {
  const invalid=()=>{throw new Error('ONECREW_PINTEREST_INVALID');};
  if (typeof pinId !== 'string' || !/^[1-9][0-9]{0,19}$/.test(pinId)
      || typeof authorId !== 'string' || !/^[1-9][0-9]{0,19}$/.test(authorId)
      || typeof username !== 'string' || !/^[A-Za-z0-9_]{1,30}$/.test(username)
      || pin?.id !== pinId || pin.type !== 'pin' || pin?.pinner?.id !== authorId
      || pin.pinner.username !== username || pin.is_repin === true || pin.promoted_is_ad === true) invalid();
  const title=pin.title ?? pin.grid_title ?? '', description=pin.description ?? '';
  const bounded=(value,max)=>typeof value==='string' && !value.includes('\0') && new TextEncoder().encode(value).length<=max;
  if (!bounded(title,512) || !bounded(description,15000) || !(title+'\n'+description).trim()) invalid();
  return {pin_id:pinId,author_id:authorId,author_username:username,
    profile_url:`https://www.pinterest.com/${username}/`,source_url:`https://www.pinterest.com/pin/${pinId}/`,
    text:title+'\n'+description,text_truncated:true};
}
export async function readContent(page, authorId, username, ownPage, pinId) {
  if (typeof authorId !== 'string') return fail();
  nativeId(authorId); nativeUsername(username);
  if(pinId!==undefined&&(typeof pinId!=='string'||nativeId(pinId)!==pinId))return fail();
  return ownPage(page, async owned => {
    const unwrap=value=>value && 'session' in Object(value) && 'data' in Object(value) ? value.data : value;
    let target;
    if(pinId!==undefined) {
      target={pin_id:pinId,author_id:authorId,author_username:username,
        source_url:`https://www.pinterest.com/pin/${pinId}/`,profile_url:`https://www.pinterest.com/${username}/`};
    } else {
      await owned.goto(`https://www.pinterest.com/${username}/`);
      const targetScript=`(async()=>{const data=await (${fetchNativeResource.toString()})("pins",${JSON.stringify(username)});if(data?.stop)return data;return (${projectPinTarget.toString()})(data,${JSON.stringify(authorId)},${JSON.stringify(username)});})()`;
      target=unwrap(await owned.evaluate(targetScript));
    }
    throwResourceStop(target);
    if (target?.author_id!==authorId || target?.author_username!==username || !/^[1-9][0-9]{0,19}$/.test(target?.pin_id ?? '')
        || target.source_url!==`https://www.pinterest.com/pin/${target.pin_id}/`
        || target.profile_url!==`https://www.pinterest.com/${username}/`) return fail();
    await owned.goto(target.source_url);
    const script=`(async()=>{const data=await (${fetchNativeResource.toString()})("pin",${JSON.stringify(target.pin_id)});if(data?.stop)return data;return (${projectPinContent.toString()})(data,${JSON.stringify(target.pin_id)},${JSON.stringify(authorId)},${JSON.stringify(username)});})()`;
    const content=unwrap(await owned.evaluate(script));
    throwResourceStop(content);
    // Check the projection again outside the page before returning a receipt.
    if(content?.pin_id!==target.pin_id || content?.author_id!==authorId || content?.author_username!==username
        || content?.source_url!==target.source_url || content?.profile_url!==target.profile_url) return fail();
    return [content];
  });
}
