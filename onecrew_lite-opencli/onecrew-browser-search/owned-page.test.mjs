import test from 'node:test';
import assert from 'node:assert/strict';
import {withOwnedSearchPage,readSearchStop,normalizeBrowserRows,waitForFacebookSearch,waitForSearchDocument} from './owned-page.mjs';
import vm from 'node:vm';

test('new-tab loading finishes before native extraction observes document.body', async()=>{
  const document={body:null,readyState:'loading'}; let polls=0; const closed=[];
  const context={document,setTimeout:callback=>{polls++;document.body={};document.readyState='interactive';callback();}};
  const page={newTab:async()=> 'loading-target',setActivePage(){},sleep:async()=>{},closeTab:async id=>closed.push(id),
    evaluate:async script=>script?.includes('restoreOriginalLanguage') ? true : vm.runInNewContext(script,context)};
  await withOwnedSearchPage(page,async owned=>{
    await owned.goto('https://www.douyin.com/search/test?type=video');
    assert.ok(document.body,'native MutationObserver requires document.body');
  });
  assert.equal(polls,1);
  assert.deepEqual(closed,['loading-target']);
});

test('document readiness has a bounded timeout instead of reading a blank page', async()=>{
  let waits=0;
  await assert.rejects(vm.runInNewContext(`(${waitForSearchDocument.toString()})()`,{
    document:{body:null,readyState:'loading'},setTimeout:callback=>{waits++;callback();},
  }),/ONECREW_SEARCH_DOCUMENT_TIMEOUT/);
  assert.equal(waits,39);
});

test('every extraction rebinds the owned target after an intervening operation', async()=>{
  let active; const page={newTab:async()=> 'owned',setActivePage:id=>{active=id;},closeTab:async()=>{},
    sleep:async()=>{active='other';},evaluate:async()=>{assert.equal(active,'owned');return true;}};
  await withOwnedSearchPage(page,async owned=>{
    await owned.goto('https://www.douyin.com/search/test');
    await owned.sleep(1);
    await owned.evaluate('document.body');
  });
});

test('rejected reused navigation is replaced by owned targets, including cleanup',async()=>{
  const events=[]; let serial=0;
  const page={goto:()=>{throw Error('Navigation rejected');},
    newTab:async url=>{events.push(['new',url]);return `owned-${++serial}`;},
    setActivePage:id=>events.push(['active',id]),
    closeTab:async id=>events.push(['close',id]),sleep:async()=>{},
    evaluate(){return this===page?'native result':'wrong receiver';}};
  const result=await withOwnedSearchPage(page,async owned=>{
    await owned.goto('https://search.douban.com/book/subject_search?search_text=Python');
    await owned.goto('https://search.douban.com/music/subject_search?search_text=music');
    return owned.evaluate();
  });
  assert.equal(result,'native result');
  assert.deepEqual(events.filter(e=>e[0]==='close'),[['close','owned-1'],['close','owned-2']]);
});
test('native auth failure is preserved and target is closed',async()=>{
  const closed=[];const failure=Error('AUTH_REQUIRED');
  const page={newTab:async()=> 'owned',setActivePage(){},sleep:async()=>{},evaluate:async()=>true,closeTab:async id=>closed.push(id)};
  await assert.rejects(withOwnedSearchPage(page,async owned=>{await owned.goto('https://www.linkedin.com/search');throw failure;}),e=>e===failure);
  assert.deepEqual(closed,['owned']);
});

test('translation must be restored before search reads and failure closes only the owned tab',async()=>{
  let reads=0; const closed=[];
  const page={newTab:async()=> 'owned',setActivePage(){},sleep:async()=>{},
    evaluate:async()=>{throw Error('ONECREW_ORIGINAL_TEXT_UNCONFIRMED');},closeTab:async id=>closed.push(id)};
  await assert.rejects(withOwnedSearchPage(page,async owned=>{
    await owned.goto('https://x.com/home'); reads++;
  }),/ORIGINAL_TEXT_UNCONFIRMED/);
  assert.equal(reads,0); assert.deepEqual(closed,['owned']);
});

