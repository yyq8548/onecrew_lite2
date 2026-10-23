import {linkedinProfileTarget, observeLinkedinProfile, projectLinkedinProfile} from './linkedin-evidence.mjs';
import {sourceUrlScript} from './source-url.mjs';

export function linkedinActivityId(source) {
  const id=typeof source==='string' && source.match(/^https:\/\/www\.linkedin\.com\/feed\/update\/urn:li:activity:([1-9]\d{0,19})\/$/)?.[1];
  if(!id) throw new Error('ONECREW_LINKEDIN_INPUT');
  return id;
}

export function projectLinkedinComments(data,handle,account,source) {
  linkedinProfileTarget(handle);
  const post=linkedinActivityId(source);
  if(typeof account!=='string'||!/^ACo[A-Za-z0-9_-]{16,125}$/.test(account)) throw new Error('ONECREW_LINKEDIN_INPUT');
  if(data?.author_handle!==handle || data.account_id!==account || data.source_url!==source
      || !Array.isArray(data.comments) || data.comments.length>5) throw new Error('ONECREW_LINKEDIN_INVALID');
  const seen=new Set();
  const comments=data.comments.map(row=>{
    if(typeof row?.id!=='string'||! /^[1-9]\d{0,19}$/.test(row.id) || row.parent_post_id!==post
        || seen.has(row.id) || typeof row.text!=='string' || row.text.includes('\0')
        || new TextEncoder().encode(row.text).length>2000) throw new Error('ONECREW_LINKEDIN_INVALID');
    seen.add(row.id);
    return {id:row.id,parent_post_id:post,text:row.text};
  });
  return {author_handle:handle,account_id:account,source_url:source,comments,evidence_source:'visible_same_post_comments'};
}

