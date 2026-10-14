// Read only the observed profile's own sections. Never inspect the operator's
// session, send a message, open an editor or return surrounding recommendation data.
export function linkedinProfileTarget(handle) {
  if (typeof handle !== 'string' || !/^[A-Za-z0-9][A-Za-z0-9_-]{0,127}$/.test(handle)
      || ['me', 'self'].includes(handle.toLowerCase())) throw new Error('ONECREW_LINKEDIN_INPUT');
  return `https://www.linkedin.com/in/${handle}/`;
}

export function projectLinkedinProfile(data, handle) {
  const source = linkedinProfileTarget(handle);
  const invalid = () => { throw new Error('ONECREW_LINKEDIN_INVALID'); };
  const text = (value, maximum) => typeof value === 'string' && new TextEncoder().encode(value).length <= maximum && !value.includes('\0');
  if (data?.author_handle !== handle || data.profile_url !== source
      || typeof data.account_id !== 'string' || !/^ACo[A-Za-z0-9_-]{16,125}$/.test(data.account_id)
      || !text(data.author_label, 512) || !data.author_label.trim() || /[\u0000-\u001f\u007f]/.test(data.author_label)
      || !text(data.headline, 2000) || !text(data.bio, 8000) || !text(data.follower_text, 100)
      || !Array.isArray(data.public_links) || data.public_links.length > 10
      || !Array.isArray(data.contact_links) || data.contact_links.length > 1
      || data.contact_links.some(link => link !== `${source}overlay/contact-info/`)) invalid();
  for (const link of data.public_links) {
    if (typeof link !== 'string' || link.length > 2048 || /\s|[\u0000-\u001f\u007f]/.test(link)) invalid();
    try {
      const url = new URL(link);
      if (url.protocol !== 'https:' || url.username || url.password || url.port
          || !url.hostname.includes('.') || url.hostname.endsWith('.')
          || /(?:^|\.)(?:localhost|local|internal|test|invalid|onion|home\.arpa)$/.test(url.hostname)
          || /^\d+\.\d+\.\d+\.\d+$/.test(url.hostname) || url.hostname.startsWith('[')) invalid();
    } catch { invalid(); }
  }
  // Only exact, unabridged counts with an explicit observed label qualify.
  // Other UI locales and abbreviated counts remain unknown, never zero.
  const count = /^(0|[1-9]\d*|[1-9]\d{0,2}(?:,\d{3})+) followers?$/i.exec(data.follower_text.trim());
  const followers = count ? Number(count[1].replaceAll(',', '')) : null;
  if (followers !== null && !Number.isSafeInteger(followers)) invalid();
  return {adapter_version:'1.0.0', author_handle:handle, account_id:data.account_id,
    author_label:data.author_label.trim(), profile_url:source, follower_count:followers,
    follower_text:data.follower_text.trim(), headline:data.headline, bio:data.bio,
    public_links:[...new Set(data.public_links)], contact_links:data.contact_links,
    evidence_source:'visible_profile_header', bio_truncated:true};
}

