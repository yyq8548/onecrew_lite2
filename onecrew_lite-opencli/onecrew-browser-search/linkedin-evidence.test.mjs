import assert from 'node:assert/strict';
import {test} from 'node:test';
import vm from 'node:vm';
import {readFileSync} from 'node:fs';
import {withOwnedSearchPage} from './owned-page.mjs';
import {linkedinProfileTarget, linkedinConnections, projectLinkedinProfile, observeLinkedinProfile, readLinkedinProfile,
  projectLinkedinContent, observeLinkedinContent, readLinkedinContent,
  projectLinkedinContact, observeLinkedinContact, readLinkedinContact} from './linkedin-evidence.mjs';

const handle = 'native-author';
const id = 'ACoAAA123456789abcdefghijk';
const source = linkedinProfileTarget(handle);
const fixture = () => ({author_handle:handle, account_id:id, author_label:'Native Author', profile_url:source,
  headline:'Keyboard reviews', bio:'Business: studio@example.org', follower_text:'1,606 followers',
  public_links:['https://creator.example/contact'], contact_links:[`${source}overlay/contact-info/`]});

test('actual LinkedIn connections observation keeps the open upper bound separate from followers', () => {
  const actual = JSON.parse(readFileSync(new URL('../../onecrew_lite-app/tests/fixtures/linkedin-metrics-profile-20261020.json', import.meta.url), 'utf8')).profile;
  assert.deepEqual(linkedinConnections(actual.connections.raw), {raw:'500+connections', lower:500, upper:null});
  const projected = projectLinkedinProfile({...actual,connection_text:actual.connections.raw}, actual.author_handle);
  assert.equal(projected.follower_count,1548);
  assert.deepEqual(projected.connections,actual.connections);
});

test('LinkedIn projection preserves public identity, original bio and observed contact locators only', () => {
  const row = projectLinkedinProfile({...fixture(), session:'private', recommendation_email:'not-author@example.org'},handle);
  assert.equal(row.account_id,id);
  assert.equal(row.follower_count,1606);
  assert.equal(row.bio,fixture().bio);
  assert.equal(row.bio_truncated,true);
  assert.deepEqual(row.public_links,fixture().public_links);
  assert.deepEqual(row.contact_links,fixture().contact_links);
  assert.doesNotMatch(JSON.stringify(row),/private|not-author/);
  for (const count of ['1.6K followers','530 abonnés','', 'not provided'])
    assert.equal(projectLinkedinProfile({...fixture(),follower_text:count},handle).follower_count,null);
  assert.equal(projectLinkedinProfile({...fixture(),follower_text:'0 followers'},handle).follower_count,0);
});

test('wrong owner, private links, redirects and unsupported profile targets fail closed', () => {
  for (const changes of [{author_handle:'other'}, {profile_url:linkedinProfileTarget('other')}, {account_id:handle},
    {author_label:'bad\0name'}, {bio:'界'.repeat(2667)}, {public_links:['https://user:secret@creator.example/']},
    {public_links:['https://127.0.0.1/']}, {public_links:['https://internal/']}, {public_links:['javascript:alert(1)']},
    {public_links:['https://creator.example:4433/']}, {contact_links:[`${linkedinProfileTarget('other')}overlay/contact-info/`]},
    {contact_links:['https://creator.example/contact']}])
    assert.throws(()=>projectLinkedinProfile({...fixture(),...changes},handle),/INVALID/);
  for (const input of ['me','self','a/b','https://www.linkedin.com/in/author/','', 'x'.repeat(129)])
    assert.throws(()=>linkedinProfileTarget(input),/INPUT/);
});

