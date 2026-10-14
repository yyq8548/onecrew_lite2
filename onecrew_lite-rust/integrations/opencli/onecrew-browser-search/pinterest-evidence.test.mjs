import test from 'node:test';
import assert from 'node:assert/strict';
import vm from 'node:vm';
import {projectProfile,projectSearch,readProfile,readSearch,fetchNativeResource,projectPinTarget,projectPinContent,readContent} from './pinterest-evidence.mjs';
const profile = {type:'user', id:'9876543210123456789', username:'native_author', full_name:'Display Name', about:'Business: studio@example.org', domain_url:'https://studio.example.org/contact'};
test('native identity and public contacts stay source-separated', () => {
  const [row] = projectProfile(profile, 'native_author');
  assert.equal(row.author_id, profile.id);
  assert.equal(row.username, profile.username);
  assert.equal(row.author_label, profile.full_name);
  assert.equal(row.bio, profile.about);
  assert.deepEqual(row.public_links, [profile.domain_url]);
  assert.equal(row.follower_count, null);
});
test('owned reader performs one exact profile request and never retries a stop', async () => {
  let navigation = 0, reads = 0, cleaned = 0;
  const own = async (page, task) => {try {return await task(page);} finally {cleaned++;}};
  const page = {goto:async url => {navigation++; assert.equal(url, 'https://www.pinterest.com/native_author/');},
    evaluate:async script => {reads++; assert.ok(script.endsWith('("profile","native_author")')); return {session:'owned',data:profile};}};
  assert.equal((await readProfile(page,profile.username,own))[0].author_id,profile.id);
  assert.deepEqual([navigation,reads,cleaned],[1,1,1]);
  page.evaluate = async () => {reads++; return {stop:'risk'};};
  await assert.rejects(readProfile(page,profile.username,own), /ONECREW_PINTEREST_RISK/);
  assert.deepEqual([navigation,reads,cleaned],[2,2,2]);
  await assert.rejects(readProfile(page,'bad/name',own));
  assert.deepEqual([navigation,reads,cleaned],[2,2,2]);
});
test('native fetch is fixed read-only resource and stops auth or risk once', async () => {
  let calls = 0;
  const context = {location:{origin:'https://www.pinterest.com',pathname:'/native_author/'},document:{body:{innerText:''},cookie:'csrftoken=fixture'},
    URLSearchParams,AbortSignal,Uint8Array,TextDecoder,
    fetch:async(path,options) => {
      calls++; assert.equal(path,'/resource/UserResource/get/'); assert.equal(options.method,'POST'); assert.equal(options.redirect,'error');
      const body = new URLSearchParams(options.body);
      assert.equal(body.get('source_url'),'/native_author/');
      assert.deepEqual(JSON.parse(body.get('data')),{options:{username:'native_author',field_set_key:'profile'},context:{}});
      return new Response(JSON.stringify({resource_response:{data:profile}}));
    }};
  const run = () => vm.runInNewContext(`(${fetchNativeResource.toString()})("profile","native_author")`,context);
  assert.equal((await run()).id,profile.id); assert.equal(calls,1);
  for (const [status,stop] of [[401,'auth'],[403,'risk'],[429,'risk']]) {
    context.fetch = async () => {calls++; return new Response('',{status});};
    assert.equal((await run()).stop,stop);
  }
  assert.equal(calls,4);
  context.document.body.innerText = 'Verify you are human';
  assert.equal((await run()).stop,'risk'); assert.equal(calls,4);
  context.document.body.innerText = ''; context.document.cookie = '';
  assert.equal((await run()).stop,'read'); assert.equal(calls,4);
  context.document.cookie='csrftoken=fixture';
  for (const [body,status,stage] of [['not JSON',200,'response_encoding'],['',500,'response_http'],['x'.repeat(200001),200,'response_size'],[JSON.stringify({resource_response:{error:'private provider detail'}}),200,'resource']]) {
    context.fetch=async()=>{calls++;return new Response(body,{status});};
    assert.deepEqual(JSON.parse(JSON.stringify(await run())),{stop:'read',stage});
  }
  assert.equal(calls,8);
});
test('search preserves native IDs and reads one page without inferring from names', async () => {
  const rows = projectSearch({results:[{...profile},{type:'pin',id:'123'}, {...profile,id:'42',username:'second'}]},2);
  assert.equal(rows.length,2); assert.equal(rows[0].author_id,profile.id);
  assert.equal(rows[0].source_url,rows[0].author_url);
  assert.throws(() => projectSearch([{...profile,id:undefined}],1));
  assert.throws(() => projectSearch([{...profile,id:profile.full_name}],1));
  let calls = 0;
  const page = {goto:async url => {calls++; assert.equal(url,'https://www.pinterest.com/search/users/?q=keyboard');},
    evaluate:async () => {calls++; return [profile];}};
  const result = await readSearch(page,'keyboard',1,async(p,task) => task(p));
  assert.equal(result[0].author_id,profile.id); assert.equal(calls,2);
});
test('native search fetch binds the exact query and never pages or accepts another resource', async () => {
  let calls = 0;
  const context = {location:{origin:'https://www.pinterest.com',pathname:'/search/users/',search:'?q=keyboard'},
    document:{body:{innerText:''},cookie:'csrftoken=fixture'},URLSearchParams,AbortSignal,Uint8Array,TextDecoder,
    fetch:async(path,options) => {
      calls++; assert.equal(path,'/resource/BaseSearchResource/get/');
      const body = new URLSearchParams(options.body);
      assert.equal(body.get('source_url'),'/search/users/?q=keyboard');
      assert.deepEqual(JSON.parse(body.get('data')),{options:{query:'keyboard',scope:'users',page_size:20},context:{}});
      return new Response(JSON.stringify({resource_response:{data:{results:[profile]}}}));
    }};
  const run = (kind,query) => vm.runInNewContext(`(${fetchNativeResource.toString()})(${JSON.stringify(kind)},${JSON.stringify(query)})`,context);
  assert.equal((await run('search','keyboard')).results[0].id,profile.id); assert.equal(calls,1);
  assert.equal((await run('search','mouse')).stop,'read'); assert.equal(calls,1);
  assert.equal((await run('delete','keyboard')).stop,'read'); assert.equal(calls,1);
});
test('unknown followers stay unknown while actual zero remains zero', () => {
  for (const count of [undefined, null, 0, 12000]) {
    assert.equal(projectProfile({...profile, follower_count:count}, profile.username)[0].follower_count, count ?? null);
  }
  for (const count of ['12000', -1, 1.5, Number.MAX_SAFE_INTEGER + 1]) {
    assert.throws(() => projectProfile({...profile, follower_count:count}, profile.username));
  }
});
test('display name, unsafe numeric IDs and wrong username cannot certify author', () => {
  for (const patch of [{id:profile.full_name}, {id:Number(profile.id)}, {username:'other'}, {type:'pin'}, {id:'0'}, {id:'1&other=2'}]) {
    assert.throws(() => projectProfile({...profile, ...patch}, profile.username));
  }
});
test('private or malformed website and overflowing original text are rejected', () => {
  for (const website of ['https://127.0.0.1/', 'https://172.16.0.1/', 'https://0x7f000001/', 'https://[::1]/', 'https://user:secret@example.org/', 'http://127.0.0.1/', 'https://internal.local/', 'https://example.org:444/', 'not a URL']) {
    assert.throws(() => projectProfile({...profile, domain_url:website}, profile.username));
  }
  assert.throws(() => projectProfile({...profile, about:'日'.repeat(2667)}, profile.username));
  assert.throws(() => projectProfile({...profile, about:'original\0invalid'}, profile.username));
});
test('profile rejection preserves fixed field categories without carrying field values', () => {
  for(const [patch,stage] of [[{username:'other'},'OWNER'],[{id:'private raw ID'},'ID'],[{follower_count:'1.2k'},'FOLLOWERS'],
    [{about:'日'.repeat(2667)},'BIO'],[{full_name:'x'.repeat(513)},'NAME'],[{domain_url:'ftp://public.example.org/'},'WEBSITE_PROTOCOL'],
    [{domain_url:'not a URL'},'WEBSITE_FORMAT'],[{domain_url:false},'WEBSITE_TYPE'],[{domain_url:'https://127.0.0.1/'},'WEBSITE_PRIVATE']]) {
    assert.throws(()=>projectProfile({...profile,...patch},profile.username),error=>error.message===`ONECREW_PINTEREST_INVALID_PROFILE_${stage}`);
  }
});
test('unsupported public HTTP website does not discard valid native identity or upgrade the URL',()=>{
  const [row]=projectProfile({...profile,domain_url:'http://public.example.org/'},profile.username);
  assert.equal(row.author_id,profile.id);assert.equal(row.bio,profile.about);
  assert.deepEqual(row.public_links,[]);
  for(const website of ['http://127.0.0.1/','http://user:private@example.org/','http://internal.local/','http://example.org:444/']) {
    assert.throws(()=>projectProfile({...profile,domain_url:website},profile.username));
  }
});
test('published bare website host preserves identity without manufacturing a URL or accepting private locators',()=>{
  for(const domain_url of ['www.public.example.org','public.example.org/']) {
    const [row]=projectProfile({...profile,domain_url},profile.username);
    assert.equal(row.author_id,profile.id);assert.equal(row.bio,profile.about);assert.deepEqual(row.public_links,[]);
  }
  for(const domain_url of ['not a URL','127.0.0.1','localhost','private.internal','user@public.example.org','public.example.org:444','public.example.org?access_token=private']) {
    assert.throws(()=>projectProfile({...profile,domain_url},profile.username));
  }
});

