import test from 'node:test';
import assert from 'node:assert/strict';
import vm from 'node:vm';
import {projectInstagramProfile,fetchInstagramProfile,readInstagramProfile,observeInstagramProfile,readInstagramWebProfile} from './instagram-profile.mjs';
const user=()=>({id:'123456789',username:'Creator',full_name:'Creator',biography:'Original bio',is_private:false,is_verified:true,edge_followed_by:{count:12000}});

test('explicit private profile preserves its link but never reads post previews',()=>{
  const main={innerText:'creator\nCreator\n18 posts\n639 followers\n738 following\nMouse reviews\nThis profile is private\nAlready follow creator? Log in to see their photos and videos.',
    querySelectorAll:()=>assert.fail('private profiles must not inspect posts')};
  const context=vm.createContext({URL,location:{origin:'https://www.instagram.com',pathname:'/creator/'},document:{querySelector:()=>main}});
  const result=vm.runInContext(`(${observeInstagramProfile.toString()})('creator','123')`,context);
  assert.equal(result.profile.private_account,true);
  assert.equal(result.profile.profile_url,'https://www.instagram.com/creator/');
  assert.equal(result.profile.web_text,'');
  assert.equal(result.profile.biography,'Mouse reviews');
});

test('public DOM preserves search binding, previews and unknown fields without API calls',async()=>{
  const main={innerText:'creator\nCreator\n18 posts\n1,079 followers\n810 following\nKeyboard reviews\nFollowed by private viewer\nFollow',querySelectorAll:()=>[
    {getAttribute:()=>'/creator/p/Abc123/',querySelector:()=>({getAttribute:()=> 'Keyboard preview'})},
    {getAttribute:()=>'/other/p/Foreign/',querySelector:()=>({getAttribute:()=> 'Unrelated preview'})},
  ]};
  const context=vm.createContext({URL,location:{origin:'https://www.instagram.com',pathname:'/creator/'},document:{querySelector:()=>main}});
  const result=vm.runInContext(`(${observeInstagramProfile.toString()})('creator','123')`,context);
  assert.equal(result.profile.id,'123');assert.equal(result.profile.follower_count,1079);
  assert.equal(result.profile.biography,'Keyboard reviews');assert.equal(result.profile.verified,null);
  assert.match(result.profile.web_text,/Keyboard preview/);assert.doesNotMatch(result.profile.web_text,/Unrelated|private viewer/);
  main.innerText=main.innerText.replace('1,079 followers','1.1K followers');
  assert.equal(vm.runInContext(`(${observeInstagramProfile.toString()})('creator','123')`,context).profile.follower_count,null);
  context.location.pathname='/other/';
  assert.equal(vm.runInContext(`(${observeInstagramProfile.toString()})('creator','123')`,context).stop,'identity');
  context.location.pathname='/challenge/';
  assert.equal(vm.runInContext(`(${observeInstagramProfile.toString()})('creator','123')`,context).stop,'risk');
});

test('web reader never falls back to the blocked profile API',async()=>{
  let evaluations=0;
  const page={goto:async()=>{},evaluate:async code=>{evaluations++;assert.ok(!code.includes('fetch('));return {stop:'risk'};}};
  await assert.rejects(readInstagramWebProfile(page,{username:'creator',author_id:'123'},(p,read)=>read(p)),/PROFILE_RISK/);
  assert.equal(evaluations,1);
});
test('Instagram public profile preserves native identity and observed count',()=>{
  const p=projectInstagramProfile(user(),'creator');
  assert.equal(p.id,'123456789');assert.equal(p.follower_count,12000);assert.equal(p.profile_url,'https://www.instagram.com/creator/');
  assert.equal(p.biography,'Original bio');
});
test('Instagram unknown count differs from known zero; ambiguous identities fail',()=>{
  const unknown=user();delete unknown.edge_followed_by;assert.equal(projectInstagramProfile(unknown,'creator').follower_count,null);
  unknown.follower_count=0;assert.equal(projectInstagramProfile(unknown,'creator').follower_count,0);
  for(const change of [u=>u.pk='999',u=>u.id=Number.MAX_SAFE_INTEGER+1,u=>u.username='other',u=>u.edge_followed_by.count=-1]) {
    const u=user();change(u);assert.throws(()=>projectInstagramProfile(u,'creator'));
  }
});
async function fetchFixture(responses) {
  const requests=[];
  const context=vm.createContext({URL,AbortSignal,location:{origin:'https://www.instagram.com',pathname:'/creator/'},fetch:async(path,opts)=>{
    requests.push(path);assert.equal(opts.credentials,'include');
    const next=responses.shift();assert.ok(next,'bounded request count');
    if(next.error) throw next.error;
    return {status:next.status,ok:next.status===200,url:next.url??'https://www.instagram.com'+path,json:async()=>next.data};
  }});
  const result=await vm.runInContext(`(${fetchInstagramProfile.toString()})('creator')`,context);
  return {result,requests};
}
test('Instagram professional-account path binds feed identity to returned profile',async()=>{
  const {result,requests}=await fetchFixture([{status:400},{status:200,data:{user:{pk:'123456789',username:'creator'}}},{status:200,data:{user:{...user(),pk:'123456789'}}}]);
  assert.equal(result.user.id,'123456789');assert.equal(requests.length,3);
  const mismatch=await fetchFixture([{status:400},{status:200,data:{user:{pk:'123456789',username:'other'}}}]);
  assert.equal(mismatch.result.stop,'identity');assert.equal(mismatch.requests.length,2);
});
test('Instagram login, rate limit and redirects cannot become empty profiles or trigger retries',async()=>{
  for(const response of [{status:401},{status:429},{status:200,url:'https://evil.example/profile'}]) {
    const {result,requests}=await fetchFixture([response]);assert.ok(result.stop);assert.equal(requests.length,1);
  }
});

