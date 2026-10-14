import assert from 'node:assert/strict';
import test from 'node:test';
import {douyinAuthor,douyinVideo,douyinExactFollowers,projectDouyinProfile,projectDouyinContent,observeDouyinEvidence,readDouyinEvidence} from './douyin-evidence.mjs';
import {withOwnedSearchPage} from './owned-page.mjs';
const author='MS4wLjABAAAAg5NIc2YQ9j1jGu9OUd4tJQc_C9cDFfpTWiLKTwWztLg';
const video='7688639864627214835';
const profile=`https://www.douyin.com/user/${author}`,source=`https://www.douyin.com/video/${video}`;
const rawProfile=()=>({author_id:author,author_label:'Creator',profile_url:profile,follower_text:'4892',bio:'Public business creator@example.com',bio_truncated:false,public_links:['https://creator.example/contact'],operator:'private'});
const rawContent=()=>({post_id:video,author_id:author,author_label:'Creator',profile_url:profile,source_url:source,text:'作者原创键盘实测标题',text_truncated:false,comments:['not original'],footer:'feedback@douyin.com'});
test('native author and video inputs never use names or editable display account numbers',()=>{
  assert.equal(douyinAuthor(author),author);assert.equal(douyinVideo(video),video);
  for(const value of ['Creator','23667677481','MS4wLjABAAAA1234567',`${author}?author_id=123`,`${author}/`])assert.throws(()=>douyinAuthor(value));
  for(const value of ['0','0123','123?x=1','123/','x'])assert.throws(()=>douyinVideo(value));
  for(const value of ['4.8千','4.8万','9.5万','4K','粉丝4892','4892 likes','004892'])assert.equal(douyinExactFollowers(value),null);
  assert.equal(douyinExactFollowers('4,892'),4892);assert.equal(douyinExactFollowers('0'),0);
});
test('typed profile retains public contacts, exact followers and bounded original text only',()=>{
  const result=projectDouyinProfile(rawProfile(),author);
  assert.equal(result.follower_count,4892);assert.equal(result.operator,undefined);
  assert.equal(projectDouyinProfile({...rawProfile(),follower_text:'4.8万'},author).follower_count,null);
  for(const change of [{author_id:author+'x'},{profile_url:source},{author_label:''},{bio:'語'.repeat(2700)},
    {bio:'x\0x'},{public_links:['https://127.0.0.1/']},{public_links:['javascript:bad']},
    {public_links:['https://user:secret@creator.example/']},{bio_truncated:null}])assert.throws(()=>projectDouyinProfile({...rawProfile(),...change},author));
});
test('typed caption preserves actual source and sec_uid without comments or footer data',()=>{
  const result=projectDouyinContent(rawContent(),video,author);
  assert.equal(result.author_id,author);assert.equal(result.text,'作者原创键盘实测标题');
  assert.equal(result.comments,undefined);assert.equal(result.footer,undefined);
  assert.equal(projectDouyinContent(rawContent(),video).profile_url,profile);
  for(const change of [{author_id:author+'x'},{post_id:'999'},{source_url:source+'?author_id=123'},
    {profile_url:profile+'?author_id=123'},{text:''},{text:'語'.repeat(4100)},{text_truncated:null}])assert.throws(()=>projectDouyinContent({...rawContent(),...change},video,author));
});