const pin = {type:'pin',id:'456',pinner:{id:profile.id,username:profile.username},title:'Keyboard review',description:'English original caption. Business studio@example.org'};
test('captionless pins do not discard usable search evidence from the same bounded page', async () => {
  const {projectPinSearch}=await import('./pinterest-evidence.mjs');
  const empty={...pin,id:'123',title:'',description:' '};
  assert.equal(projectPinSearch([empty,pin],3).length,1);
  assert.deepEqual(projectPinSearch([empty],3),[]);
  assert.deepEqual(projectPinSearch([...Array(20).fill(empty),pin],3),[]);
});

test('a reserved search Pin is read directly instead of selecting a different recent Pin', async () => {
  const content=projectPinContent(pin,'456',profile.id,profile.username);
  const visits=[];let reads=0;
  const page={goto:async url=>visits.push(url),evaluate:async()=>{reads++;return content;}};
  assert.deepEqual(await readContent(page,profile.id,profile.username,async(p,task)=>task(p),'456'),[content]);
  assert.deepEqual(visits,['https://www.pinterest.com/pin/456/']);assert.equal(reads,1);
  page.evaluate=async()=>({...content,pin_id:'789',source_url:'https://www.pinterest.com/pin/789/'});
  await assert.rejects(readContent(page,profile.id,profile.username,async(p,task)=>task(p),'456'),/INVALID/);
});

