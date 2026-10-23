import assert from 'node:assert/strict';
import test from 'node:test';
import {facebookProfileTarget,projectFacebookProfile,readFacebookProfile,readFacebookProfileHeader,facebookContactTarget,projectFacebookContact,readFacebookContactPanel,readFacebookContact} from './facebook-profile.mjs';
const id='61590270912364';
const fixture=()=>({id,profile_url:facebookProfileTarget(id),display_name:'Hair Dye Fast',follower_text:'1.3K followers'});
test('numeric observed ID is the only profile target',()=>{
  assert.equal(facebookProfileTarget(id),`https://www.facebook.com/profile.php?id=${id}`);
  for(const bad of ['creator','0123','123&sk=followers','123#fragment','1e3','',123]) assert.throws(()=>facebookProfileTarget(bad));
});
test('abbreviated or unrecognized counts remain unknown, exact zero is observed',()=>{
  for(const text of ['1.3K followers','Followers','1.3万位粉丝','']) {
    const row=projectFacebookProfile({...fixture(),follower_text:text},id);
    assert.equal(row.follower_count,null);assert.equal(row.follower_text,text);
  }
  for(const [text,count] of [['0 followers',0],['1 follower',1],['12,345 followers',12345]])
    assert.equal(projectFacebookProfile({...fixture(),follower_text:text},id).follower_count,count);
});
test('reject identity drift, redirects, malformed and oversized public fields',()=>{
  for(const changes of [{id:'456'},{profile_url:facebookProfileTarget('456')},{profile_url:facebookProfileTarget(id)+'&redirect=x'},
    {display_name:''},{display_name:'x'.repeat(201)},{display_name:'x\0y'},{follower_text:'9007199254740992 followers'},{follower_text:'x'.repeat(101)}])
    assert.throws(()=>projectFacebookProfile({...fixture(),...changes},id));
});
test('invalid numeric profile targets fail before any browser ownership is requested',async()=>{
  for(const id of ['creator','0','123&sk=about','123/456']) await assert.rejects(()=>readFacebookProfile(null,{id},null),/ONECREW_FB_PROFILE_INPUT/);
});
test('numeric profile header binds only its visible relationship link and unique name',async()=>{
  const previous={location:globalThis.location,document:globalThis.document};
  const visible={getClientRects:()=>[{}]};
  const name={...visible,innerText:'Hair Dye Fast'};
  const header={querySelectorAll:()=>[name]};
  const link={...visible,href:facebookProfileTarget(id)+'&sk=followers',innerText:'1.3K followers',getAttribute:()=>null,
    querySelectorAll:()=>[],parentElement:header};
  const main={...visible,querySelectorAll:()=>[link]};
  try {
    globalThis.location={origin:'https://www.facebook.com',pathname:'/profile.php',href:facebookProfileTarget(id)};
    globalThis.document={querySelectorAll:()=>[main]};
    const data=await readFacebookProfileHeader(facebookProfileTarget(id),id);
    assert.deepEqual(data,fixture());
    assert.equal(projectFacebookProfile(data,id).follower_count,null);
    main.querySelectorAll=()=>[link,link];
    await assert.rejects(()=>readFacebookProfileHeader(facebookProfileTarget(id),id),/INVALID/);
    main.querySelectorAll=()=>[link];header.querySelectorAll=()=>[name,name];
    await assert.rejects(()=>readFacebookProfileHeader(facebookProfileTarget(id),id),/INVALID/);
    globalThis.location.pathname='/login/';
    await assert.rejects(()=>readFacebookProfileHeader(facebookProfileTarget(id),id),/AUTH/);
    globalThis.location.pathname='/checkpoint/';
    await assert.rejects(()=>readFacebookProfileHeader(facebookProfileTarget(id),id),/RISK/);
    globalThis.location.pathname='/profile.php';globalThis.location.href=facebookProfileTarget('456');
    await assert.rejects(()=>readFacebookProfileHeader(facebookProfileTarget(id),id),/INVALID/);
  } finally {globalThis.location=previous.location;globalThis.document=previous.document;}
});

test('Facebook contact navigation accepts only an observed same-author About tab',()=>{
  const data=fixture();data.contact_links=[`${facebookProfileTarget(id)}&sk=about_contact_and_basic_info`];
  assert.deepEqual(projectFacebookProfile(data,id).contact_links,data.contact_links);
  for(const link of [`${facebookProfileTarget('456')}&sk=about_contact_and_basic_info`,`${facebookProfileTarget(id)}&sk=timeline`,'https://evil.example/contact']) {
    assert.throws(()=>projectFacebookProfile({...fixture(),contact_links:[link]},id),/INVALID/);
  }
});