function pageContext() {
  const visible = {getClientRects:()=>[1],closest:()=>null};
  const anchor = (href) => ({...visible,href});
  const name = {...visible,textContent:'Native Author',closest:selector=>selector==='a[href]'?anchor(source):selector==='section'?header:null};
  const message = anchor(`https://www.linkedin.com/messaging/compose/?profileUrn=${encodeURIComponent(`urn:li:fsd_profile:${id}`)}&recipient=${id}`);
  const website = anchor('https://www.linkedin.com/safety/go/?url='+encodeURIComponent('https://creator.example/contact'));
  const contact = anchor(`${source}overlay/contact-info/`);
  const paragraphs = ['· 3rd','Keyboard reviews','1,606 followers'].map(textContent=>({...visible,textContent}));
  const aboutHeading = {...visible,textContent:'About'};
  const about = {querySelector:()=>null,querySelectorAll:selector=>selector==='h2'?[aboutHeading]:[{...visible,textContent:'Business: studio@example.org'}]};
  const header = {querySelectorAll:selector=>selector==='h1,h2,h3'?[name]:selector==='a[href]'?[message,message,website,contact]:paragraphs};
  const main = {querySelectorAll:selector=>selector==='h1,h2'?[name]:[header,about]};
  const treeWalker = paragraph => {
    const nodes=paragraph.nodes || [{textContent:paragraph.textContent,parentElement:paragraph}];
    let index=-1;
    return {nextNode(){index++;this.currentNode=nodes[index];return index<nodes.length;},currentNode:null};
  };
  const context = {location:{origin:'https://www.linkedin.com',pathname:`/in/${handle}/`,href:source},URL,TextEncoder,
    NodeFilter:{SHOW_TEXT:4},document:{querySelector:()=>main,querySelectorAll:()=>[],createTreeWalker:treeWalker}};
  return {context,header,name,message,main,paragraphs,about};
}

test('profile observer binds native compose recipient inside header and ignores sidebar identities', () => {
  const {context,header,message} = pageContext();
  const execute = () => vm.runInNewContext(`(${observeLinkedinProfile.toString()})(${JSON.stringify(source)},${JSON.stringify(handle)})`,context);
  const data = execute();
  assert.equal(data.account_id,id);
  assert.equal(data.bio,fixture().bio);
  assert.deepEqual(Array.from(data.public_links),fixture().public_links);
  assert.deepEqual(Array.from(data.contact_links),fixture().contact_links);
  assert.equal(projectLinkedinProfile(data,handle).follower_count,1606);
  const original = header.querySelectorAll;
  header.querySelectorAll = selector=>selector==='a[href]'?[message,{...message,href:message.href.replaceAll(id,'ACoAAA987654321otheraccount')}]:original(selector);
  assert.throws(execute,/INVALID/,'conflicting header native identities must not resolve');
  header.querySelectorAll = original;
  message.href=message.href.replace(`recipient=${id}`,'recipient=ACoAAA987654321otheraccount');
  assert.throws(execute,/INVALID/,'native URN and recipient must agree');
});

test('unlinked profile heading requires its exact own contact locator and native recipient', () => {
  const {context,header,name}=pageContext();
  const originalClosest=name.closest;
  name.closest=selector=>selector==='a[href]'?null:originalClosest(selector);
  const execute=()=>vm.runInNewContext(`(${observeLinkedinProfile.toString()})(${JSON.stringify(source)},${JSON.stringify(handle)})`,context);
  assert.equal(execute().account_id,id);
  const original=header.querySelectorAll;
  header.querySelectorAll=selector=>selector==='a[href]'?original(selector).filter(a=>!a.href.endsWith('/overlay/contact-info/')):original(selector);
  assert.equal(execute(),null);
});

test('observed non-self redirect retains canonical identity; arbitrary identity queries remain rejected', () => {
  const setup = pageContext();
  const execute = () => vm.runInNewContext(`(${observeLinkedinProfile.toString()})(${JSON.stringify(source)},${JSON.stringify(handle)})`,setup.context);
  setup.context.location.href = `${source}?isSelfProfile=false`;
  const original = setup.name.closest;
  setup.name.closest = selector => selector==='a[href]'?{href:`${source}?isSelfProfile=false`}:original(selector);
  assert.equal(execute().profile_url,source);
  for (const query of ['?isSelfProfile=true','?isSelfProfile=false&next=other','?next=other','#other']) {
    setup.context.location.href=source+query;
    assert.throws(execute,/INVALID/);
  }
  setup.context.location.href=source;
  setup.name.closest=selector=>selector==='a[href]'?{href:`${source}?isSelfProfile=true`}:original(selector);
  assert.equal(execute(),null);
  const content = contentContext();
  content.context.location.href=`${source}?isSelfProfile=false`;
  content.author.href=`${source}?isSelfProfile=false`;
  assert.equal(content.execute().account_id,id);
});

