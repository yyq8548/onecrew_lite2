export function redditProfileTarget(username, authorId) {
  if (typeof username !== 'string' || !/^[A-Za-z0-9_-]{3,20}$/.test(username)
      || !/^t2_[a-z0-9]{1,15}$/.test(authorId ?? '')) throw new Error('ONECREW_REDDIT_PROFILE_INPUT');
  return username;
}

export function projectRedditProfile(payload, username, authorId) {
  redditProfileTarget(username, authorId);
  const user = payload?.data;
  const invalid = () => { throw new Error('ONECREW_REDDIT_PROFILE_INVALID'); };
  if (payload?.kind !== 't2' || user?.is_suspended === true || typeof user?.id !== 'string'
      || `t2_${user.id}` !== authorId || typeof user.name !== 'string'
      || user.name.toLowerCase() !== username.toLowerCase()) invalid();
  const community = user.subreddit;
  let follower_count = null;
  let biography = '';
  if (community != null) {
    if (community.subreddit_type !== 'user'
        || typeof community.display_name !== 'string'
        || community.display_name.toLowerCase() !== `u_${user.name.toLowerCase()}`) invalid();
    if (community.subscribers != null) {
      if (!Number.isSafeInteger(community.subscribers) || community.subscribers < 0) invalid();
      follower_count = community.subscribers;
    }
    if (community.public_description != null) {
      if (typeof community.public_description !== 'string' || community.public_description.includes('\0')) invalid();
      biography = Array.from(community.public_description).slice(0, 2000).join('');
    }
  }
  return {adapter_version:'1.0.0',id:authorId,username:user.name,biography,follower_count,
    profile_url:`https://www.reddit.com/user/${user.name}/`,evidence_source:'native_profile_response'};
}

export async function fetchRedditProfile(username) {
  if (location.origin !== 'https://www.reddit.com') return {stop:'read'};
  const path = '/user/' + encodeURIComponent(username) + '/about.json';
  const response = await fetch(path + '?raw_json=1', {credentials:'include',signal:AbortSignal.timeout(8000)});
  const url = new URL(response.url);
  if (url.origin !== location.origin) return {stop:'read'};
  if (/^\/login(?:\/|$)/.test(url.pathname) || response.status === 401) return {stop:'auth'};
  if (response.status === 429) return {stop:'risk'};
  if ([403,404].includes(response.status)) return {stop:'inaccessible'};
  if (!response.ok || url.pathname.toLowerCase() !== path.toLowerCase()) return {stop:'read'};
  return {payload:await response.json()};
}

export async function readRedditProfile(page, args, ownPage) {
  const username = redditProfileTarget(args.username, args.author_id);
  return ownPage(page, async owned => {
    await owned.goto(`https://www.reddit.com/user/${username}/`);
    const response = await owned.evaluate(`(async()=>{
      const result=await (${fetchRedditProfile.toString()})(${JSON.stringify(username)});
      if(result.stop)return result;
      const redditProfileTarget=${redditProfileTarget.toString()};
      return {profile:(${projectRedditProfile.toString()})(result.payload,${JSON.stringify(username)},${JSON.stringify(args.author_id)})};
    })()`);
    const result = response && 'session' in Object(response) && 'data' in Object(response) ? response.data : response;
    if (result?.stop) throw new Error('ONECREW_REDDIT_PROFILE_' + result.stop.toUpperCase());
    if(!result?.profile) throw new Error('ONECREW_REDDIT_PROFILE_INVALID');
    return result.profile;
  });
}