test('pin search preserves the content source separately from its observed publishing account', async () => {
  const {projectPinSearch}=await import('./pinterest-evidence.mjs');
  const [row]=projectPinSearch({results:[pin]},3);
  assert.equal(row.source_url,'https://www.pinterest.com/pin/456/');
  assert.equal(row.author_url,`https://www.pinterest.com/${profile.username}/`);
  assert.equal(row.author_id,profile.id);
  assert.equal(row.title,pin.title+'\n'+pin.description);
});

test('long Pin headings retain same-Pin caption evidence without dropping sales language', async () => {
  const {projectPinSearch}=await import('./pinterest-evidence.mjs');
  const [row]=projectPinSearch([{...pin,title:'Mouse '.repeat(80),description:'Buy now. I tested click latency in CS2.'}],3);
  assert.ok(row.title.includes('Buy now. I tested click latency in CS2.'));
  assert.ok(Array.from(row.title).length<=500);
  assert.equal(row.source_url,'https://www.pinterest.com/pin/456/');
});

test('pin search never treats user-name matches, ads or repins as original content', async () => {
  const {projectPinSearch}=await import('./pinterest-evidence.mjs');
  const discarded=[{...pin,type:'user'},{...pin,is_promoted:true},{...pin,promoted_is_ad:true},{...pin,is_repin:true}];
  assert.deepEqual(projectPinSearch(discarded,3),[]);
  assert.equal(projectPinSearch([...discarded,pin,pin],3).length,1);
  for(const bad of [{...pin,pinner:{}},{...pin,id:'../456'},{...pin,description:{text:'wrong shape'}}])
    assert.throws(()=>projectPinSearch([bad],3),/INVALID/);
  assert.throws(()=>projectPinSearch([pin,{...pin,pinner:{...pin.pinner,id:'999'}}],3),/INVALID/);
});

