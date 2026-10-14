import test from 'node:test';
import assert from 'node:assert/strict';
import vm from 'node:vm';
import {projectInstagramSearch,fetchInstagramSearch,readInstagramSearch} from './instagram-search.mjs';
const fixture=()=>({users:[{user:{pk:'123456789',username:'Creator',full_name:'Name'}}]});

test('known private search accounts are skipped before the result limit',()=>{
  const payload={users:[
    {user:{pk:'1',username:'private_mouse',full_name:'Mouse review',is_private:true}},
    {user:{pk:'2',username:'public_mouse',full_name:'Mouse review',is_private:false}},
    {user:{pk:'3',username:'unknown_mouse',full_name:'Mouse review'}},
  ]};
  assert.deepEqual(projectInstagramSearch(payload,1).map(row=>row.author_id),['2']);
  assert.deepEqual(projectInstagramSearch(payload,3).map(row=>row.author_id),['2','3']);
});

test('search sends the exact query and rejects a response for a different query',async()=>{
  const query='鼠标测评';let requested;
  const context=vm.createContext({URL,AbortSignal,location:{origin:'https://www.instagram.com',pathname:'/'},fetch:async path=>{
    requested=new URL(path,'https://www.instagram.com');
    return {status:200,ok:true,url:'https://www.instagram.com/web/search/topsearch/?query=ping&context=user',json:async()=>fixture()};
  }});
  const result=await vm.runInContext(`(${fetchInstagramSearch.toString()})(${JSON.stringify(query)})`,context);
  assert.equal(requested.searchParams.get('query'),query);
  assert.equal(result.stop,'read');
});
test('search accepts the response bound to the exact Unicode query',async()=>{
  const query='鼠标测评';
  const context=vm.createContext({URL,AbortSignal,location:{origin:'https://www.instagram.com',pathname:'/'},fetch:async path=>({
    status:200,ok:true,url:new URL(path,'https://www.instagram.com').href,json:async()=>fixture(),
  })});
  const result=await vm.runInContext(`(${fetchInstagramSearch.toString()})(${JSON.stringify(query)})`,context);
  assert.equal(result.payload.users[0].user.pk,'123456789');
});
test('Instagram search keeps native account identity without inventing followers',()=>{
  const rows=projectInstagramSearch(fixture(),5);
  assert.deepEqual(rows,[{provider:'onecrew-instagram-native-v1',author_id:'123456789',username:'creator',name:'Name',url:'https://www.instagram.com/creator/'}]);
});
test('Instagram ambiguous or absent native identity cannot become an author binding',()=>{
  for(const change of [u=>delete u.pk,u=>u.id='999',u=>u.pk=Number.MAX_SAFE_INTEGER+1,u=>u.username='../evil']){
    const payload=fixture();change(payload.users[0].user);assert.throws(()=>projectInstagramSearch(payload,5));
  }
  assert.deepEqual(projectInstagramSearch({users:[]},5),[]);
});
test('Instagram auth and restrictions stop after one bounded request',async()=>{
  for(const status of [401,403,429]){
    let calls=0;
    const context=vm.createContext({URL,AbortSignal,location:{origin:'https://www.instagram.com',pathname:'/'},fetch:async()=>{
      calls++;return {status,ok:false,url:'https://www.instagram.com/web/search/topsearch/'};
    }});
    const result=await vm.runInContext(`(${fetchInstagramSearch.toString()})('shampoo')`,context);
    assert.equal(result.stop,status===401?'auth':'risk');assert.equal(calls,1);
  }
  await assert.rejects(readInstagramSearch(null,{query:'',limit:5},()=>assert.fail('must not navigate')));
});