test('profile bio excludes buttons and hidden descendants before contact or language evidence', () => {
  const {context,about}=pageContext();
  const paragraph={getClientRects:()=>[1],closest:()=>null,textContent:'Original bio ...more hidden@example.org',nodes:[
    {textContent:'Original bio ',parentElement:{closest:()=>null}},
    {textContent:'...more',parentElement:{closest:()=>({tagName:'BUTTON'})}},
    {textContent:'hidden@example.org',parentElement:{closest:()=>({})}}]};
  const original=about.querySelectorAll;
  about.querySelectorAll=selector=>selector==='p'?[paragraph]:original(selector);
  const data=vm.runInNewContext(`(${observeLinkedinProfile.toString()})(${JSON.stringify(source)},${JSON.stringify(handle)})`,context);
  assert.equal(data.bio,'Original bio');
});

function contentContext() {
  const setup=pageContext();
  const visible={getClientRects:()=>[1],closest:()=>null};
  const post='7432844064649293824';
  const url=`https://www.linkedin.com/feed/update/urn:li:activity:${post}/`;
  const paragraph={...visible,textContent:'Keyboard review ...more',nodes:[
    {textContent:'Keyboard review. Contact studio@example.org ',parentElement:{closest:()=>null}},
    {textContent:'...more',parentElement:{closest:()=>({tagName:'BUTTON'})}},
    {textContent:'foreign@example.org',parentElement:{closest:()=>({})}}]};
  const body={...visible,href:url,querySelectorAll:selector=>['p','p [data-testid="expandable-text-box"]'].includes(selector)?[paragraph]:[]};
  const author={...visible,href:source};
  const comment={...visible,href:url,querySelectorAll:selector=>selector==='p'?[{textContent:'2 comments'}]:[]};
  const card={...visible,querySelectorAll:()=>[author,body,comment]};
  const activity={querySelector:()=>null,querySelectorAll:selector=>selector==='h2'?[{textContent:'Activity'}]:[card]};
  const original=setup.main.querySelectorAll;
  setup.main.querySelectorAll=selector=>selector==='section'?[activity]:original(selector);
  const execute=()=>vm.runInNewContext(`(${observeLinkedinContent.toString()})(${JSON.stringify(source)},${JSON.stringify(handle)},${JSON.stringify(id)},(${observeLinkedinProfile.toString()}))`,setup.context);
  return {...setup,post,url,paragraph,body,author,comment,card,activity,execute};
}

test('native Activity body binds profile account, strips UI text and persists the actual post URL', () => {
  const {execute,post,url}=contentContext();
  const raw=execute();
  const result=projectLinkedinContent({...raw,session:'private'},handle,id);
  assert.equal(result.post_id,post);
  assert.equal(result.source_url,url);
  assert.equal(result.account_id,id);
  assert.equal(result.text,'Keyboard review. Contact studio@example.org');
  assert.equal(result.text_truncated,false);
  assert.equal(result.observations.text_completeness,'unknown');
  assert.doesNotMatch(JSON.stringify(result),/more|foreign|private/);
  assert.equal(Object.keys(result).length,9);
  for(const changes of [{account_id:'ACoAAA987654321otheraccount'}, {author_handle:'other'},
    {profile_url:linkedinProfileTarget('other')}, {post_id:'0'}, {source_url:source},
    {text:'界'.repeat(5334)}, {text:'bad\0body'}, {text:' '}])
    assert.throws(()=>projectLinkedinContent({...raw,...changes},handle,id),/INVALID/);
});

test('reposts, company cards, ambiguous sources and wrong native identity cannot supply author body', () => {
  const {execute,card,body,author,comment,message}=contentContext();
  for(const anchors of [[author,{...author,href:linkedinProfileTarget('other')},body],
    [author,{...author,href:'https://www.linkedin.com/company/other/'},body],
    [author,body,{...comment,href:body.href.replace('7432844064649293824','7432844064649293825')}],
    [author,body,{...body}], [body], [author,comment]]) {
    card.querySelectorAll=()=>anchors;
    assert.equal(execute(),null);
  }
  card.querySelectorAll=()=>[author,body,comment];
  message.href=message.href.replaceAll(id,'ACoAAA987654321otheraccount');
  assert.throws(execute,/INVALID/);
});