test('content-search reader requests one bounded native pin page without user fallback', async () => {
  const {readPinSearch}=await import('./pinterest-evidence.mjs');
  let calls=0;
  const context={location:{origin:'https://www.pinterest.com',pathname:'/search/pins/',search:'?q=gaming%20mouse'},
    document:{body:{innerText:''},cookie:'csrftoken=fixture'},URLSearchParams,AbortSignal,Uint8Array,TextDecoder,
    fetch:async(path,options)=>{
      calls++;assert.equal(path,'/resource/BaseSearchResource/get/');
      const body=new URLSearchParams(options.body);
      assert.equal(body.get('source_url'),'/search/pins/?q=gaming%20mouse');
      const args=JSON.parse(body.get('data')).options;
      assert.equal(args.scope,'pins');assert.equal(args.page_size,3);assert.equal(args.query,'gaming mouse');
      return new Response(JSON.stringify({resource_response:{data:{results:[pin]}}}));
    }};
  const page={goto:async url=>assert.equal(url,'https://www.pinterest.com/search/pins/?q=gaming%20mouse'),
    evaluate:script=>vm.runInNewContext(script,context)};
  assert.equal((await readPinSearch(page,'gaming mouse',3,async(p,task)=>task(p))).length,1);
  assert.equal(calls,1);
});
test('source drift and absent CSRF preserve a safe reason; challenge stops before resource access', async () => {
  let calls=0;
  const context={location:{origin:'https://www.pinterest.com',pathname:'/search/pins/',search:'?q=fashion'},
    document:{body:{innerText:''},cookie:''},URLSearchParams,
    fetch:async()=>{calls++;throw new Error('must not fetch');}};
  const run=()=>vm.runInNewContext(`(${fetchNativeResource.toString()})("search","fashion")`,context);
  assert.deepEqual(JSON.parse(JSON.stringify(await run())),{stop:'read',stage:'source'});
  context.document.body.innerText='Security check: verify you are human';
  assert.equal((await run()).stop,'risk');
  context.location.pathname='/login/';context.document.body.innerText='';
  assert.equal((await run()).stop,'auth');
  context.location.pathname='/search/users/';
  assert.deepEqual(JSON.parse(JSON.stringify(await run())),{stop:'read',stage:'session'});
  assert.equal(calls,0);
});
test('reader forwards only fixed safe stages without treating missing CSRF as auth', async () => {
  const page={goto:async()=>{},evaluate:async()=>({stop:'read',stage:'session'})};
  await assert.rejects(readSearch(page,'fashion',1,async(p,task)=>task(p)),/ONECREW_PINTEREST_READ_SESSION/);
  page.evaluate=async()=>({stop:'read',stage:'private response or URL'});
  await assert.rejects(readSearch(page,'fashion',1,async(p,task)=>task(p)),/^Error: ONECREW_PINTEREST_READ$/);
});
test('actual search reader requests its bounded count rather than an oversized twenty-user payload', async () => {
  let calls=0, pageSize;
  const context={location:{origin:'https://www.pinterest.com',pathname:'/search/users/',search:'?q=fashion'},
    document:{body:{innerText:''},cookie:'csrftoken=fixture'},URLSearchParams,AbortSignal,Uint8Array,TextDecoder,
    fetch:async(path,options)=>{
      calls++;assert.equal(path,'/resource/BaseSearchResource/get/');
      pageSize=JSON.parse(new URLSearchParams(options.body).get('data')).options.page_size;
      const users=Array.from({length:pageSize},(_,index)=>({...profile,id:String(index+1),username:`author_${index}`}));
      return new Response(JSON.stringify({resource_response:{data:{results:users}},padding:'x'.repeat(pageSize*50000)}));
    }};
  const page={goto:async()=>{},evaluate:script=>vm.runInNewContext(script,context)};
  const rows=await readSearch(page,'fashion',5,async(p,task)=>task(p));
  assert.equal(rows.length,5);assert.equal(pageSize,5);assert.equal(calls,1);
  await assert.rejects(readSearch(page,'fashion',20,async(p,task)=>task(p)),/ONECREW_PINTEREST_READ_RESPONSE_SIZE/);
  assert.equal(calls,2);
});
test('only native pinner-bound captions qualify; repins and ads never qualify', () => {
  const target = projectPinTarget([pin],profile.id,profile.username);
  const content = projectPinContent(pin,target.pin_id,profile.id,profile.username);
  assert.equal(content.source_url,'https://www.pinterest.com/pin/456/');
  assert.equal(content.text_truncated,true);
  assert.equal(content.text,'Keyboard review\nEnglish original caption. Business studio@example.org');
  for (const patch of [{pinner:{id:'999',username:profile.username}},{pinner:{id:profile.id,username:'other'}},{is_repin:true},{promoted_is_ad:true},{id:456},{type:'user'}]) {
    assert.throws(()=>projectPinTarget([{...pin,...patch}],profile.id,profile.username));
    assert.throws(()=>projectPinContent({...pin,...patch},'456',profile.id,profile.username));
  }
  assert.throws(()=>projectPinTarget(new Array(5).fill({type:'ad'}).concat(pin),profile.id,profile.username));
  assert.throws(()=>projectPinContent({...pin,description:'日'.repeat(5001)},'456',profile.id,profile.username));
  assert.throws(()=>projectPinContent({...pin,title:'',description:''},'456',profile.id,profile.username));
  assert.throws(()=>projectPinContent({...pin,description:'private\0invalid'},'456',profile.id,profile.username));
});
test('absence of an own caption reports bounded fixed reasons without provider data',()=>{
  assert.throws(()=>projectPinTarget([{...pin,is_repin:true}],profile.id,profile.username),/^Error: ONECREW_PINTEREST_READ_CAPTION_UNAVAILABLE inspected=1 type=0 author=0 repin=1 ad=0$/);
  const rows = [{type:'unexpected-secret'}, {...pin,pinner:{id:'other-secret',username:'other'}},
    {...pin,is_repin:true,promoted_is_ad:true}, {...pin,promoted_is_ad:true}, null, pin];
  assert.throws(()=>projectPinTarget(rows,profile.id,profile.username),
    /^Error: ONECREW_PINTEREST_READ_CAPTION_UNAVAILABLE inspected=5 type=2 author=1 repin=1 ad=1$/);
  assert.throws(()=>projectPinTarget([],profile.id,profile.username),
    /^Error: ONECREW_PINTEREST_READ_CAPTION_UNAVAILABLE inspected=0 type=0 author=0 repin=0 ad=0$/);
});