// Model the native header boundary observed in the live page. The outer page
// deliberately includes unrelated footer, recommendation and comment text.
function dom(kind) {
  const node=(text,tag='DIV')=>({tagName:tag,textContent:text,innerText:text,children:[],setAttribute(){},getClientRects:()=>[1],closest:()=>null,querySelectorAll:()=>[]});
  const title=node(kind==='profile'?'Creator':'作者原创标题','H1');
  const fans=node('粉丝4892');
  const bio=node('公开邮箱 creator@example.com');
  const expanded=node('公开邮箱 creator@example.com','P');
  let expandedVisible=true;
  const more=node('更多','SPAN');let clicks=0;more.click=()=>{clicks++;expandedVisible=true;};
  bio.querySelectorAll=selector=>selector==='p'?(expandedVisible?[expanded]:[]):selector==='*'?[more]:[];
  const metadata=node('抖音号：23667677481 IP属地：其他 43岁','P');metadata.nextElementSibling=bio;
  const anchors=[{...node(''),href:profile},{...node('Creator'),href:profile}];
  const header=node('');header.children=[node(''),node(''),metadata,bio];
  header.querySelectorAll=selector=>selector==='h1'?[title]:selector==='[data-e2e="user-info-fans"]'?[fans]:selector==='a[href]'?anchors:[];
  let risk=false,auth=false,translated=false;
  return {document:{documentElement:{classList:{contains:()=>translated}},querySelectorAll:selector=>
    selector==='[data-e2e="user-info"]'?[header]:selector==='h1'?[title]:selector.startsWith('iframe')?(risk?[node('security')]:[]):
    selector.startsWith('.login')?(auth?[node('登录')]:[]):[],
    createTreeWalker:root=>{let done=false;return {nextNode:()=>{if(done)return false;done=true;return true;},currentNode:{parentElement:root,textContent:root.textContent}};}},
    title,anchors,header,bio,setRisk:value=>risk=value,setAuth:value=>auth=value,setTranslation:value=>translated=value,
    collapse:()=>expandedVisible=false,clicks:()=>clicks};
}
function withDom(kind,task) {
  const previous={document:globalThis.document,location:globalThis.location,NodeFilter:globalThis.NodeFilter};
  const fixture=dom(kind);Object.assign(globalThis,{document:fixture.document,location:{origin:'https://www.douyin.com',href:kind==='profile'?profile:source},NodeFilter:{SHOW_TEXT:4}});
  try{return task(fixture);}finally{Object.assign(globalThis,previous);}
}
test('visible profile uses only native fans and biography, excluding metadata and footer',()=>withDom('profile',fixture=>{
  const row=observeDouyinEvidence('profile',author,profile);
  assert.equal(row.follower_text,'4892');assert.equal(row.bio,'公开邮箱 creator@example.com');
  assert.equal(row.bio.includes('43岁'),false);assert.equal(row.bio.includes('feedback@douyin.com'),false);
  fixture.collapse();const collapsed=observeDouyinEvidence('profile',author,profile,false);assert.equal(collapsed.bio_truncated,true);
  assert.deepEqual(observeDouyinEvidence('profile',author,profile,true),{expand_requested:true});assert.equal(fixture.clicks(),1);
}));
test('video caption binds the visible author, never query author_id or recommendation authors',()=>withDom('content',fixture=>{
  const row=observeDouyinEvidence('content',video,source);assert.equal(row.author_id,author);assert.equal(row.text,'作者原创标题');
  fixture.anchors[1].href=profile+'?author_id=123';assert.equal(observeDouyinEvidence('content',video,source),null);
  fixture.anchors[1].href=`https://www.douyin.com/user/${author}other`;assert.equal(observeDouyinEvidence('content',video,source),null);
}));
test('wrong source, verification, login and translated page fail before collecting evidence',()=>withDom('content',fixture=>{
  fixture.setRisk(true);assert.throws(()=>observeDouyinEvidence('content',video,source),/RISK/);fixture.setRisk(false);
  fixture.setAuth(true);assert.throws(()=>observeDouyinEvidence('content',video,source),/AUTH/);fixture.setAuth(false);
  fixture.setTranslation(true);assert.throws(()=>observeDouyinEvidence('content',video,source),/ORIGINAL_TEXT_UNCONFIRMED/);fixture.setTranslation(false);
  globalThis.location.href=source+'?x=1';assert.throws(()=>observeDouyinEvidence('content',video,source),/INVALID/);
}));
test('owned reads expand biography once after delayed load and always close their own target',async()=>{
  let evaluations=0,closed=0;const calls=[];
  const page={newTab:async url=>{calls.push(url);return 'owned';},setActivePage:async id=>assert.equal(id,'owned'),
    evaluate:async script=>{if(script.includes('function restoreOriginalLanguage'))return null;evaluations++;
      if(evaluations===1)return null;
      if(evaluations===2){assert.equal(script.endsWith(',true)'),true);return {expand_requested:true};}
      assert.equal(script.endsWith(',false)'),true);return {session:'private',data:rawProfile()};},
    hover:async selector=>assert.equal(selector,'[data-e2e="user-info"] [data-onecrew-douyin-bio-more="true"]'),
    sleep:async()=>{},closeTab:async id=>{assert.equal(id,'owned');closed++;}};
  const result=await readDouyinEvidence(page,'profile',author,undefined,withOwnedSearchPage);
  assert.equal(result.follower_count,4892);assert.deepEqual(calls,[profile]);assert.equal(closed,1);assert.equal(evaluations,3);
  page.evaluate=async script=>{if(script.includes('function restoreOriginalLanguage'))return null;throw new Error('ONECREW_DOUYIN_RISK');};
  await assert.rejects(readDouyinEvidence(page,'content',video,author,withOwnedSearchPage),/RISK/);assert.equal(closed,2);
});
test('profile expansion waits for the rendered full bio rather than publishing the collapsed email',async()=>{
  let reads=0,sleeps=0,hovers=0;
  const page={goto:async()=>{},hover:async selector=>{assert.equal(selector,'[data-e2e="user-info"] [data-onecrew-douyin-bio-more="true"]');hovers++;},sleep:async()=>{sleeps++;},evaluate:async()=>{
    reads++;
    if(reads===1)return {expand_requested:true};
    if(reads<4)return {...rawProfile(),bio:'mail@example...',bio_truncated:true,expansion_pending:true};
    return {...rawProfile(),bio:'mail@example.org',bio_truncated:false,expansion_pending:false};
  }};
  const result=await readDouyinEvidence(page,'profile',author,undefined,async(p,task)=>task(p));
  assert.equal(result.bio,'mail@example.org');assert.equal(reads,4);assert.equal(sleeps,3);
  assert.equal(hovers,1);
  assert.equal(result.expansion_pending,undefined);
});
test('unresponsive expansion remains bounded and explicitly truncated',async()=>{
  let reads=0,sleeps=0;
  const page={goto:async()=>{},hover:async()=>{},sleep:async()=>{sleeps++;},evaluate:async()=>++reads===1
    ? {expand_requested:true} : {...rawProfile(),bio:'mail@example...',bio_truncated:true,expansion_pending:true}};
  const result=await readDouyinEvidence(page,'profile',author,undefined,async(p,task)=>task(p));
  assert.equal(reads,12);assert.equal(sleeps,11);assert.equal(result.bio_truncated,true);
  assert.equal(result.expansion_pending,undefined);
});
