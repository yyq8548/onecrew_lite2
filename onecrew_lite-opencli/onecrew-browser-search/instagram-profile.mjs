import {observeInstagramVisibleProfile,observeInstagramVisiblePosts} from './instagram-visible.mjs';
import {equivalentSourceUrl,sourceUrlScript} from './source-url.mjs';

export function instagramProfileTarget(username) {
  if (typeof username !== 'string' || !/^[A-Za-z0-9_.]{1,30}$/.test(username)) throw new Error('ONECREW_PROFILE_INPUT');
  return username.toLowerCase();
}

export async function waitForInstagramProfile(username) {
  for (let poll = 0; poll < 32; poll++) {
    if (/^\/(accounts\/login|challenge|checkpoint)\//.test(location.pathname)) return;
    const main=document.querySelector('main'),header=main?.querySelector('header'),heading=header?.querySelector('h1,h2');
    const text=header?.innerText||'';
    const countsReady=/(?:[0-9][0-9,.\s]*[KMB万亿千]?\s*)(?:followers|粉丝|位粉丝)/i.test(text);
    const privateAccount=/This (?:profile|account) is private|这是私密(?:账户|帐号|账号)|此(?:帐户|账户|帐号|账号)(?:是|为)私密/i.test(main?.innerText||'');
    if (heading?.textContent?.trim().toLowerCase()===username&&(countsReady||privateAccount)) return;
    if (poll<31) await new Promise(resolve=>setTimeout(resolve,250));
  }
  throw new Error('ONECREW_PROFILE_TIMEOUT');
}

export async function readInstagramWebProfile(page,args,ownPage) {
  const username=instagramProfileTarget(args.username);
  if (!/^[1-9][0-9]{0,24}$/.test(args.author_id||'')&&args.author_id!==`https://www.instagram.com/${username}/`) throw new Error('ONECREW_PROFILE_INPUT');
  return ownPage(page,async owned=>{
    const unwrap=raw=>raw&&'session' in Object(raw)&&'data' in Object(raw)?raw.data:raw;
    await owned.goto(`https://www.instagram.com/${username}/`,{settleMs:0});
    await owned.evaluate(`(${waitForInstagramProfile.toString()})(${JSON.stringify(username)})`);
    const expansion=unwrap(await owned.evaluate(sourceUrlScript(`(${instagramVisibleExpansion.toString()})(${JSON.stringify(username)})`)));
    if(expansion?.stop) throw new Error('ONECREW_PROFILE_'+expansion.stop.toUpperCase());
    if(expansion?.selector) await owned.click(expansion.selector);
    const data=unwrap(await owned.evaluate(sourceUrlScript(`(${observeInstagramVisibleProfile.toString()})(${JSON.stringify(username)},${JSON.stringify(args.author_id)})`)));
    if(!data?.profile) throw new Error('ONECREW_PROFILE_'+(data?.stop||'read').toUpperCase());
    if(data.profile.private_account!==true) {
      const observed=unwrap(await owned.evaluate(sourceUrlScript(`(${observeInstagramVisiblePosts.toString()})(${JSON.stringify(username)})`)));
      if(observed?.stop) throw new Error('ONECREW_PROFILE_'+observed.stop.toUpperCase());
      if(!Array.isArray(observed?.posts)) throw new Error('ONECREW_INSTAGRAM_PROFILE_INVALID');
      data.profile.visible_posts=observed.posts;
      const preview='Visible profile post previews; bounded excerpts, not verified full original captions.\n'+observed.posts.map(post=>`${post.url}\n${post.preview}`).join('\n\n');
      let bytes=0;const characters=[];
      for(const character of preview) {
        const size=new TextEncoder().encode(character).length;
        if(bytes+size>3900) break;
        characters.push(character);bytes+=size;
      }
      data.profile.web_text=observed.posts.length?characters.join(''):'';
    }
    return data.profile;
  });
}

export function instagramVisibleExpansion(username) {
  if(!equivalentSourceUrl(location.href,`https://www.instagram.com/${username}/`)) return {stop:'identity'};
  const main=document.querySelector('main'),header=main?.querySelector('header');
  if(!header) return {stop:'read'};
  const buttons=[...header.querySelectorAll('button,[role="button"],span')].filter(node=>node.getClientRects().length&&/^(更多|more)$/i.test(node.innerText.trim())&&!Array.from(node.children).some(child=>/^(更多|more)$/i.test(child.innerText.trim())));
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