test('serialized content reader follows only observed Pin, keeps raw fields in browser and cleans on stop',async()=>{
  const navigations=[];let count=0, cleaned=0;
  const context={TextEncoder};
  const page={goto:async url=>navigations.push(url),evaluate:async script=>{
    count++;
    const replacement = fetchNativeResource.toString();
    // Execute the actual serialized projection, substituting only native transport.
    return vm.runInNewContext(script.replace(replacement,`async function(){return ${JSON.stringify(count%2 ? [pin]:{...pin,session:'secret-not-returned'})};}`),context);
  }};
  const owned=async(p,task)=>{try{return await task(p);}finally{cleaned++;}};
  const [row]=await readContent(page,profile.id,profile.username,owned);
  assert.deepEqual(navigations,['https://www.pinterest.com/native_author/','https://www.pinterest.com/pin/456/']);
  assert.equal(count,2);assert.equal(cleaned,1);
  assert.equal(JSON.stringify(row).includes('secret-not-returned'),false);
  for(const stop of ['auth','risk','read']){
    const previous=count;page.evaluate=async()=>{count++;return {session:'bridge',data:{stop}};};
    await assert.rejects(readContent(page,profile.id,profile.username,owned));
    assert.equal(count,previous+1);
  }
  assert.equal(cleaned,4);
  await assert.rejects(readContent(page,123,profile.username,owned));
  assert.equal(cleaned,4);
});