test('Instagram preserves native rejection status without retrying another endpoint',async()=>{
  for (const status of [403,429]) {
    const {result,requests}=await fetchFixture([{status}]);
    assert.equal(result.stop,'risk');
    assert.equal(result.status,status);
    assert.equal(requests.length,1);
  }
});

test('Instagram profile reader carries HTTP status to its command error',async()=>{
  const page={goto:async()=>{},evaluate:async()=>({stop:'risk',status:429})};
  await assert.rejects(readInstagramProfile(page,{username:'creator'},async(p,task)=>task(p)),
    error=>error.message==='ONECREW_PROFILE_RISK' && error.httpStatus===429);
});
test('Instagram rejects unsafe username before browser navigation',async()=>{
  for(const username of ['../accounts','creator?x=1','https://evil.example',''])
    await assert.rejects(readInstagramProfile(null,{username},()=>assert.fail('must not navigate')));
});
test('Instagram request timeout is distinct and does not retry or invent a count',async()=>{
  const {result,requests}=await fetchFixture([{error:Object.assign(new Error('timeout'),{name:'TimeoutError'})}]);
  assert.equal(result.stop,'timeout');assert.equal(requests.length,1);assert.equal(result.user,undefined);
});

test('Instagram keeps public contact fields separate from biography',()=>{
  const u=user();u.public_email='business@example.com';u.external_url='https://creator.example/contact';
  u.bio_links=[{url:'https://linktr.ee/creator'},{url:'https://creator.example/contact'},{url:'javascript:alert(1)'}];
  const p=projectInstagramProfile(u,'creator');
  assert.equal(p.biography,'Original bio');assert.equal(p.public_email,'business@example.com');
  assert.deepEqual(p.contact_links,['https://creator.example/contact','https://linktr.ee/creator']);
});
test('Instagram contact projection excludes credentials and bounds links',()=>{
  const u=user();u.external_url='https://secret:password@creator.example/';
  u.bio_links=Array.from({length:25},(_,i)=>({url:`https://creator.example/${i}`}));
  const p=projectInstagramProfile(u,'creator');assert.equal(p.public_email,null);
  assert.equal(p.contact_links.length,10);assert.ok(p.contact_links.every(s=>!s.includes('password')));
});

 test('public follower tooltip supplies exact count behind abbreviated display',()=>{
  const follower={getAttribute:k=>k==='href'?'/creator/followers/':null,querySelectorAll:()=>[{getAttribute:()=> '1,234,567'}]};
  const main={innerText:'creator\nCreator\n18 posts\n1.2M followers\n810 following\nMakeup',querySelectorAll:selector=>selector==='a[href]'?[follower]:[]};
  const context=vm.createContext({URL,location:{origin:'https://www.instagram.com',pathname:'/creator/'},document:{querySelector:()=>main}});
  assert.equal(vm.runInContext(`(${observeInstagramProfile.toString()})('creator','123')`,context).profile.follower_count,1234567);
  follower.getAttribute=k=>k==='href'?'/other/followers/':null;
  assert.equal(vm.runInContext(`(${observeInstagramProfile.toString()})('creator','123')`,context).profile.follower_count,null);
 });
