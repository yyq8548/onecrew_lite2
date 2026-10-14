import test from 'node:test';
import assert from 'node:assert/strict';
import {runInNewContext} from 'node:vm';
import {projectRedditProfile,redditProfileTarget,readRedditProfile} from './reddit-profile.mjs';
const profile = () => ({kind:'t2',data:{id:'abc123',name:'Example_User',link_karma:999999,
  subreddit:{subreddit_type:'user',display_name:'u_Example_User',public_description:'Original bio'}}});
test('native user binding preserves unknown followers instead of karma',()=>{
  const result=projectRedditProfile(profile(),'example_user','t2_abc123');
  assert.equal(result.follower_count,null);
  assert.equal(result.id,'t2_abc123');
  assert.equal(result.biography,'Original bio');
  assert.ok(!Object.hasOwn(result,'link_karma'));
});
test('only native own-profile subscribers supply the follower count, including zero',()=>{
  for(const count of [0,15]){
    const data=profile();data.data.subreddit.subscribers=count;
    assert.equal(projectRedditProfile(data,'Example_User','t2_abc123').follower_count,count);
  }
});
test('reject account drift, unrelated communities, unsafe counts and suspended profiles',()=>{
  for(const change of [p=>p.data.id='other',p=>p.data.name='another',p=>p.data.is_suspended=true,
    p=>p.data.subreddit.subreddit_type='public',p=>p.data.subreddit.display_name='u_other',
    p=>p.data.subreddit.subscribers=-1,p=>p.data.subreddit.subscribers=Number.MAX_SAFE_INTEGER+1]){
    const data=profile();change(data);
    assert.throws(()=>projectRedditProfile(data,'Example_User','t2_abc123'));
  }
});
test('reject path injection and non-native account IDs before browser reads',()=>{
  for(const username of ['../other','a/b','[deleted]','user?x=1'])
    assert.throws(()=>redditProfileTarget(username,'t2_abc123'));
  assert.throws(()=>redditProfileTarget('Example_User','Example_User'));
});
test('profile read propagates risk without fabricating an observation',async()=>{
  await assert.rejects(readRedditProfile({goto:async()=>{},evaluate:async()=>({stop:'risk'})},
    {username:'Example_User',author_id:'t2_abc123'},(page,read)=>read(page)),/PROFILE_RISK/);
});
test('only bounded public projection crosses the browser bridge',async()=>{
  const data=profile();data.data.unneeded_private_fixture='must-not-cross';
  let crossed;
  const page={goto:async()=>{},evaluate:async script=>{
    crossed=await runInNewContext(script,{location:{origin:'https://www.reddit.com'},URL,AbortSignal,
      fetch:async()=>({url:'https://www.reddit.com/user/Example_User/about.json',status:200,ok:true,json:async()=>data})});
    return crossed;
  }};
  const result=await readRedditProfile(page,{username:'Example_User',author_id:'t2_abc123'},(p,read)=>read(p));
  assert.equal(result.id,'t2_abc123');
  assert.equal(JSON.stringify(crossed).includes('must-not-cross'),false);
  assert.deepEqual(Object.keys(crossed),['profile']);
});