test('native Pin requests bind feed and detail URLs; no paging or unrelated resource',async()=>{
  const context={URLSearchParams,AbortSignal,Uint8Array,TextDecoder,
    location:{origin:'https://www.pinterest.com',pathname:'/native_author/'},document:{body:{innerText:''},cookie:'csrftoken=local-fixture'},
    fetch:async(path,options)=>{
      assert.equal(path,'/resource/UserPinsResource/get/');
      const body=new URLSearchParams(options.body);
      assert.deepEqual(JSON.parse(body.get('data')).options,{username:'native_author',field_set_key:'grid_item',page_size:5});
      return new Response(JSON.stringify({resource_response:{data:[pin]}}));
    }};
  const run=(kind,target)=>vm.runInNewContext(`(${fetchNativeResource.toString()})(${JSON.stringify(kind)},${JSON.stringify(target)})`,context);
  assert.equal((await run('pins','native_author'))[0].id,'456');
  assert.equal((await run('pins','other')).stop,'read');
  context.location.pathname='/pin/456/';
  context.fetch=async(path,options)=>{
    assert.equal(path,'/resource/PinResource/get/');
    assert.deepEqual(JSON.parse(new URLSearchParams(options.body).get('data')).options,{id:'456',field_set_key:'detailed'});
    return new Response(JSON.stringify({resource_response:{data:pin}}));
  };
  assert.equal((await run('pin','456')).pinner.id,profile.id);
  assert.equal((await run('pin','789')).stop,'read');
});

test('observed title-slug redirect retains the reserved Pin and never follows a different identity',async()=>{
  const id='15621929951103237';
  const native={...pin,id};
  let calls=0;
  const context={URLSearchParams,AbortSignal,Uint8Array,TextDecoder,
    location:{origin:'https://www.pinterest.com',pathname:`/pin/rate-this-look-from-110--${id}/`},
    document:{body:{innerText:''},cookie:'csrftoken=fixture'},
    fetch:async(path,options)=>{
      calls++;assert.equal(path,'/resource/PinResource/get/');
      const body=new URLSearchParams(options.body);
      assert.equal(body.get('source_url'),`/pin/${id}/`);
      assert.deepEqual(JSON.parse(body.get('data')).options,{id,field_set_key:'detailed'});
      return new Response(JSON.stringify({resource_response:{data:native}}));
    }};
  const run=()=>vm.runInNewContext(`(${fetchNativeResource.toString()})("pin",${JSON.stringify(id)})`,context);
  const result=await run();
  assert.equal(result.id,id);assert.equal(calls,1);
  assert.equal(projectPinContent(result,id,profile.id,profile.username).source_url,`https://www.pinterest.com/pin/${id}/`);
  for(const path of ['/pin/other-look--789/',`/pin/${id}/comments/`,`/pin/nested/look--${id}/`,`/user/look--${id}/`,`/pin/look--${id}0/`]){
    context.location.pathname=path;
    assert.equal((await run()).stop,'read');assert.equal(calls,1,path);
  }
  context.location.pathname=`/pin/look--${id}/`;context.location.origin='https://other.example.org';
  assert.equal((await run()).stop,'read');assert.equal(calls,1);
  context.location.origin='https://www.pinterest.com';context.document.body.innerText='Verify you are human';
  assert.equal((await run()).stop,'risk');assert.equal(calls,1);
  assert.throws(()=>projectPinContent({...native,id:'789'},id,profile.id,profile.username));
  assert.throws(()=>projectPinContent({...native,pinner:{id:'999',username:profile.username}},id,profile.id,profile.username));
});
