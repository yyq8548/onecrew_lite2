import test from 'node:test';
import assert from 'node:assert/strict';
import {withOwnedSearchPage,readSearchStop,normalizeBrowserRows,waitForFacebookSearch,waitForSearchDocument} from './owned-page.mjs';
import vm from 'node:vm';

function languageContext(translated = false) {
  const classes = new Set(translated ? ['translated-ltr'] : []);
  return {URL,location:new URL('https://example.test/'),setTimeout:callback=>setTimeout(callback,0),
    document:{body:{innerText:'original'},readyState:'complete',querySelectorAll:()=>[],
      documentElement:{classList:{contains:name=>classes.has(name),add:name=>classes.add(name)},setAttribute(){}}}};
}

test('new-tab loading finishes before native extraction observes document.body', async()=>{
  const document={body:null,readyState:'loading'}; let polls=0; const closed=[];
  const context={document,setTimeout:callback=>{polls++;document.body={};document.readyState='interactive';callback();}};
  const page={newTab:async()=> 'loading-target',setActivePage(){},sleep:async()=>{},closeTab:async id=>closed.push(id),
    evaluate:async script=>vm.runInNewContext(script,context)};
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
  const context=languageContext();
  let active; const page={newTab:async()=> 'owned',setActivePage:id=>{active=id;},closeTab:async()=>{},
    sleep:async()=>{active='other';},evaluate:async script=>{assert.equal(active,'owned');return vm.runInNewContext(script,context);}};
  await withOwnedSearchPage(page,async owned=>{
    await owned.goto('https://www.douyin.com/search/test');
    await owned.sleep(1);
    await owned.evaluate('document.body');
  });
});

test('rejected reused navigation is replaced by owned targets, including cleanup',async()=>{
  const events=[]; let serial=0; const context=languageContext();
  const page={goto:()=>{throw Error('Navigation rejected');},
    newTab:async url=>{events.push(['new',url]);return `owned-${++serial}`;},
    setActivePage:id=>events.push(['active',id]),
    closeTab:async id=>events.push(['close',id]),sleep:async()=>{},
    evaluate(script){assert.equal(this,page);return vm.runInNewContext(script,context);}};
  const result=await withOwnedSearchPage(page,async owned=>{
    await owned.goto('https://search.douban.com/book/subject_search?search_text=Python');
    await owned.goto('https://search.douban.com/music/subject_search?search_text=music');
    return owned.evaluate('"native result"');
  });
  assert.equal(result,'native result');
  assert.deepEqual(events.filter(e=>e[0]==='close'),[['close','owned-1'],['close','owned-2']]);
});
test('native auth failure is preserved and target is closed',async()=>{
  const closed=[];const failure=Error('AUTH_REQUIRED');const context=languageContext();
  const page={newTab:async()=> 'owned',setActivePage(){},sleep:async()=>{},evaluate:async script=>vm.runInNewContext(script,context),closeTab:async id=>closed.push(id)};
  await assert.rejects(withOwnedSearchPage(page,async owned=>{await owned.goto('https://www.linkedin.com/search');throw failure;}),e=>e===failure);
  assert.deepEqual(closed,['owned']);
});

test('translation must be restored before search reads and failure closes only the owned tab',async()=>{
  const context=languageContext(true);context.reads=0;const closed=[];
  const page={newTab:async()=> 'owned',setActivePage(){},sleep:async()=>{},
    evaluate:async script=>vm.runInNewContext(script,context),closeTab:async id=>closed.push(id)};
  await assert.rejects(withOwnedSearchPage(page,async owned=>{
    await owned.goto('https://example.test/');await owned.evaluate('(++globalThis.reads,document.body.innerText)');
  }),/ORIGINAL_TEXT_UNCONFIRMED/);
  assert.equal(context.reads,0); assert.deepEqual(closed,['owned']);
});

test('owned navigation preserves the native render settling budget',async()=>{
  const sleeps=[];const context=languageContext();context.document.body.innerText='No results found';
  const page={newTab:async()=> 'owned',setActivePage(){},evaluate:async script=>vm.runInNewContext(script,context),closeTab:async()=>{},sleep:async seconds=>sleeps.push(seconds)};
  await withOwnedSearchPage(page,async owned=>{
    await owned.goto('https://www.facebook.com/search/top?q=skincare',{settleMs:4000});
  });
  assert.deepEqual(sleeps,[4]);
});