test('Activity scans only the first five visible positions and bounds UTF-8 before returning', () => {
  const {execute,activity,card,paragraph}=contentContext();
  const repost={getClientRects:()=>[1],querySelectorAll:()=>[{href:linkedinProfileTarget('other'),getClientRects:()=>[1]}]};
  activity.querySelectorAll=selector=>selector==='h2'?[{textContent:'Activity'}]:[repost,repost,repost,repost,repost,card];
  assert.equal(execute(),null,'sixth card cannot replace an ineligible first-five card');
  activity.querySelectorAll=selector=>selector==='h2'?[{textContent:'Activity'}]:[repost,card];
  paragraph.nodes=[{textContent:'界'.repeat(6000),parentElement:{closest:()=>null}}];
  const result=execute();
  assert.equal(new TextEncoder().encode(result.text).length,15999);
  assert.equal(projectLinkedinContent(result,handle,id).text_truncated,true);
});

test('content reader stays on the observed profile, cleans up on failure and does not fetch another author', async () => {
  const {context,message}=contentContext();
  const calls=[];
  const page={goto:async url=>calls.push(url),evaluate:async script=>({session:'private',data:vm.runInNewContext(script,context)}),sleep:async()=>{throw new Error('Unexpected retry');}};
  const owned=async(p,task)=>{try{return await task(p);}finally{calls.push('closed');}};
  const result=await readLinkedinContent(page,handle,id,owned);
  assert.equal(result.account_id,id);
  assert.deepEqual(calls,[source,'closed']);
  message.href=message.href.replaceAll(id,'ACoAAA987654321otheraccount');
  await assert.rejects(()=>readLinkedinContent(page,handle,id,owned),/INVALID/);
  assert.deepEqual(calls,[source,'closed',source,'closed']);
  await assert.rejects(()=>readLinkedinContent(page,handle,handle,owned),/INPUT/);
  assert.deepEqual(calls,[source,'closed',source,'closed']);
});

test('missing visible Activity is unavailable evidence, not malformed provider output', async () => {
  let reads=0;
  const page={goto:async()=>{},evaluate:async()=>{reads++;return null;},sleep:async()=>{}};
  await assert.rejects(
    readLinkedinContent(page,handle,id,async(p,task)=>task(p)),
    /ONECREW_LINKEDIN_CONTENT_UNAVAILABLE/,
  );
  assert.equal(reads,12);
});

test('observer rejects wrong page, login, challenge, missing native ID and multi-section header', () => {
  const {context,header} = pageContext();
  const execute = () => vm.runInNewContext(`(${observeLinkedinProfile.toString()})(${JSON.stringify(source)},${JSON.stringify(handle)})`,context);
  for (const [pathname,error] of [['/checkpoint/challenge','RISK'],['/login','AUTH']]) {
    context.location.pathname=pathname;assert.throws(execute,new RegExp(error));
  }
  context.location.pathname=`/in/${handle}/`;
  context.location.href=linkedinProfileTarget('other');assert.throws(execute,/INVALID/);
  context.location.href=source;
  context.location.origin='https://www.linkedin.com.evil.example';assert.throws(execute,/INVALID/);
  context.location.origin='https://www.linkedin.com';
  const original=header.querySelectorAll;
  header.querySelectorAll=selector=>selector==='a[href]'?[]:original(selector);
  assert.equal(execute(),null,'profile URL/name alone cannot prove native account identity');
  header.querySelectorAll=selector=>selector==='h1,h2,h3'?[{},{}]:original(selector);
  assert.equal(execute(),null,'do not include Activity or recommendations in the profile header');
});

test('profile reader waits for About hydration on the same native author page', async () => {
  let reads=0, sleeps=0;
  const page={goto:async()=>{}, evaluate:async()=>({...fixture(),bio:reads++?'Business: studio@example.org':'',about_pending:reads===1}),sleep:async()=>{sleeps++;}};
  const row=await readLinkedinProfile(page,handle,async(p,task)=>task(p));
  assert.equal(row.bio,'Business: studio@example.org');
  assert.equal(reads,2);
  assert.equal(sleeps,1);
});

