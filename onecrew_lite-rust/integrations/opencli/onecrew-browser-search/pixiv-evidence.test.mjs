import assert from 'node:assert/strict';
import test from 'node:test';
import {pixivId,projectPixivProfile,projectPixivContent,fetchPixivEvidence,readPixivEvidence} from './pixiv-evidence.mjs';
const profile=()=>({userId:'123',name:'Artist',comment:'Business art@example.com',links:['https://artist.example/contact'],following:15000});
const content=()=>({illustId:'456',userId:'123',title:'Original art',description:'Fashion illustration',truncated:false});
test('native profile keeps unknown followers and independent public contact links',()=>{
  const row=projectPixivProfile(profile(),'123');
  assert.equal(row.follower_count,null);assert.equal(row.bio,'Business art@example.com');
  assert.equal(row.profile_url,'https://www.pixiv.net/users/123');
  assert.deepEqual(row.public_links,['https://artist.example/contact']);assert.equal(row.following,undefined);
});
test('profile rejects wrong native owner and unsafe or oversized fields',()=>{
  for(const changes of [{userId:'456'},{userId:undefined},{name:''},{comment:'bad\0text'},
    {comment:'語'.repeat(3000)},{links:['javascript:alert(1)']},{links:['https://user:secret@artist.example/']},{links:Array(11).fill('https://artist.example/')}])
    assert.throws(()=>projectPixivProfile({...profile(),...changes},'123'),/INVALID/);
  for(const id of ['0','0123','Display Name','123?x=1'])assert.throws(()=>pixivId(id));
});
test('illustration requires actual native artwork and author IDs',()=>{
  const row=projectPixivContent(content(),'456','123');assert.equal(row.source_url,'https://www.pixiv.net/artworks/456');
  assert.equal(row.text,'Original art\nFashion illustration');
  for(const changes of [{illustId:'999'},{userId:'999'},{userId:undefined},{title:''},{description:'x'.repeat(15001)},{truncated:null}])
    assert.throws(()=>projectPixivContent({...content(),...changes},'456','123'),/INVALID/);
});
test('owned profile command makes one navigation and discards session wrapper',async()=>{
  let navigations=0;const page={goto:async url=>{assert.equal(url,'https://www.pixiv.net/users/123');navigations++;},evaluate:async()=>({session:'private',data:profile()})};
  assert.equal((await readPixivEvidence(page,'profile','123',null,async(p,task)=>task(p))).author_id,'123');
  assert.equal(navigations,1);
});
test('browser identity drift and captcha stop before any request',async()=>{
  const previous={location:globalThis.location,document:globalThis.document,fetch:globalThis.fetch};let calls=0;
  try{
    globalThis.fetch=async()=>{calls++;throw new Error('must not fetch');};
    globalThis.location={origin:'https://www.pixiv.net',href:'https://www.pixiv.net/users/999'};
    globalThis.document={querySelector:()=>null};
    assert.equal((await fetchPixivEvidence('profile','123','https://www.pixiv.net/users/123')).stop,'invalid');
    globalThis.location.href='https://www.pixiv.net/users/123';
    globalThis.document.querySelector=selector=>selector.startsWith('iframe')?{}:null;
    assert.equal((await fetchPixivEvidence('profile','123',globalThis.location.href)).stop,'risk');
    globalThis.document.querySelector=selector=>selector.startsWith('form')?{}:null;
    assert.equal((await fetchPixivEvidence('profile','123',globalThis.location.href)).stop,'auth');
    assert.equal(calls,0);
  }finally{Object.assign(globalThis,previous);}
});
