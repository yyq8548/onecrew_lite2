import test from 'node:test';
import assert from 'node:assert/strict';
import vm from 'node:vm';
import {facebookReelTarget,projectFacebookReelComments,readFacebookReelComments,readFacebookReelPage} from './facebook-reel-comments.mjs';
import {readFacebookComments} from './facebook-comments.mjs';
const source='https://www.facebook.com/reel/2067046843941874';
const data=()=>({author_id:'61590270912364',text:'Original shampoo post',comments:[
  {text:'Is this ammonia free?',source_url:`${source}/?comment_id=1427989936108468&__tn__=R`,commenter_id:'must-not-export'},
]});
test('Reel retains observed numeric owner and only comment language/provenance',()=>{
  const rows=projectFacebookReelComments(data(),source,5);
  assert.equal(rows[0].native_author_id,'61590270912364');
  assert.equal(rows[0].author_url,'https://www.facebook.com/profile.php?id=61590270912364');
  assert.deepEqual(rows[1],{type:'COMMENT',text:'Is this ammonia free?',text_truncated:false,source_url:`${source}?comment_id=1427989936108468`});
});
test('reject wrong post, ambiguous ID, replies and malformed owner',()=>{
  for(const mutate of [d=>d.author_id='Hair Dye Fast',d=>d.author_id='061590270912364',
    d=>d.comments[0].source_url=source.replace('2067046843941874','2067046843941875')+'?comment_id=123',
    d=>d.comments[0].source_url+='&comment_id=456',d=>d.comments[0].source_url+='&reply_comment_id=456',
    d=>d.comments[0].text='bad\0text']) {
    const d=data();mutate(d);assert.throws(()=>projectFacebookReelComments(d,source,5));
  }
});
test('no visible comments stays unavailable, never a successful empty read',()=>{
  const d=data();d.comments=[];
  assert.throws(()=>projectFacebookReelComments(d,source,5),/UNAVAILABLE/);
});
test('bounds and deduplicates text without splitting Unicode',()=>{
  const d=data();d.comments.push({...d.comments[0]},{text:'😀'.repeat(2001),source_url:`${source}?comment_id=123`});
  assert.equal(projectFacebookReelComments(d,source,1).length,2);
  const rows=projectFacebookReelComments(d,source,20);
  assert.equal(rows.length,3);assert.equal(Array.from(rows[2].text).length,2000);assert.equal(rows[2].text_truncated,true);
});
test('invalid targets never navigate',async()=>{
  for(const id of [source+'?comment_id=123',source+'#fragment',source.replace('2067046843941874','0'),source.replace('www.facebook.com','www.facebook.com.evil')]) {
    assert.throws(()=>facebookReelTarget(id));
    await assert.rejects(readFacebookReelComments(null,{id},()=>assert.fail('must not open')));
  }
});
test('existing comments command dispatches genuine Reel through owned page',async()=>{
  const calls=[];
  const page={goto:async url=>calls.push(url),evaluate:async script=>{assert.match(script,/readFacebookReelPage/);return {session:'owned',data:['result']};}};
  assert.deepEqual(await readFacebookComments(page,{id:source,limit:5},async(p,fn)=>fn(p)),['result']);
  assert.deepEqual(calls,[source]);
});

// DOM shape observed on the real Reel: DIV role=main, a current player,
// an offscreen preloaded player, and a separate complementary comment panel.
function pageFixture(sidebarAuthor='61590270912364') {
  const rect=(top=72)=>({width:600,height:900,top,bottom:top+900,left:0,right:600});
  const owner=id=>({getBoundingClientRect:()=>rect(),querySelectorAll:selector=>{
    assert.equal(selector,'h2 a[href]');return [{href:`https://www.facebook.com/profile.php?id=${id}&sk=reels_tab`}];
  }});
  const player=owner('61590270912364'),next=owner('999');next.getBoundingClientRect=()=>rect(2000);
  const body={innerText:'Original shampoo caption',querySelector:s=>s==='div'?{}:null,closest:()=>null};
  player.querySelectorAll=selector=>selector==='h2 a[href]'?[{href:'https://www.facebook.com/profile.php?id=61590270912364&sk=reels_tab'}]:
    selector==='span[dir="auto"]'?[body]:assert.fail(`content must not read comments: ${selector}`);
  const block={innerText:'Is this ammonia free?',querySelector:()=>null,closest:()=>null};
  const article={parentElement:{closest:()=>null},getClientRects:()=>[{}],querySelectorAll:s=>
    s==='div[dir="auto"]'?[block]:[{href:`${source}/?comment_id=123&__tn__=R`}]};
  const root=owner(sidebarAuthor);
  root.querySelectorAll=s=>s==='h2 a[href]'?[{href:`https://www.facebook.com/profile.php?id=${sidebarAuthor}&sk=reels_tab`}]:
    s==='span[dir="auto"]'?[body]:s==='[role="article"]'?[article]:assert.fail(`unexpected root selector ${s}`);
  const document={querySelectorAll:s=>s==='[role="main"] [role="group"][aria-label="Video player"]'?[player,next]:
    s==='[role="complementary"]'?[root]:assert.fail(`unexpected document selector ${s}`)};
  return {document,location:new URL(source),innerWidth:1600,innerHeight:1100,URL};
}
test('observed Reel DOM excludes preloaded next player and restores before extracting',async()=>{
  let restored=false;
  const run=vm.runInNewContext(`(${readFacebookReelPage.toString()})`,pageFixture());
  const rows=await run(source,5,(d,s,l)=>{assert.equal(restored,true);return projectFacebookReelComments(d,s,l);},async()=>{restored=true;});
  assert.equal(rows[0].native_author_id,'61590270912364');
  assert.equal(rows[1].text,'Is this ammonia free?');
});
test('sidebar ownership drift or failed original restoration prevents evidence export',async()=>{
  const project=()=>assert.fail('must not export');
  const drift=vm.runInNewContext(`(${readFacebookReelPage.toString()})`,pageFixture('999'));
  await assert.rejects(drift(source,5,project,async()=>true),/SOURCE_INVALID/);
  const translated=vm.runInNewContext(`(${readFacebookReelPage.toString()})`,pageFixture());
  await assert.rejects(translated(source,5,project,async()=>{throw new Error('ONECREW_ORIGINAL_TEXT_UNCONFIRMED');}),/ORIGINAL_TEXT_UNCONFIRMED/);
});
test('initial caption read needs no comments and does not open a comment panel',async()=>{
  const fixture=pageFixture();
  const original=fixture.document.querySelectorAll;
  fixture.document.querySelectorAll=s=>s==='[role="complementary"]'?[]:original(s);
  const run=vm.runInNewContext(`(${readFacebookReelPage.toString()})`,fixture);
  const rows=await run(source,1,projectFacebookReelComments,async()=>true,true);
  assert.equal(rows.length,1);assert.equal(rows[0].text,'Original shampoo caption');
  assert.equal(rows[0].native_author_id,'61590270912364');
});