// This function is serialized into one owned provider page. All filtering and
// byte bounds run there before any result crosses the browser bridge.
export function observeLinkedinProfile(source, handle, contactOverlay = false) {
  if (location.origin !== 'https://www.linkedin.com') throw new Error('ONECREW_LINKEDIN_INVALID');
  if (/^\/(?:checkpoint|challenge|security)/.test(location.pathname)) throw new Error('ONECREW_LINKEDIN_RISK');
  if (/^\/(?:login|uas\/login|authwall)/.test(location.pathname)) throw new Error('ONECREW_LINKEDIN_AUTH');
  const sameProfile = href => href === source || href === `${source}?isSelfProfile=false`;
  if (!sameProfile(location.href) && !(contactOverlay && location.href === `${source}overlay/contact-info/`)) throw new Error('ONECREW_LINKEDIN_INVALID');
  const visible = node => node.getClientRects().length && !node.closest('aside, [role="complementary"], nav, [role="navigation"], [aria-hidden="true"]');
  if ([...document.querySelectorAll('iframe[src*="captcha"], .captcha-container, #challenge-running')].some(visible)) throw new Error('ONECREW_LINKEDIN_RISK');
  const main = document.querySelector('main');
  if (!main) return null;
  const headings = [...main.querySelectorAll('h1,h2')].filter(node => {
    const anchor = node.closest('a[href]');
    if (!visible(node)) return false;
    if (anchor) return sameProfile(anchor.href);
    const section = node.closest('section');
    return section?.querySelectorAll('h1,h2,h3').length === 1
      && [...section.querySelectorAll('a[href]')].some(link => visible(link)
        && link.href === `${source}overlay/contact-info/`);
  });
  if (headings.length !== 1) return null;
  const name = headings[0];
  const header = name.closest('section');
  if (!header || header.querySelectorAll('h1,h2,h3').length !== 1) return null;
  const bounded = (value, maximum) => {
    const original = String(value || '').trim();
    let bytes = 0, output = '';
    for (const character of original) {
      const size = new TextEncoder().encode(character).length;
      if (bytes + size > maximum) break;
      output += character; bytes += size;
    }
    return output;
  };
  const ids = new Set();
  const links = [], contact = [];
  for (const anchor of header.querySelectorAll('a[href]')) {
    if (!visible(anchor)) continue;
    let url;
    try { url = new URL(anchor.href); } catch { continue; }
    if (url.origin === location.origin && url.pathname === '/messaging/compose/') {
      const urn = url.searchParams.get('profileUrn');
      const id = urn?.match(/^urn:li:fsd_profile:(ACo[A-Za-z0-9_-]{16,125})$/)?.[1];
      if (!id || url.searchParams.get('recipient') !== id) throw new Error('ONECREW_LINKEDIN_INVALID');
      ids.add(id);
    }
    if (anchor.href === `${source}overlay/contact-info/`) contact.push(anchor.href);
    if (url.origin === location.origin && url.pathname === '/safety/go/') {
      const target = url.searchParams.get('url');
      if (!target) continue;
      try { url = new URL(target); } catch { continue; }
    }
    if (url.protocol === 'https:' && url.origin !== location.origin && !url.username && !url.password
        && !url.port && url.href.length <= 2048 && links.length < 10) links.push(url.href);
  }
  if (ids.size > 1) throw new Error('ONECREW_LINKEDIN_INVALID');
  if (ids.size === 0) return null;
  const paragraphs = [...header.querySelectorAll('p')].filter(visible).map(node => node.textContent.trim());
  const count = paragraphs.find(text => /^(?:\d[\d,.\s]*|[\d.]+[KMB]) followers?$/i.test(text)) || '';
  // A stable section anchor is preferred. Translated headings are locators only;
  // the paragraph text remains original and is never synthesized or translated.
  const aboutLabels = new Set(['About','关于','關於','概要','소개','Acerca de','À propos','Info','Informazioni','Sobre','Over','Om','O mnie','O mně','Hakkında','О себе','Про себе','Tentang','Giới thiệu','نبذة']);
  const about = [...main.querySelectorAll('section')].find(section => {
    const hs = [...section.querySelectorAll('h2')];
    return hs.length === 1 && (section.querySelector('[id="about"]') || aboutLabels.has(hs[0].textContent.trim()));
  });
  const originalText = paragraph => {
    const walker = document.createTreeWalker(paragraph, NodeFilter.SHOW_TEXT);
    let text = '';
    while (walker.nextNode()) {
      const node = walker.currentNode;
      if (!node.parentElement?.closest('button, [role="button"], [aria-hidden="true"]')) text += node.textContent;
    }
    return text.trim();
  };
  const bio = about ? [...about.querySelectorAll('p')].filter(visible).map(originalText).filter(Boolean).join('\n') : '';
  const headline = paragraphs.find(text => text && text !== count && !/^·|^(?:She|He|They)\//.test(text)
    && !['Contact info','联系信息','聯絡資訊'].includes(text)) || '';
  return {author_handle:handle, account_id:[...ids][0], profile_url:source,
    author_label:bounded(name.textContent,512), headline:bounded(headline,2000), bio:bounded(bio,8000),
    follower_text:bounded(count,100), public_links:[...new Set(links)], contact_links:[...new Set(contact)], about_pending:!about};
}