// Inspect only the same native Activity page. Comment IDs include their actual
// parent Activity; a repost's original-thread comments cannot bind to a repost.
export function observeLinkedinComments(source,profile,handle,account) {
  if(location.origin!=='https://www.linkedin.com') throw new Error('ONECREW_LINKEDIN_INVALID');
  if(/^\/(?:checkpoint|challenge|security)/.test(location.pathname)) throw new Error('ONECREW_LINKEDIN_RISK');
  if(/^\/(?:login|uas\/login|authwall)/.test(location.pathname)) throw new Error('ONECREW_LINKEDIN_AUTH');
  if(location.href!==source) throw new Error('ONECREW_LINKEDIN_INVALID');
  const visible=node=>node.getClientRects().length && !node.closest('aside, nav, [role="complementary"], [aria-hidden="true"]');
  if([...document.querySelectorAll('iframe[src*="captcha"], .captcha-container, #challenge-running')].some(visible)) throw new Error('ONECREW_LINKEDIN_RISK');
  const main=document.querySelector('main');
  if(!main) return null;
  const cards=[...main.querySelectorAll('[role="listitem"]')].filter(card=>visible(card)
    && [...card.children].some(child=>child.querySelectorAll('h2').length===1
      && [...child.querySelectorAll('a[href]')].some(anchor=>visible(anchor)&&anchor.href===source)));
  if(cards.length!==1) return null;
  const card=cards[0];
  const primary=[...card.children].filter(child=>child.querySelectorAll('h2').length===1);
  if(primary.length!==1 || primary[0].querySelector('[contenteditable="true"], textarea, input')) return null;
  const anchors=[...primary[0].querySelectorAll('a[href]')].filter(visible);
  const authors=anchors.filter(anchor=>{try{return new URL(anchor.href).origin===location.origin && new URL(anchor.href).pathname.startsWith('/in/');}catch{return false;}});
  if(!authors.length || authors.some(anchor=>anchor.href!==profile && anchor.href!==`${profile}?isSelfProfile=false`)
      || anchors.some(anchor=>{try{return new URL(anchor.href).pathname.startsWith('/company/');}catch{return false;}})) throw new Error('ONECREW_LINKEDIN_INVALID');
  const permalinks=anchors.filter(anchor=>/^https:\/\/www\.linkedin\.com\/feed\/update\/urn:li:activity:[1-9]\d{0,19}\/$/.test(anchor.href));
  if(!permalinks.length || permalinks.some(anchor=>anchor.href!==source)) throw new Error('ONECREW_LINKEDIN_INVALID');
  const keySelector='[componentkey^="CommentComponentReference_urn:li:comment:("]';
  let scope=card;
  // The observed detail layout keeps comments beside the post card, four
  // ancestors above it. Stop before another post, heading or recommendation.
  for(let depth=0;depth<5;depth++) {
    if(scope.querySelectorAll(keySelector).length) break;
    const parent=scope.parentElement;
    if(!parent || parent===main || parent.querySelectorAll('h2').length!==1
        || [...parent.querySelectorAll('[role="listitem"]')].filter(node=>node.querySelector('h2')).length!==1) break;
    scope=parent;
  }
  const rows=[...scope.querySelectorAll(keySelector)].filter(visible).slice(0,5);
  const post=source.match(/urn:li:activity:([1-9]\d{0,19})\/$/)[1];
  const comments=[];
  for(const row of rows) {
    const identity=row.getAttribute('componentkey')?.match(/^CommentComponentReference_urn:li:comment:\(activity:([1-9]\d{0,19}),([1-9]\d{0,19})\)$/);
    if(!identity || identity[1]!==post) throw new Error('ONECREW_LINKEDIN_INVALID');
    const columns=[...row.children];
    const parts=columns.length===2?[...columns[1].children]:[];
    if(parts.length!==3) return null;
    const body=parts[1];
    if(body.querySelector('[contenteditable="true"], textarea, input')) return null;
    if([...body.querySelectorAll('button,[role="button"]')].some(button=>visible(button)
      && /^(Show original|See original|View original|显示原文|查看原文|顯示原文|查看原始内容)$/i.test(button.textContent.trim()))) throw new Error('ONECREW_ORIGINAL_TEXT_UNCONFIRMED');
    const paragraphs=[...body.querySelectorAll('p')].filter(visible);
    if(paragraphs.length!==1) return null;
    const walker=document.createTreeWalker(paragraphs[0],NodeFilter.SHOW_TEXT);
    let original='';
    while(walker.nextNode()) {
      const node=walker.currentNode;
      if(!node.parentElement?.closest('button, [role="button"], [aria-hidden="true"], [contenteditable="true"]')) original+=node.textContent;
    }
    let text='',bytes=0;
    for(const character of original.trim()) {
      const size=new TextEncoder().encode(character).length;
      if(bytes+size>2000) break;
      text+=character;bytes+=size;
    }
    comments.push({id:identity[2],parent_post_id:post,text});
  }
  return {author_handle:handle,account_id:account,source_url:source,comments};
}

export async function readLinkedinComments(page,handle,account,source,ownedPage) {
  const profileUrl=linkedinProfileTarget(handle);
  linkedinActivityId(source);
  if(typeof account!=='string'||!/^ACo[A-Za-z0-9_-]{16,125}$/.test(account)) throw new Error('ONECREW_LINKEDIN_INPUT');
  return ownedPage(page,async owned=>{
    await owned.goto(profileUrl,{settleMs:2000});
    let profile;
    for(let attempt=0;attempt<12;attempt++) {
      const raw=await owned.evaluate(sourceUrlScript(`(${observeLinkedinProfile.toString()})(${JSON.stringify(profileUrl)},${JSON.stringify(handle)})`));
      const data=raw && 'session' in Object(raw) && 'data' in Object(raw)?raw.data:raw;
      if(data) {profile=projectLinkedinProfile(data,handle);break;}
      if(attempt<11) await owned.sleep(0.5);
    }
    if(profile?.account_id!==account) throw new Error('ONECREW_LINKEDIN_INVALID');
    await owned.goto(source,{settleMs:2000});
    for(let attempt=0;attempt<12;attempt++) {
      const raw=await owned.evaluate(`(${observeLinkedinComments.toString()})(${JSON.stringify(source)},${JSON.stringify(profileUrl)},${JSON.stringify(handle)},${JSON.stringify(account)})`);
      const data=raw && 'session' in Object(raw) && 'data' in Object(raw)?raw.data:raw;
      if(data) return projectLinkedinComments(data,handle,account,source);
      if(attempt<11) await owned.sleep(0.5);
    }
    throw new Error('ONECREW_LINKEDIN_INVALID');
  });
}
