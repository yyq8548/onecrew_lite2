import {redditProfileTarget} from './reddit-profile.mjs';

export function projectRedditRecent(payload, username, authorId) {
  redditProfileTarget(username, authorId);
  const invalid=()=>{throw new Error('ONECREW_REDDIT_RECENT_INVALID');};
  if(payload?.kind!=='Listing'||!Array.isArray(payload?.data?.children)||payload.data.children.length>100) invalid();
  const urls=[],seen=new Set();
  for(const entry of payload.data.children){
    const post=entry?.data;
    if(entry?.kind!=='t3'||!post||typeof post.author!=='string') invalid();
    if(['[deleted]','[removed]'].includes(post.author)) continue;
    if(post.author_fullname!==authorId||post.author.toLowerCase()!==username.toLowerCase()
        ||!/^t3_[a-z0-9]{1,15}$/.test(post.name??'')||post.name!==`t3_${post.id}`||seen.has(post.id)) invalid();
    seen.add(post.id);
    if(post.crosspost_parent||['[deleted]','[removed]'].includes(post.selftext)) continue;
    urls.push({source_url:`https://www.reddit.com/comments/${post.id}/`});
    if(urls.length===3)break;
  }
  return urls;
}

export async function fetchRedditRecent(username){
  if(location.origin!=='https://www.reddit.com')return {stop:'read'};
  const path='/user/'+encodeURIComponent(username)+'/submitted.json';
  const response=await fetch(path+'?sort=new&limit=10&raw_json=1',{credentials:'include',signal:AbortSignal.timeout(8000)});
  const url=new URL(response.url);
  if(url.origin!==location.origin)return {stop:'read'};
  if(/^\/login(?:\/|$)/.test(url.pathname)||response.status===401)return {stop:'auth'};
  if(response.status===429)return {stop:'risk'};
  if([403,404].includes(response.status))return {stop:'inaccessible'};
  if(!response.ok||url.pathname.toLowerCase()!==path.toLowerCase())return {stop:'read'};
  return {payload:await response.json()};
}

export async function readRedditRecent(page,args,ownPage){
  const username=redditProfileTarget(args.username,args.author_id);
  return ownPage(page,async owned=>{
    await owned.goto(`https://www.reddit.com/user/${username}/submitted/`);
    const response=await owned.evaluate(`(async()=>{
      const result=await (${fetchRedditRecent.toString()})(${JSON.stringify(username)});
      if(result.stop)return result;
      const redditProfileTarget=${redditProfileTarget.toString()};
      return {rows:(${projectRedditRecent.toString()})(result.payload,${JSON.stringify(username)},${JSON.stringify(args.author_id)})};
    })()`);
    const result=response&&'session' in Object(response)&&'data' in Object(response)?response.data:response;
    if(result?.stop)throw new Error('ONECREW_REDDIT_RECENT_'+result.stop.toUpperCase());
    if(!Array.isArray(result?.rows))throw new Error('ONECREW_REDDIT_RECENT_INVALID');
    return result.rows;
  });
}
