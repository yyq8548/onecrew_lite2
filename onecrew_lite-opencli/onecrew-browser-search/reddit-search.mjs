export function projectRedditSearch(payload, limit) {
  const invalid=()=>{throw new Error('ONECREW_REDDIT_SEARCH_INVALID');};
  if(!Number.isInteger(limit)||limit<1||limit>20||payload?.kind!=='Listing'
      ||!Array.isArray(payload?.data?.children)||payload.data.children.length>100)invalid();
  return payload.data.children.slice(0,limit).map(entry=>{
    const post=entry?.data;
    if(entry?.kind!=='t3'||!post||!/^t3_[a-z0-9]{1,15}$/.test(post.name??'')
        ||post.name!==`t3_${post.id}`||typeof post.title!=='string'||!post.title.trim()
        ||post.title.includes('\0')||typeof post.author!=='string')invalid();
    let author_id=null,author_url=null;
    if(!['[deleted]','[removed]'].includes(post.author)&&post.author_fullname!=null){
      if(!/^[A-Za-z0-9_-]{3,20}$/.test(post.author)||!/^t2_[a-z0-9]{1,15}$/.test(post.author_fullname))invalid();
      author_id=post.author_fullname;
      author_url=`https://www.reddit.com/user/${post.author}/`;
    }
    if(post.selftext!=null&&(typeof post.selftext!=='string'||post.selftext.includes('\0')))invalid();
    // Keep only text already returned by this search read, bound to the same post.
    // A short title alone often cannot distinguish review evidence from a request.
    const body=['[removed]','[deleted]'].includes(post.selftext)?'':(post.selftext??'').trim();
    // Search previews often begin with a long image URL. Preserve visible link
    // labels but not Markdown destinations, so the bounded excerpt retains prose.
    const prose=body.replace(/!?\[([^\]\n]*)\]\(https?:\/\/[^\s)]+\)/g,'$1')
      .replace(/https?:\/\/preview\.redd\.it\/\S+/g,'').trim();
    const heading=Array.from(post.title).slice(0,prose?200:500).join('');
    const preview=prose?`${heading}\n\n${prose}`:heading;
    return {provider:'onecrew-reddit-native-v1',id:post.id,title:Array.from(preview).slice(0,500).join(''),
      author:post.author,url:`https://www.reddit.com/comments/${post.id}/`,author_id,author_url};
  });
}

export async function fetchRedditSearch(query,limit){
  if(location.origin!=='https://www.reddit.com')return {stop:'read'};
  const params=new URLSearchParams({q:query,sort:'relevance',t:'all',limit:String(limit),raw_json:'1'});
  const response=await fetch('/search.json?'+params,{credentials:'include',signal:AbortSignal.timeout(8000)});
  const url=new URL(response.url);
  if(url.origin!==location.origin)return {stop:'read'};
  if(/^\/login(?:\/|$)/.test(url.pathname)||response.status===401)return {stop:'auth'};
  if([403,429].includes(response.status))return {stop:'risk'};
  if(!response.ok||url.pathname!=='/search.json')return {stop:'read'};
  return {payload:await response.json()};
}

export async function readRedditSearch(page,args,ownPage){
  const limit=args.limit??20,query=args.query;
  if(typeof query!=='string'||!query.trim()||query.length>1000||query.includes('\0')
      ||!Number.isInteger(limit)||limit<1||limit>20)throw new Error('ONECREW_REDDIT_SEARCH_INPUT');
  return ownPage(page,async owned=>{
    await owned.goto('https://www.reddit.com/');
    const response=await owned.evaluate(`(async()=>{
      const result=await (${fetchRedditSearch.toString()})(${JSON.stringify(query)},${limit});
      return result.stop?result:{rows:(${projectRedditSearch.toString()})(result.payload,${limit})};
    })()`);
    const result=response&&'session' in Object(response)&&'data' in Object(response)?response.data:response;
    if(result?.stop)throw new Error('ONECREW_REDDIT_SEARCH_'+result.stop.toUpperCase());
    if(!Array.isArray(result?.rows))throw new Error('ONECREW_REDDIT_SEARCH_INVALID');
    return result.rows;
  });
}