test('owned navigation preserves the native render settling budget',async()=>{
  const sleeps=[];
  const page={newTab:async()=> 'owned',setActivePage(){},evaluate:async()=>true,closeTab:async()=>{},sleep:async seconds=>sleeps.push(seconds)};
  await withOwnedSearchPage(page,async owned=>{
    await owned.goto('https://www.facebook.com/search/top?q=skincare',{settleMs:4000});
  });
  assert.deepEqual(sleeps,[4]);
});
test('visible bot verification is a risk stop, while unknown empty pages stay unknown',()=>{
  const context={location:{hostname:'search.brave.com',pathname:'/search'},document:{body:{innerText:"Verifying you're not a bot"},querySelectorAll:()=>[]}};
  assert.equal(vm.runInNewContext(`(${readSearchStop.toString()})()`,context),'risk');
  context.document.body.innerText='Not Found';
  assert.equal(vm.runInNewContext(`(${readSearchStop.toString()})()`,context),'unknown');
  context.document.body.innerText='我们需要确认您是人类 在继续操作之前，请完成安全检查。';
  assert.equal(vm.runInNewContext(`(${readSearchStop.toString()})()`,context),'risk');
  context.document.body.innerText='Not Found';
  context.location={hostname:'passport.weibo.com',pathname:'/visitor/visitor'};
  assert.equal(vm.runInNewContext(`(${readSearchStop.toString()})()`,context),'loading_auth');
  context.location.pathname='/sso/signin';
  assert.equal(vm.runInNewContext(`(${readSearchStop.toString()})()`,context),'auth');
});

test('Google questions and opaque ads do not poison real source rows or exceed the limit',()=>{
  const result={type:'result',title:'Creator',url:'https://example.com/creator'};
  const rows=[{type:'paa',title:'Question',url:''},{type:'snippet',title:'Summary',url:''},
    {type:'result',title:'Ad',url:'https://www.google.com/goto?url=opaque'},result,result];
  assert.deepEqual(normalizeBrowserRows('google',rows,1),[result]);
  const invalid={type:'result',title:'Malformed',url:''};
  assert.deepEqual(normalizeBrowserRows('google',[invalid],5),[invalid]);
  assert.deepEqual(normalizeBrowserRows('dribbble',rows,1),rows);
});

test('Facebook waits through a skeleton, ignores search decoys, and stops at a visible result', async()=>{
  let polls=0, sleeps=0;
  const context={URL,location:{pathname:'/search/top'},document:{body:{innerText:''},
    querySelectorAll:()=>{polls++; return [{href:polls<3?'https://www.facebook.com/search/top?q=a':'https://www.facebook.com/misaludcorporal',textContent:'Saludable',getClientRects:()=>[{}]}];}},
    setTimeout:callback=>{sleeps++;callback();}};
  await vm.runInNewContext(`(${waitForFacebookSearch.toString()})()`,context);
  assert.equal(polls,3); assert.equal(sleeps,2);
});

test('Facebook waiting is bounded and does not wait through login or a challenge',async()=>{
  let sleeps=0;
  const context={URL,location:{pathname:'/search/top'},document:{body:{innerText:''},querySelectorAll:()=>[]},setTimeout:callback=>{sleeps++;callback();}};
  await vm.runInNewContext(`(${waitForFacebookSearch.toString()})()`,context);
  assert.equal(sleeps,23);
  for(const [path,text] of [['/checkpoint/',''],['/search/top','verify you are human'],['/search/top','No results found']]){
    sleeps=0;context.location.pathname=path;context.document.body.innerText=text;
    await vm.runInNewContext(`(${waitForFacebookSearch.toString()})()`,context);
    assert.equal(sleeps,0);
  }
});