export async function readLinkedinProfile(page, handle, ownedPage) {
  const source = linkedinProfileTarget(handle);
  return ownedPage(page, async owned => {
    await owned.goto(source, {settleMs:2000});
    let latest;
    for (let attempt = 0; attempt < 12; attempt++) {
      const raw = await owned.evaluate(`(${observeLinkedinProfile.toString()})(${JSON.stringify(source)},${JSON.stringify(handle)})`);
      const data = raw && 'session' in Object(raw) && 'data' in Object(raw) ? raw.data : raw;
      if (data) {
        latest = projectLinkedinProfile(data, handle);
        if (!data.about_pending) return latest;
      }
      if (attempt < 11) await owned.sleep(0.5);
    }
    if (latest) return latest;
    throw new Error('ONECREW_LINKEDIN_INVALID');
  });
}

export function projectLinkedinContent(data, handle, account) {
  const source = linkedinProfileTarget(handle);
  if (typeof account !== 'string' || !/^ACo[A-Za-z0-9_-]{16,125}$/.test(account)) throw new Error('ONECREW_LINKEDIN_INPUT');
  if (data?.account_id !== account || data.author_handle !== handle || data.profile_url !== source
      || typeof data.post_id !== 'string' || !/^[1-9]\d{0,19}$/.test(data.post_id)
      || data.source_url !== `https://www.linkedin.com/feed/update/urn:li:activity:${data.post_id}/`
      || typeof data.text !== 'string' || !data.text.trim() || data.text.includes('\0')
      || new TextEncoder().encode(data.text).length > 16000) throw new Error('ONECREW_LINKEDIN_INVALID');
  return {account_id:account, author_handle:handle, profile_url:source, post_id:data.post_id,
    source_url:data.source_url, text:data.text, text_truncated:true, evidence_source:'visible_author_activity'};
}

// Inspect at most the first five visible Activity cards. Reposts and ambiguous
// authors are not an author's own body. A missing body is not language evidence.
export function observeLinkedinContent(source, handle, account, readProfile) {
  const profile = readProfile(source,handle);
  if (!profile) return null;
  if (profile.account_id !== account) throw new Error('ONECREW_LINKEDIN_INVALID');
  const main=document.querySelector('main');
  const activityLabels=new Set(['Activity','动态','動態','アクティビティ','활동','Actividad','Activité','Aktivitäten','Attività','Atividades','Activiteiten','Aktivitet','Aktywność','Aktivita','Faaliyet','Действия','Діяльність','Aktivitas','Hoạt động','النشاط']);
  const activity=[...main.querySelectorAll('section')].find(section=>{
    const headings=[...section.querySelectorAll('h2')];
    return headings.length===1 && activityLabels.has(headings[0].textContent.trim());
  });
  if(!activity) return null;
  const cards=[...activity.querySelectorAll('[role="listitem"]')]
    .filter(node=>node.getClientRects().length).slice(0,5);
  for(const card of cards) {
    const anchors=[...card.querySelectorAll('a[href]')].filter(node=>node.getClientRects().length);
    const authorLinks=anchors.filter(node=>{
      try {const url=new URL(node.href);return url.origin===location.origin && url.pathname.startsWith('/in/');}
      catch{return false;}
    });
    if(!authorLinks.length || authorLinks.some(node=>node.href!==source && node.href!==`${source}?isSelfProfile=false`)) continue;
    if(anchors.some(node=>{try{return new URL(node.href).pathname.startsWith('/company/');}catch{return false;}})) continue;
    const posts=anchors.filter(node=>/^https:\/\/www\.linkedin\.com\/feed\/update\/urn:li:activity:[1-9]\d{0,19}\/$/.test(node.href));
    const urls=[...new Set(posts.map(node=>node.href))];
    if(urls.length!==1) continue;
    const bodySelector='p [data-testid="expandable-text-box"]';
    const bodies=posts.filter(node=>node.querySelectorAll(bodySelector).length);
    if(bodies.length!==1) continue;
    const texts=[];
    for(const paragraph of bodies[0].querySelectorAll(bodySelector)) {
      const walker=document.createTreeWalker(paragraph,NodeFilter.SHOW_TEXT);
      let text='';
      while(walker.nextNode()) {
        const node=walker.currentNode;
        if(!node.parentElement?.closest('button, [role="button"], [aria-hidden="true"]')) text+=node.textContent;
      }
      if(text.trim()) texts.push(text.trim());
    }
    let bytes=0,text='';
    for(const character of texts.join('\n')) {
      const size=new TextEncoder().encode(character).length;
      if(bytes+size>16000) break;
      text+=character;bytes+=size;
    }
    if(!text.trim()) continue;
    const post_id=urls[0].match(/urn:li:activity:([1-9]\d{0,19})\/$/)[1];
    return {account_id:account,author_handle:handle,profile_url:source,post_id,source_url:urls[0],text};
  }
  return null;
}