test('reader creates one owned page, projects inside bounded scope, cleans up and never retries another author', async () => {
  const {context} = pageContext();
  const calls=[];
  const page={goto:async url=>calls.push(url),evaluate:async script=>({session:'private',data:vm.runInNewContext(script,context)}),sleep:async()=>{throw new Error('Unexpected retry');}};
  const owned=async(p,task)=>{try{return await task(p);}finally{calls.push('closed');}};
  const result=await readLinkedinProfile(page,handle,owned);
  assert.equal(result.account_id,id);
  assert.deepEqual(calls,[source,'closed']);
  assert.doesNotMatch(JSON.stringify(result),/private/);
  context.location.pathname='/checkpoint/challenge';
  await assert.rejects(()=>readLinkedinProfile(page,handle,owned),/RISK/);
  assert.deepEqual(calls,[source,'closed',source,'closed']);
});

function contactContext() {
  const setup=pageContext();
  setup.context.location.href=`${source}overlay/contact-info/`;
  setup.context.location.pathname=`/in/${handle}/overlay/contact-info/`;
  const visible={getClientRects:()=>[1],closest:()=>null};
  const author={...visible,href:source};
  const email={...visible,href:'mailto:studio@example.org?subject=Business'};
  const website={...visible,href:'https://www.linkedin.com/safety/go/?url='+encodeURIComponent('https://creator.example/contact')};
  const telegram={...visible,href:'https://t.me/creator_business'};
  const premium={...visible,href:'https://www.linkedin.com/premium/redeem/?private=never-return'};
  const scope={querySelector:()=>null,querySelectorAll:selector=>selector==='h1,h2,h3'?[{textContent:'联系信息'}]:[author,email,website,telegram,premium]};
  const dialog={...visible,querySelector:()=>({parentElement:scope})};
  setup.context.document.querySelectorAll=selector=>selector==='dialog[open], [role="dialog"][aria-modal="true"]'?[dialog]:[];
  const execute=()=>vm.runInNewContext(`(${observeLinkedinContact.toString()})(${JSON.stringify(source)},${JSON.stringify(handle)},${JSON.stringify(id)},(${observeLinkedinProfile.toString()}))`,setup.context);
  return {...setup,author,email,website,telegram,premium,scope,dialog,execute};
}

test('contact dialog binds native background identity and emits only visible email and public links', () => {
  const {execute}=contactContext();
  const row=projectLinkedinContact({...execute(),session:'private'},handle,id);
  assert.equal(row.contact_text,'studio@example.org');
  assert.equal(row.source_url,`${source}overlay/contact-info/`);
  assert.deepEqual(row.contact_links,['https://creator.example/contact','https://t.me/creator_business']);
  assert.equal(row.evidence_source,'visible_contact_dialog');
  assert.equal(row.truncated,true);
  assert.doesNotMatch(JSON.stringify(row),/session|premium|private/);
  for(const change of [{author_handle:'other'}, {account_id:'ACoAAA987654321otheraccount'}, {source_url:source},
    {title:'bad\0name'}, {contact_text:'界'.repeat(2667)}, {contact_links:['https://127.0.0.1/']}])
    assert.throws(()=>projectLinkedinContact({...execute(),...change},handle,id),/INVALID/);
});

test('wrong contact owner, missing dialog, UI links and unsupported redirects do not supply contacts', () => {
  const setup=contactContext();
  setup.author.href=linkedinProfileTarget('other');
  assert.throws(setup.execute,/INVALID/);
  setup.author.href=source;
  const original=setup.scope.querySelectorAll;
  setup.scope.querySelectorAll=selector=>selector==='h1,h2,h3'?[{}]:[setup.author,setup.premium];
  assert.equal(setup.execute().contact_text,'');
  assert.deepEqual(Array.from(setup.execute().contact_links),[]);
  setup.scope.querySelectorAll=original;
  setup.email.closest=()=>({tagName:'BUTTON'});
  assert.equal(setup.execute().contact_text,'','button email cannot masquerade as author contact');
  setup.email.closest=()=>null;
  setup.website.href='https://www.linkedin.com/safety/go/?url=www.creator.example';
  assert.deepEqual(Array.from(setup.execute().contact_links),['https://t.me/creator_business'],'scheme must not be guessed');
  setup.scope.querySelector=()=>({tagName:'ARTICLE'});
  assert.equal(setup.execute(),null);
  setup.scope.querySelector=()=>null;
  setup.context.document.querySelectorAll=()=>[];
  assert.equal(setup.execute(),null);
});

