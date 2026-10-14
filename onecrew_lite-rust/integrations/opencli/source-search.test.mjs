import assert from 'node:assert/strict';
import {test} from 'node:test';
import vm from 'node:vm';
import {projectPosts,searchPosts} from './onecrew-substack/search.mjs';
import {searchCards,readCards} from './onecrew-source-search/search.mjs';

const post = {id:216810906,publication_id:28738,slug:'mo-lei-tau-1',
  canonical_url:'https://review.3ook.com/p/mo-lei-tau-1',title:'Review',publishedBylines:[{name:'Writer'}]};
test('Substack canonical custom domains retain native post provenance',()=>{
  assert.equal(projectPosts({results:[post]},5)[0].post_id,'216810906');
  for (const bad of [{id:null},{publication_id:0},{slug:'other'},
    {canonical_url:'https://user:secret@review.3ook.com/p/mo-lei-tau-1'},
    {canonical_url:'https://127.0.0.1/p/mo-lei-tau-1'},
    {canonical_url:'https://review.test/p/mo-lei-tau-1'}]) {
    assert.throws(()=>projectPosts({results:[{...post,...bad}]},5));
  }
  assert.throws(()=>projectPosts({},5));
  assert.deepEqual(projectPosts({results:[]},5),[]);
});
test('Substack only requests its fixed search endpoint and bounds malformed results',async()=>{
  let requested;
  const rows = await searchPosts('--profile=other',5,async(url,options)=>{
    requested=url; assert.equal(options.redirect,'error');
    return new Response(JSON.stringify({results:[post]}));
  });
  assert.equal(requested.origin,'https://substack.com');
  assert.equal(requested.searchParams.get('query'),'--profile=other');
  assert.equal(rows.length,1);
  await assert.rejects(searchPosts('review',5,async()=>new Response('error',{status:429})),/SUBSTACK_RISK/);
});
test('DOM reader refuses stale queries before inspecting cards',()=>{
  const result=vm.runInNewContext(`(${readCards.toString()})('nowcoder','current',5)`,{
    URL,location:{origin:'https://www.nowcoder.com',pathname:'/search/all',href:'https://www.nowcoder.com/search/all?query=previous'},
  });
  assert.equal(result.status,'wrong_page');
});
test('Nowcoder visible card binds one native author and rejects ambiguous authors',()=>{
  const link=(href,text)=>({getAttribute:()=>href,textContent:text,getClientRects:()=>[{}]});
  const authors=[link('/users/123','Writer')];
  const card={querySelectorAll:()=>authors};
  const article={...link('/discuss/374857292930441216?sourceSSR=search','Phone stand review'),closest:()=>card};
  const context={URL,location:{origin:'https://www.nowcoder.com',pathname:'/search/all',
    href:'https://www.nowcoder.com/search/all?query=current'},getComputedStyle:()=>({visibility:'visible'}),
    document:{body:{innerText:'Phone stand review'},querySelectorAll:selector=>selector.startsWith('a[href')?[article]:
      selector.startsWith('input[')?[{value:'current',getClientRects:()=>[{}]}]:[]}};
  let result=vm.runInNewContext(`(${readCards.toString()})('nowcoder','current',5)`,context);
  assert.equal(result.rows[0].url,'https://www.nowcoder.com/discuss/374857292930441216');
  assert.equal(result.rows[0].author_url,'https://www.nowcoder.com/users/123');
  authors.push(link('/users/456','Other writer'));
  result=vm.runInNewContext(`(${readCards.toString()})('nowcoder','current',5)`,context);
  assert.equal(result.status,'invalid_author');assert.equal(result.rows.length,0);
  context.document.body.innerText='请求太频繁';
  result=vm.runInNewContext(`(${readCards.toString()})('nowcoder','current',5)`,context);
  assert.equal(result.status,'risk');
});
test('Nowcoder login-main wall stops before recommendation cards or stale search input',()=>{
  const context={URL,location:{origin:'https://www.nowcoder.com',pathname:'/search/all',href:'https://www.nowcoder.com/search/all?query=current'},
    getComputedStyle:()=>({visibility:'visible'}),document:{body:{innerText:'注册/登录'},
      querySelectorAll:selector=>selector.includes('.login-main')?[{innerText:'注册/登录',getClientRects:()=>[{}]}]:[]}};
  assert.equal(vm.runInNewContext(`(${readCards.toString()})('nowcoder','current',5)`,context).status,'auth');
  context.document.querySelectorAll=selector=>selector.startsWith('input[')?[{value:'recommended',getClientRects:()=>[{}]}]:[];
  assert.equal(vm.runInNewContext(`(${readCards.toString()})('nowcoder','current',5)`,context).status,'wrong_page');
});
test('search opens one owned tab, waits for hydration, cleans only that tab',async()=>{
  const calls=[];let reads=0;
  const page={newTab:async url=>{calls.push(url);return 'owned';},setActivePage:async id=>calls.push(id),
    sleep:async()=>{},evaluate:async code=>code.includes('async function restoreOriginalLanguage()')?true:++reads===1?{status:'wrong_page',rows:[]}:
      {status:'content',rows:[{title:'Review',url:'https://www.nowcoder.com/discuss/123'}]},
    closeTab:async id=>calls.push(id),goto:()=>assert.fail('must not reuse old target')};
  const rows=await searchCards(page,'nowcoder','phone stand',5);
  assert.equal(rows.length,1);assert.equal(reads,2);
  assert.equal(calls[0],'https://www.nowcoder.com/search/all?type=all&query=phone%20stand');
  assert.deepEqual(calls.slice(1),['owned','owned']);
});
test('login and risk walls remain failures; timeout never becomes empty success',async()=>{
  for (const status of ['auth','risk','invalid_author','loading']) {
    let closed=false,reads=0;
    const page={newTab:async()=> 'owned',setActivePage:async()=>{},sleep:async()=>{},
      evaluate:async code=>{if(code.includes('async function restoreOriginalLanguage()'))return true;reads++;return {status,rows:[]};},closeTab:async()=>{closed=true;}};
    await assert.rejects(searchCards(page,'nowcoder','phone stand',5),new RegExp(status==='loading'?'SOURCE_TIMEOUT':`SOURCE_${status.toUpperCase()}`));
    assert.equal(closed,true);assert.equal(reads,status==='loading'?12:1);
  }
});
