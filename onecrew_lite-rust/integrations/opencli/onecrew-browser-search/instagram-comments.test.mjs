import test from 'node:test';
import assert from 'node:assert/strict';
import {instagramPostTarget, projectInstagramComments, readInstagramComments, readInstagramCommentPage} from './instagram-comments.mjs';
import vm from 'node:vm';
const source='https://www.instagram.com/creator/reel/Abc_123/';
const data=()=>({author_url:'/creator/',caption:'作者原文',comments:[{author_url:'/reader/',text:'评论原文',source_url:'/p/Abc_123/c/123456789/'}]});
test('Instagram input allows only exact post or reel sources',()=>{
  assert.deepEqual(instagramPostTarget(source),{username:'creator',code:'Abc_123'});
  assert.equal(instagramPostTarget('https://www.instagram.com/p/Abc_123/').username,null);
  for(const invalid of [source+'?x=1',source+'more','https://evil.example/p/Abc_123/','https://www.instagram.com/direct/123/'])
    assert.throws(()=>instagramPostTarget(invalid),/INPUT/);
});
test('Instagram preserves real comment locators without fabricating stable account IDs',()=>{
  const rows=projectInstagramComments(data(),source,'Abc_123','creator',5);
  assert.equal(rows[1].id,'123456789'); assert.ok(!Object.hasOwn(rows[1],'native_author_id'));
  assert.equal(rows[1].source_url,'https://www.instagram.com/p/Abc_123/c/123456789/');
  assert.equal(rows[0].text,'作者原文');
  assert.throws(()=>projectInstagramComments(data(),source,'other','creator',5),/INVALID/);
  assert.throws(()=>projectInstagramComments(data(),source,'Abc_123','other',5),/INVALID/);
});
test('Instagram rejects unknown sources, duplicates and text-less rows',()=>{
  for(const changes of [{source_url:'https://evil.example/p/Abc_123/c/123/'},{text:undefined}]){
    const d=data(); Object.assign(d.comments[0],changes);
    assert.throws(()=>projectInstagramComments(d,source,'Abc_123','creator',5),/INVALID/);
  }
  const d=data(); d.comments.push({...d.comments[0]});
  assert.throws(()=>projectInstagramComments(d,source,'Abc_123','creator',5),/INVALID/);
  d.comments[1].source_url='/p/Abc_123/c/222/'; d.comments[0].text='😀'.repeat(2001);
  const rows=projectInstagramComments(d,source,'Abc_123','creator',1);
  assert.equal(rows.length,2); assert.equal(Array.from(rows[1].text).length,2000); assert.equal(rows[1].text_truncated,true);
});
test('Instagram bad target never accesses browser',async()=>{
  await assert.rejects(readInstagramComments({}, {id:'https://www.instagram.com/direct/'},()=>assert.fail('browser')),/INPUT/);
});

test('Instagram standalone layout binds each timestamp to its own text and ignores other post links',async()=>{
  const makeTime=(author,text,href)=>{
    const time={closest:()=>href?{getAttribute:()=>href}:null};
    const authorNode={getAttribute:()=>author};
    const header={contains:node=>node===time,querySelectorAll:()=>[authorNode]};
    const body={contains:()=>false,innerText:text};
    time.parentElement={children:[header,body]};
    return time;
  };
  const times=[makeTime('/creator/','作者原文'),makeTime('/reader/','评论','/p/Abc_123/c/123/'),
    makeTime('/other/','不应读取','/p/Other/c/456/')];
  let restored=0;
  const context={document:{querySelectorAll:()=>[],querySelector:()=>({querySelectorAll:()=>times})},
    location:{origin:'https://www.instagram.com',pathname:'/creator/reel/Abc_123/'},
    restore:async()=>{restored++;},project:projectInstagramComments,source};
  const rows=await vm.runInNewContext(`(${readInstagramCommentPage.toString()})(source,'Abc_123','creator',5,project,restore)`,context);
  assert.equal(rows.length,2);assert.equal(rows[1].text,'评论');assert.equal(restored,1);
  context.restore=async()=>{throw Error('ONECREW_ORIGINAL_TEXT_UNCONFIRMED');};
  await assert.rejects(vm.runInNewContext(`(${readInstagramCommentPage.toString()})(source,'Abc_123','creator',5,project,restore)`,context),/ORIGINAL_TEXT/);
});
