import {observeInstagramVisiblePosts,observeInstagramVisibleCaption} from './instagram-visible.mjs';
import {waitForInstagramProfile} from './instagram-profile.mjs';
import {sourceUrlScript} from './source-url.mjs';

export function selectInstagramVisiblePost(posts,query='') {
  const terms=[...new Set(query.normalize('NFKC').toLowerCase().match(/[\p{L}\p{N}]{3,}/gu)||[])].filter(term=>!['blogger','creator','influencer','account','official'].includes(term));
  const score=post=>terms.reduce((sum,term)=>sum+(post.preview?.normalize('NFKC').toLowerCase().includes(term)?1:0),0);
  return posts.reduce((best,post)=>!best||score(post)>score(best)?post:best,null);
}

export function instagramRecentTarget(username,authorId) {
  if(typeof username!=='string'||!/^[a-z0-9_.]{1,30}$/.test(username)||typeof authorId!=='string'
    ||!/^[1-9][0-9]{0,24}$/.test(authorId)&&authorId!==`https://www.instagram.com/${username}/`) throw new Error('ONECREW_INSTAGRAM_RECENT_INPUT');
}

export async function readInstagramRecent(page,args,ownPage) {
  instagramRecentTarget(args.username,args.author_id);
  return ownPage(page,async owned=>{
    const unwrap=raw=>raw&&'session' in Object(raw)&&'data' in Object(raw)?raw.data:raw;
    let post;
    if(args.source_url) {
      const match=/^https:\/\/www\.instagram\.com\/([^/]+)\/(p|reel)\/([A-Za-z0-9_-]{1,32})\/$/.exec(args.source_url);
      if(!match||match[1]!==args.username) throw new Error('ONECREW_INSTAGRAM_RECENT_INPUT');
      post={url:args.source_url,kind:match[2],code:match[3]};
    } else {
      await owned.goto(`https://www.instagram.com/${args.username}/`,{settleMs:0});
      await owned.evaluate(`(${waitForInstagramProfile.toString()})(${JSON.stringify(args.username)})`);
      const observed=unwrap(await owned.evaluate(sourceUrlScript(`(${observeInstagramVisiblePosts.toString()})(${JSON.stringify(args.username)})`)));
      if(observed?.stop==='private') return [];
      if(observed?.stop) throw new Error('ONECREW_INSTAGRAM_RECENT_'+observed.stop.toUpperCase());
      if(!Array.isArray(observed?.posts)||!observed.posts.length) throw new Error('ONECREW_INSTAGRAM_RECENT_POST_LINK_TIMEOUT');
      post=selectInstagramVisiblePost(observed.posts,args.query||'');
    }
    await owned.goto(post.url,{settleMs:0});
    const caption=unwrap(await owned.evaluate(`(${observeInstagramVisibleCaption.toString()})(${JSON.stringify(args.username)},${JSON.stringify(post.code)})`));
    if(caption?.stop) throw new Error('ONECREW_INSTAGRAM_RECENT_'+caption.stop.toUpperCase());
    if(typeof caption?.text!=='string'||typeof caption.text_truncated!=='boolean') throw new Error('ONECREW_INSTAGRAM_RECENT_INVALID');
    return [{post_id:post.code,author_id:args.author_id,source_url:`https://www.instagram.com/${post.kind}/${post.code}/`,...caption}];
  });
}
