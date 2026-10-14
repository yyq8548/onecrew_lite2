import test from 'node:test';
import assert from 'node:assert/strict';
import vm from 'node:vm';
import {projectFacebookRecent,readFacebookRecentPage,readFacebookRecent} from './facebook-recent.mjs';
const id='61590270912364',base=`https://www.facebook.com/profile.php?id=${id}`,source=`${base}&sk=reels_tab`;
const reel=n=>`https://www.facebook.com/reel/${n}`;
test('recent links are bounded and must retain exact requested profile identity',()=>{
  assert.deepEqual(projectFacebookRecent({author_id:id,sources:[reel('123')]},id),[{author_id:id,source_url:reel('123')}]);
  for(const data of [{author_id:'999',sources:[reel('123')]},{author_id:id,sources:[]},
    {author_id:id,sources:[reel('123'),reel('123')]},{author_id:id,sources:[reel('0')]},
    {author_id:id,sources:[reel('123')+'?redirect=evil']},{author_id:id,sources:['https://evil.test/reel/123']}])
    assert.throws(()=>projectFacebookRecent(data,id));
});
test('observed numeric profile list drops tracking and navigation decoys, deduplicates and stops at three',async()=>{
  const anchor=href=>({href,getClientRects:()=>[{}],getAttribute:()=>null});
  const links=[anchor(`${base}&sk=followers`),anchor('https://www.facebook.com/reel/?s=tab'),
    ...['2067046843941874','2067046843941874','1657088309273782','1805129463825611','1391377773203877'].map(n=>anchor(`${reel(n)}/?s=fb_shorts_profile&stack_idx=0`))];
  const main={getClientRects:()=>[{}],querySelectorAll:()=>links};
  let restored=false;
  const run=vm.runInNewContext(`(${readFacebookRecentPage.toString()})`,{location:new URL(source),document:{querySelectorAll:()=>[main]},URL});
  const data=await run(source,id,async()=>{restored=true;});
  assert.equal(restored,true);
  assert.deepEqual(Array.from(data.sources),['2067046843941874','1657088309273782','1805129463825611'].map(reel));
});
test('wrong source or failed original restoration never returns recent links',async()=>{
  const run=vm.runInNewContext(`(${readFacebookRecentPage.toString()})`,{location:new URL(base),document:{querySelectorAll:()=>assert.fail('must not read wrong page')},URL});
  await assert.rejects(run(source,id,async()=>true),/RECENT_INVALID/);
  const owner={href:`${base}&sk=followers`,getClientRects:()=>[{}],getAttribute:()=>null};
  const main={getClientRects:()=>[{}],querySelectorAll:()=>[owner]};
  const translated=vm.runInNewContext(`(${readFacebookRecentPage.toString()})`,{location:new URL(source),document:{querySelectorAll:()=>[main]},URL});
  await assert.rejects(translated(source,id,async()=>{throw new Error('ONECREW_ORIGINAL_TEXT_UNCONFIRMED');}),/ORIGINAL_TEXT_UNCONFIRMED/);
  await assert.rejects(readFacebookRecent(null,{id:'name'},()=>assert.fail('must not open')));
});