test('Facebook header exposes observed contact tab without timeline links',async()=>{
  const previous={location:globalThis.location,document:globalThis.document};
  const name={innerText:'Creator',getClientRects:()=>[1]};
  const header={querySelectorAll:()=>[name]};
  const link={href:facebookProfileTarget(id)+'&sk=followers',innerText:'2K followers',parentElement:header,querySelectorAll:()=>[],getClientRects:()=>[1],getAttribute:()=>null};
  const tab={href:facebookProfileTarget(id)+'&sk=about_contact_and_basic_info',getClientRects:()=>[1],getAttribute:()=> 'tab'};
  const main={getClientRects:()=>[1],querySelectorAll:()=>[link,tab]};
  try {
    globalThis.location={origin:'https://www.facebook.com',pathname:'/profile.php',href:facebookProfileTarget(id)};
    globalThis.document={querySelectorAll:()=>[main]};
    const projected=projectFacebookProfile(await readFacebookProfileHeader(facebookProfileTarget(id),id),id);
    assert.deepEqual(projected.contact_links,[tab.href]);
  } finally {globalThis.location=previous.location;globalThis.document=previous.document;}
});


test('contact projection preserves exact owner and strips unexpected fields',()=>{
  const data={id,source_url:facebookContactTarget(id),title:'Creator',contact_text:'business@example.com',contact_links:['https://creator.example/contact'],truncated:true};
  assert.deepEqual(projectFacebookContact({...data,adapter_version:'attacker',cookie:'secret'},id),{adapter_version:'1.0.0',...data});
  for(const changes of [{id:'456'},{source_url:facebookContactTarget('456')},{title:'bad\0name'},
    {contact_text:'x'.repeat(12001)},{contact_links:['javascript:alert(1)']},{contact_links:['https://user:secret@creator.example/']}, {truncated:null}])
    assert.throws(()=>projectFacebookContact({...data,...changes},id),/INVALID/);
});

test('contact reader rejects an About tab without an isolated author contact scope',async()=>{
  const previous={location:globalThis.location,document:globalThis.document,NodeFilter:globalThis.NodeFilter};
  const element=(text,excluded=false)=>({textContent:text,getClientRects:()=>[1],closest:()=>excluded?{}:null});
  const heading=element('Creator');const selected={...element('Contact'),href:facebookContactTarget(id)};
  const good=element('Business business@example.com');const feed=element('commenter@wrong.example',true);
  const linked={...element('Website'),href:'https://www.facebook.com/l.php?u='+encodeURIComponent('https://creator.example/contact')};
  const hidden={...element('Hidden'),href:'https://wrong.example/',getClientRects:()=>[]};
  let busy=true,iterations=0;
  const main={...element(''),querySelectorAll:selector=>selector==='h1'?[heading]:selector.includes('aria-selected')?[selected]:selector.includes('aria-busy')?(busy?[element('loading')]:[]):[linked,hidden]};
  const texts=[heading,good,feed].map(parentElement=>({parentElement,textContent:parentElement.textContent}));
  try {
    globalThis.location={origin:'https://www.facebook.com',pathname:'/profile.php',href:facebookContactTarget(id)};
    globalThis.NodeFilter={SHOW_TEXT:4};
    globalThis.document={querySelectorAll:()=>{iterations++;if(iterations===2)busy=false;return [main];},createTreeWalker:()=>{let i=-1;return {nextNode(){i++;this.currentNode=texts[i];return i<texts.length;}};}};
    await assert.rejects(()=>readFacebookContactPanel(facebookContactTarget(id),id),/CONTACT_INVALID/);
    assert.equal(iterations,2);
    globalThis.location.pathname='/two_step_verification/two_factor/';
    await assert.rejects(()=>readFacebookContactPanel(facebookContactTarget(id),id),/RISK/);
    globalThis.location.pathname='/login/';
    await assert.rejects(()=>readFacebookContactPanel(facebookContactTarget(id),id),/AUTH/);
    globalThis.location.pathname='/profile.php';globalThis.location.href=facebookContactTarget('456');
    await assert.rejects(()=>readFacebookContactPanel(facebookContactTarget(id),id),/INVALID/);
  } finally {Object.assign(globalThis,previous);}
});

test('contact command projects a single owned observation without additional navigation',async()=>{
  let navigations=0;
  const page={goto:async url=>{assert.equal(url,facebookContactTarget(id));navigations++;},evaluate:async()=>({session:'discard',data:{id,source_url:facebookContactTarget(id),title:'Creator',contact_text:'business@example.com',contact_links:[],truncated:true}})};
  const row=await readFacebookContact(page,{id},async(p,work)=>work(p));
  assert.equal(row.contact_text,'business@example.com');assert.equal(navigations,1);
});