export async function readLinkedinContent(page,handle,account,ownedPage) {
  const source=linkedinProfileTarget(handle);
  if(typeof account!=='string'||!/^ACo[A-Za-z0-9_-]{16,125}$/.test(account)) throw new Error('ONECREW_LINKEDIN_INPUT');
  return ownedPage(page,async owned=>{
    await owned.goto(source,{settleMs:2000});
    for(let attempt=0;attempt<12;attempt++) {
      const raw=await owned.evaluate(`(${observeLinkedinContent.toString()})(${JSON.stringify(source)},${JSON.stringify(handle)},${JSON.stringify(account)},(${observeLinkedinProfile.toString()}))`);
      const data=raw && 'session' in Object(raw) && 'data' in Object(raw) ? raw.data : raw;
      if(data) return projectLinkedinContent(data,handle,account);
      if(attempt<11) await owned.sleep(0.5);
    }
    // No visible original post is missing evidence, not a parser failure.
    // Keep it source-local so three empty profiles cannot quarantine LinkedIn.
    throw new Error('ONECREW_LINKEDIN_CONTENT_UNAVAILABLE');
  });
}

export function projectLinkedinContact(data,handle,account) {
  const profile=linkedinProfileTarget(handle);
  if(typeof account!=='string'||!/^ACo[A-Za-z0-9_-]{16,125}$/.test(account)) throw new Error('ONECREW_LINKEDIN_INPUT');
  if(data?.account_id!==account || data.author_handle!==handle || data.source_url!==`${profile}overlay/contact-info/`
      || typeof data.title!=='string' || !data.title.trim() || new TextEncoder().encode(data.title).length>512 || [...data.title].length>200
      || /[\u0000-\u001f\u007f]/.test(data.title) || typeof data.contact_text!=='string' || data.contact_text.includes('\0')
      || new TextEncoder().encode(data.contact_text).length>8000 || !Array.isArray(data.contact_links)
      || data.contact_links.length>10) throw new Error('ONECREW_LINKEDIN_INVALID');
  // Reuse the strict public-link projection; no unobserved website is invented.
  const checked=projectLinkedinProfile({author_handle:handle,account_id:account,profile_url:profile,
    author_label:data.title,headline:'',bio:'',follower_text:'',public_links:data.contact_links,contact_links:[]},handle);
  return {adapter_version:'1.0.0',author_handle:handle,account_id:account,source_url:data.source_url,
    title:checked.author_label,contact_text:data.contact_text,contact_links:checked.public_links,
    truncated:true,evidence_source:'visible_contact_dialog'};
}

