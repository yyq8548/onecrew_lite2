import test from 'node:test';
import assert from 'node:assert/strict';
import vm from 'node:vm';
import {projectInstagramRecent,fetchInstagramRecent,readInstagramRecent} from './instagram-recent.mjs';
const post=()=>({pk:'123456789012345678',id:'123456789012345678_123',code:'Code_1',user:{pk:'123',username:'Creator'},caption:{text:'Original caption',user:{pk:'123'}},product_type:'clips'});
test('Instagram original feed preserves native author and at most three distinct original captions',()=>{
  const items=Array.from({length:4},(_,i)=>({...post(),pk:String(100+i),id:`${100+i}_123`,code:`Code${i}`}));
  const rows=projectInstagramRecent({status:'ok',items},'creator','123');
  assert.equal(rows.length,3);
  assert.deepEqual(rows[0],{post_id:'100',author_id:'123',source_url:'https://www.instagram.com/reel/Code0/',text:'Original caption',text_truncated:false});
  assert.deepEqual(projectInstagramRecent({status:'ok',items:[]},'creator','123'),[]);
});
test('Instagram original caption truncates safely and absent caption remains unknown',()=>{
  const item=post();item.caption.text='日'.repeat(3000);
  const row=projectInstagramRecent({status:'ok',items:[item]},'creator','123')[0];
  assert.equal(Buffer.byteLength(row.text),7998);assert.equal(row.text_truncated,true);
  item.caption=null;assert.equal(projectInstagramRecent({status:'ok',items:[item]},'creator','123')[0].text,'');
});
test('Instagram owner drift, caption drift, ambiguous IDs and duplicate posts fail closed',()=>{
  for(const change of [p=>p.user.pk='456',p=>p.user.username='other',p=>p.caption.user.pk='456',p=>p.id='different',p=>p.pk=Number.MAX_SAFE_INTEGER+1,p=>p.code='../bad']){
    const item=post();change(item);assert.throws(()=>projectInstagramRecent({status:'ok',items:[item]},'creator','123'));
  }
  assert.throws(()=>projectInstagramRecent({status:'ok',items:[post(),post()]},'creator','123'));
  assert.throws(()=>projectInstagramRecent({status:'fail',items:[]},'creator','123'));
});
test('Instagram feed authentication, risk and timeout stop without retry; input rejected before navigation',async()=>{
  for(const status of [401,403,429]){
    let calls=0;
    const context=vm.createContext({URL,AbortSignal,location:{origin:'https://www.instagram.com',pathname:'/'},fetch:async()=>{
      calls++;return {status,ok:false,url:'https://www.instagram.com/api/v1/feed/user/creator/username/'};
    }});
    const result=await vm.runInContext(`(${fetchInstagramRecent.toString()})('creator')`,context);
    assert.equal(result.stop,status===401?'auth':'risk');assert.equal(calls,1);
  }
  const context=vm.createContext({URL,AbortSignal,location:{origin:'https://www.instagram.com',pathname:'/'},fetch:async()=>{throw {name:'TimeoutError'};}});
  assert.equal((await vm.runInContext(`(${fetchInstagramRecent.toString()})('creator')`,context)).stop,'timeout');
  await assert.rejects(readInstagramRecent(null,{username:'../bad',author_id:'123'},()=>assert.fail('must not navigate')));
});
test('Instagram recent uses upstream username feed once and rejects root redirects',async()=>{
  let calls=0;
  const payload={status:'ok',items:[post()]};
  const context=vm.createContext({URL,AbortSignal,location:{origin:'https://www.instagram.com',pathname:'/'},fetch:async(path)=>{
    calls++;assert.equal(path,'/api/v1/feed/user/creator/username/?count=3');
    return {status:200,ok:true,url:'https://www.instagram.com/api/v1/feed/user/creator/username/?count=3',json:async()=>payload};
  }});
  assert.equal((await vm.runInContext(`(${fetchInstagramRecent.toString()})('creator')`,context)).payload,payload);
  assert.equal(calls,1);
  context.fetch=async()=>({status:200,ok:true,url:'https://www.instagram.com/',json:()=>assert.fail('homepage must not be parsed as feed')});
  assert.equal((await vm.runInContext(`(${fetchInstagramRecent.toString()})('creator')`,context)).stop,'source_response_path_root');
});