test('one evaluation restores original text and preserves expression, function-string and function arguments',async()=>{
  const cases=[
    ['document.body.innerText',[],'original'],
    ['document.body.innerText // caller comment',[],'original'],
    ['(async()=>document.body.innerText)()',[],'original'],
    ['() => document.body.innerText',[],'original'],
    ['async () => document.body.innerText',[],'original'],
    ['function read(){return document.body.innerText;}',[],'original'],
    [function read(value){return document.body.innerText+':'+value;},[3],'original:3'],
    [async (value,options)=>document.body.innerText+':'+value+':'+options.limit,['arg',{limit:2}],'original:arg:2'],
  ];
  for(const [expression,args,expected] of cases){
    const context=languageContext();let evaluations=0;
    const page={evaluate:script=>{evaluations++;return vm.runInNewContext(script,context);}};
    assert.equal(await withOwnedSearchPage(page,owned=>owned.evaluate(expression,...args)),expected);
    assert.equal(evaluations,1);
    assert.equal(context.document.documentElement.classList.contains('notranslate'),true);
  }
});

test('native lexical blocks and statements retain completion values through their original two evaluations',async()=>{
  const cases=[
    '{\nconst value = "native";\n(()=>document.body.innerText+":"+value)()\n}',
    'const value = "native"; document.body.innerText+":"+value;',
    '(document.body.innerText+":native");',
  ];
  for(const expression of cases){
    const context=languageContext();let evaluations=0;
    const page={evaluate:script=>{evaluations++;return vm.runInNewContext(script,context);}};
    assert.equal(await withOwnedSearchPage(page,owned=>owned.evaluate(expression)),'original:native');
    assert.equal(evaluations,2);
  }
});

test('native string-argument rejection and statement syntax errors remain visible',async()=>{
  const context=languageContext();let evaluations=0;
  const page={evaluate:script=>{evaluations++;return vm.runInNewContext(script,context);}};
  await assert.rejects(withOwnedSearchPage(page,owned=>owned.evaluate('document.body.innerText','arg')),/string/);
  assert.equal(evaluations,0);
  await assert.rejects(withOwnedSearchPage(page,owned=>owned.evaluate('const =')),error=>error.name==='SyntaxError');
  assert.equal(evaluations,2);
});

test('combined evaluation waits for asynchronous original-text DOM changes before extraction',async()=>{
  const context=languageContext();context.document.body.innerText='translated';
  let pending=true,clicks=0,evaluations=0;
  context.document.querySelectorAll=()=>pending?[{textContent:'Show original',getClientRects:()=>[1],click:()=>{
    clicks++;setTimeout(()=>{context.document.body.innerText='original';pending=false;},0);
  }}]:[];
  const page={evaluate:script=>{evaluations++;return vm.runInNewContext(script,context);}};
  assert.equal(await withOwnedSearchPage(page,owned=>owned.evaluate('document.body.innerText')),'original');
  assert.equal(clicks,1);assert.equal(evaluations,1);
});

test('combined caller failure and cancellation still close the owned target and remove listeners',async()=>{
  for(const cancelled of [false,true]){
    const context=languageContext();const closed=[];const listeners=process.listenerCount('SIGTERM');
    context.cancel=()=>process.emit('SIGTERM');
    const page={newTab:async()=> 'owned',setActivePage(){},sleep:async()=>{},
      evaluate:script=>vm.runInNewContext(script,context),closeTab:async id=>closed.push(id)};
    await assert.rejects(withOwnedSearchPage(page,async owned=>{
      await owned.goto('https://example.test/');
      return owned.evaluate(cancelled?'(globalThis.cancel(),document.body.innerText)':'(()=>{throw new Error("READ_FAILED")})()');
    }),cancelled?/ONECREW_BROWSER_CANCEL_REQUESTED/:/READ_FAILED/);
    assert.deepEqual(closed,['owned']);assert.equal(process.listenerCount('SIGTERM'),listeners);
  }
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