// The native dialog and background header must identify the same observed
// author. Extract email/TG and public web links only, not phone, birthday,
// premium controls, recommendations or the operator's account information.
export function observeLinkedinContact(source,handle,account,readProfile) {
  let profile;
  try {profile=readProfile(source,handle,true);}
  catch(error) {
    if(error?.message==='ONECREW_LINKEDIN_INVALID') throw new Error('ONECREW_LINKEDIN_INVALID_CONTACT_PROFILE_ROUTE');
    throw error;
  }
  if(!profile) return null;
  if(profile.account_id!==account) throw new Error('ONECREW_LINKEDIN_INVALID_CONTACT_OWNER');
  // The observed link opens asynchronously. Only the already validated same
  // profile can remain pending; any other route is rejected by readProfile.
  if(location.href!==`${source}overlay/contact-info/`) return null;
  const visible=node=>node.getClientRects().length && !node.closest('[aria-hidden="true"]');
  const dialogs=[...document.querySelectorAll('dialog[open], [role="dialog"][aria-modal="true"]')].filter(visible);
  if(dialogs.length!==1) return null;
  const dialog=dialogs[0];
  const header=dialog.querySelector('header');
  const scope=header?.parentElement;
  if(!scope || scope.querySelectorAll('h1,h2,h3').length!==1
      || scope.querySelector('[role="feed"], article, aside, [role="progressbar"], [aria-busy="true"]')) return null;
  const anchors=[...scope.querySelectorAll('a[href]')].filter(node=>visible(node)
    && !node.closest('button, [role="button"], nav, [role="navigation"], aside'));
  const authorLinks=anchors.filter(node=>{
    try {const url=new URL(node.href);return url.origin===location.origin && url.pathname.startsWith('/in/');}
    catch{return false;}
  });
  if(!authorLinks.length) return null;
  if(authorLinks.some(node=>node.href!==source)) throw new Error('ONECREW_LINKEDIN_INVALID_CONTACT_DIALOG_OWNER');
  const emails=[],links=[];
  let linkBytes=0;
  for(const anchor of anchors) {
    let url;
    try {url=new URL(anchor.href);} catch {continue;}
    if(url.protocol==='mailto:') {
      let email;
      try {email=decodeURIComponent(url.pathname);} catch {continue;}
      if(email.length<=254 && /^[A-Za-z0-9.!#$%&'*+\/=?^_`{|}~-]+@[A-Za-z0-9](?:[A-Za-z0-9.-]*[A-Za-z0-9])?\.[A-Za-z]{2,}$/.test(email)
          && !/\s/.test(email) && !emails.includes(email) && emails.length<10) emails.push(email);
      continue;
    }
    if(url.origin===location.origin && url.pathname==='/safety/go/') {
      const target=url.searchParams.get('url');
      if(!target) continue;
      try {url=new URL(target);} catch {continue;}
    }
    if(url.protocol!=='https:' || url.origin===location.origin || url.username || url.password || url.port
        || url.href.length>2048 || links.includes(url.href) || links.length>=10) continue;
    const size=new TextEncoder().encode(url.href).length;
    if(linkBytes+size>6000) continue;
    links.push(url.href);linkBytes+=size;
  }
  return {author_handle:handle,account_id:account,source_url:location.href,title:[...profile.author_label].slice(0,200).join(''),
    contact_text:emails.join('\n'),contact_links:links};
}

export async function readLinkedinContact(page,handle,account,ownedPage) {
  const source=linkedinProfileTarget(handle);
  if(typeof account!=='string'||!/^ACo[A-Za-z0-9_-]{16,125}$/.test(account)) throw new Error('ONECREW_LINKEDIN_INPUT');
  return ownedPage(page,async owned=>{
    await owned.goto(source,{settleMs:2000});
    let profile;
    for(let attempt=0;attempt<12;attempt++) {
      const raw=await owned.evaluate(`(${observeLinkedinProfile.toString()})(${JSON.stringify(source)},${JSON.stringify(handle)})`);
      const data=raw && 'session' in Object(raw) && 'data' in Object(raw) ? raw.data : raw;
      if(data) {profile=projectLinkedinProfile(data,handle);break;}
      if(attempt<11) await owned.sleep(0.5);
    }
    const contact=profile?.contact_links[0];
    if(profile?.account_id!==account || contact!==`${source}overlay/contact-info/`) throw new Error('ONECREW_LINKEDIN_INVALID_CONTACT_BEFORE_CLICK');
    // LinkedIn redirects a newly opened overlay URL back to the profile.
    // Follow only the exact contact locator observed in this author's header,
    // retaining the owned page and its native background identity.
    await owned.click(`main a[href="${contact}"]`);
    for(let attempt=0;attempt<12;attempt++) {
      const raw=await owned.evaluate(`(${observeLinkedinContact.toString()})(${JSON.stringify(source)},${JSON.stringify(handle)},${JSON.stringify(account)},(${observeLinkedinProfile.toString()}))`);
      const data=raw && 'session' in Object(raw) && 'data' in Object(raw) ? raw.data : raw;
      if(data) {
        try {return projectLinkedinContact(data,handle,account);}
        catch(error) {
          if(error?.message==='ONECREW_LINKEDIN_INVALID') throw new Error('ONECREW_LINKEDIN_INVALID_CONTACT_PROJECTION');
          throw error;
        }
      }
      if(attempt<11) await owned.sleep(0.5);
    }
    throw new Error('ONECREW_LINKEDIN_INVALID_CONTACT_UNAVAILABLE');
  });
}
