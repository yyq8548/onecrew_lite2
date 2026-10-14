import {restoreOriginalLanguage} from './original-language.mjs';

export function facebookReelTarget(source) {
  const match = typeof source === 'string' && /^https:\/\/www\.facebook\.com\/reel\/([1-9][0-9]{0,24})\/?$/.exec(source);
  if (!match) throw new Error('ONECREW_COMMENTS_INPUT');
  return {id:match[1], source:source.replace(/\/$/,'')};
}

export function projectFacebookReelComments(data, source, limit, contentOnly=false) {
  const invalid = () => {throw new Error('ONECREW_FACEBOOK_COMMENTS_INVALID');};
  if (!/^https:\/\/www\.facebook\.com\/reel\/[1-9][0-9]{0,24}$/.test(source)
      || !Number.isInteger(limit) || limit < 1 || limit > 20
      || !/^[1-9][0-9]{0,19}$/.test(data?.author_id ?? '')
      || !Array.isArray(data.comments) || data.comments.length > 100) invalid();
  const text = (value, max) => {
    if (typeof value !== 'string' || !value.trim() || value.includes('\0')) invalid();
    const chars=Array.from(value);
    return {text:chars.slice(0,max).join(''),text_truncated:chars.length>max};
  };
  const rows=[{type:'POST',source_url:source,native_author_id:data.author_id,
    author_url:`https://www.facebook.com/profile.php?id=${data.author_id}`,
    identity_source:'visible_numeric_profile_link',...text(data.text,8000)}];
  const seen=new Set();
  for(const comment of data.comments) {
    let url;
    try {url=new URL(comment.source_url);} catch {invalid();}
    const ids=url.searchParams.getAll('comment_id'), id=ids[0];
    if(url.origin!=='https://www.facebook.com' || url.username || url.password || url.hash
        || `${url.origin}${url.pathname.replace(/\/$/,'')}`!==source
        || ids.length!==1 || !/^[1-9][0-9]{0,24}$/.test(id ?? '')
        || url.searchParams.has('reply_comment_id')) invalid();
    if(seen.has(id)) continue;
    seen.add(id);
    rows.push({type:'COMMENT',source_url:`${source}?comment_id=${id}`,...text(comment.text,2000)});
    if(rows.length===limit+1) break;
  }
  if(rows.length===1 && !contentOnly) throw new Error('ONECREW_FACEBOOK_COMMENTS_UNAVAILABLE');
  return rows;
}

export async function readFacebookReelPage(source, limit, project, restore, contentOnly=false) {
  const visible=n=>{const r=n.getBoundingClientRect();return r.width>0&&r.height>0&&r.top<innerHeight&&r.bottom>0&&r.left<innerWidth&&r.right>0;};
  const check=()=>{
    if(location.origin!=='https://www.facebook.com') throw new Error('ONECREW_FACEBOOK_SOURCE_INVALID');
    if(/^\/(?:login|recover)(?:\/|\.php|$)/.test(location.pathname)) throw new Error('ONECREW_COMMENTS_AUTH');
    if(/^\/(?:checkpoint|challenge|two_step_verification)(?:\/|$)/.test(location.pathname)) throw new Error('ONECREW_COMMENTS_RISK');
    if(`${location.origin}${location.pathname.replace(/\/$/,'')}`!==source || location.search || location.hash)
      throw new Error('ONECREW_FACEBOOK_SOURCE_INVALID');
  };
  const identity=root=>{
    const links=[...root.querySelectorAll('h2 a[href]')];
    if(links.length!==1) return null;
    const url=new URL(links[0].href), ids=url.searchParams.getAll('id');
    return url.origin==='https://www.facebook.com' && url.pathname==='/profile.php'
      && ids.length===1 && /^[1-9][0-9]{0,19}$/.test(ids[0]) ? ids[0] : null;
  };
  let opened=false;
  for(let attempt=0;attempt<24;attempt++) {
    check();
    const players=[...document.querySelectorAll('[role="main"] [role="group"][aria-label="Video player"]')].filter(visible);
    if(players.length>1) throw new Error('ONECREW_FACEBOOK_SOURCE_AMBIGUOUS');
    const owner=players.length===1 && identity(players[0]);
    const roots=[...document.querySelectorAll('[role="complementary"]')].filter(visible);
    if(roots.length>1) throw new Error('ONECREW_FACEBOOK_SOURCE_AMBIGUOUS');
    const root=contentOnly ? players[0] : roots[0];
    if(owner && root) {
      if(identity(root)!==owner) throw new Error('ONECREW_FACEBOOK_SOURCE_INVALID');
      await restore(); check();
      if(identity(root)!==owner || identity(players[0])!==owner) throw new Error('ONECREW_FACEBOOK_SOURCE_INVALID');
      const bodies=[...root.querySelectorAll('span[dir="auto"]')].filter(n=>n.querySelector('div')
        && !n.closest('[role="article"], [role="button"], h2, [contenteditable="true"]')
        && !n.querySelector('span[dir="auto"]'));
      const comments=[];
      for(const article of (contentOnly ? [] : [...root.querySelectorAll('[role="article"]')].slice(0,100))) {
        if(article.parentElement.closest('[role="article"]') || !article.getClientRects().length) continue;
        const blocks=[...article.querySelectorAll('div[dir="auto"]')].filter(n=>!n.querySelector('div[dir="auto"]')&&!n.closest('[role="button"],a'));
        const links=[...article.querySelectorAll('a[href]')].filter(a=>{
          const u=new URL(a.href);
          return `${u.origin}${u.pathname.replace(/\/$/,'')}`===source && u.searchParams.has('comment_id')&&!u.searchParams.has('reply_comment_id');
        });
        if(blocks.length===1 && links.length===1) comments.push({text:blocks[0].innerText,source_url:links[0].href});
      }
      if(bodies.length===1 && (contentOnly || comments.length)) return project({author_id:owner,text:bodies[0].innerText,comments},source,limit,contentOnly);
    } else if(owner && !root && !opened) {
      const buttons=[...document.querySelectorAll('[role="main"] [role="button"][aria-label]')].filter(n=>visible(n)&&/^(Comment|评论|評論)$/.test(n.getAttribute('aria-label')));
      if(buttons.length===1) {buttons[0].click();opened=true;}
    }
    if(attempt<23) await new Promise(resolve=>setTimeout(resolve,500));
  }
  throw new Error('ONECREW_FACEBOOK_COMMENTS_UNAVAILABLE');
}

export async function readFacebookReelComments(page,args,ownPage,contentOnly=false) {
  const target=facebookReelTarget(args.id),limit=args.limit??20;
  if(!Number.isInteger(limit)||limit<1||limit>20) throw new Error('ONECREW_COMMENTS_INPUT');
  return ownPage(page,async owned=>{
    await owned.goto(target.source,{settleMs:3000});
    const result=await owned.evaluate(`(${readFacebookReelPage.toString()})(${JSON.stringify(target.source)},${limit},${projectFacebookReelComments.toString()},${restoreOriginalLanguage.toString()},${contentOnly})`);
    return result && 'session' in Object(result) && 'data' in Object(result) ? result.data : result;
  });
}