test('contact observation waits for this profile SPA route, without accepting source drift',()=>{
  const setup=contactContext();
  setup.context.location.href=source;
  assert.equal(setup.execute(),null);
  setup.context.location.href=`${source}?isSelfProfile=false`;
  assert.equal(setup.execute(),null);
  setup.context.location.href=linkedinProfileTarget('other');
  assert.throws(setup.execute,/INVALID/);
  setup.context.location.href=`${source}overlay/contact-info/`;
  assert.equal(setup.execute().contact_text,'studio@example.org');
});

test('contact dialog skeleton is pending until its author link is visible',()=>{
  const setup=contactContext();
  const select=setup.scope.querySelectorAll;
  setup.scope.querySelectorAll=selector=>selector==='a[href]'?[]:select(selector);
  assert.equal(setup.execute(),null);
  setup.scope.querySelectorAll=select;
  assert.equal(setup.execute().contact_text,'studio@example.org');
  setup.author.href=linkedinProfileTarget('other');
  assert.throws(setup.execute,/INVALID_CONTACT_DIALOG_OWNER/);
});

test('contact reader rechecks observed locator and native identity; owned pages close on every path', async () => {
  const setup=contactContext();
  const calls=[];
  const selector=`main a[href="${source}overlay/contact-info/"]`;
  const page={goto:async url=>{calls.push(url);setup.context.location.href=url;setup.context.location.pathname=new URL(url).pathname;},
    click:async target=>{assert.equal(target,selector);calls.push('contact-click');setup.context.location.href=`${source}overlay/contact-info/`;},
    evaluate:async script=>({session:'private',data:vm.runInNewContext(script,setup.context)}),sleep:async()=>{throw new Error('Unexpected retry');}};
  const owned=async(p,task)=>{try{return await task(p);}finally{calls.push('closed');}};
  const row=await readLinkedinContact(page,handle,id,owned);
  assert.equal(row.contact_text,'studio@example.org');
  assert.deepEqual(calls,[source,'contact-click','closed']);
  setup.message.href=setup.message.href.replaceAll(id,'ACoAAA987654321otheraccount');
  await assert.rejects(()=>readLinkedinContact(page,handle,id,owned),/INVALID/);
  assert.deepEqual(calls,[source,'contact-click','closed',source,'closed']);
  await assert.rejects(()=>readLinkedinContact(page,handle,handle,owned),/INPUT/);
  assert.equal(calls.length,5);
});

test('contact overlay follows the observed link in its owned profile tab; direct overlay navigation bounces',async()=>{
  const setup=contactContext(),events=[];
  setup.context.document.body={};
  setup.context.document.readyState='complete';
  let opened=false;
  const readDialogs=setup.context.document.querySelectorAll;
  setup.context.document.querySelectorAll=selector=>opened?readDialogs(selector):[];
  const page={
    newTab:async url=>{events.push(['new',url]);opened=false;
      setup.context.location.href=source;setup.context.location.pathname=new URL(source).pathname;
      return 'owned-profile';},
    setActivePage:async target=>events.push(['active',target]),
    closeTab:async target=>events.push(['closed',target]),
    sleep:async()=>{},
    click:async selector=>{assert.equal(selector,`main a[href="${source}overlay/contact-info/"]`);
      events.push(['click',selector]);opened=true;setup.context.location.href=`${source}overlay/contact-info/`;},
    evaluate:async script=>script.includes('restoreOriginalLanguage')?true:
      {session:'private',data:await vm.runInNewContext(script,setup.context)}
  };
  const row=await readLinkedinContact(page,handle,id,withOwnedSearchPage);
  assert.equal(row.contact_text,'studio@example.org');
  assert.deepEqual(events.filter(e=>e[0]==='new'),[['new',source]]);
  assert.equal(events.filter(e=>e[0]==='click').length,1);
  assert.deepEqual(events.filter(e=>e[0]==='closed'),[['closed','owned-profile']]);
  events.length=0;
  page.click=async()=>{throw new Error('click-unavailable');};
  await assert.rejects(readLinkedinContact(page,handle,id,withOwnedSearchPage),/click-unavailable/);
  assert.deepEqual(events.filter(e=>e[0]==='closed'),[['closed','owned-profile']]);
});
