import test from 'node:test';
import assert from 'node:assert/strict';
import {facebookHeaderNodes} from './facebook-identity.mjs';
import {facebookNativeContactTarget,projectFacebookNativeProfile,projectFacebookNativeContact,readFacebookNativeHeader,readFacebookNativeProfile,readFacebookNativeContact} from './facebook-native-profile.mjs';
const base='https://www.facebook.com/LinusTech',id='100044572203205';
const source=base+'/directory_contact_info',canonical=`https://www.facebook.com/profile.php?id=${id}`;
const profile={id,display_name:'Linus Tech Tips',profile_url:canonical,observed_profile_url:base,follower_text:'1.3M followers',contact_links:[source,base+'/directory_links']};
const response=(nativeId=id)=>({__bbox:{result:{data:{user:{profile_header_renderer:{user:{__typename:'User',id:nativeId,name:profile.display_name,url:base}}}}}}});

test('native projections bind exact observed profile, ID and contact route without private payload',()=>{
  const row=projectFacebookNativeProfile({...profile,viewer:'private'},id,base);
  assert.equal(row.follower_count,null);
  assert.equal(row.viewer,undefined);
  for(const patch of [{id:'999'},{observed_profile_url:base+'Other'},{contact_links:[source,source]},{contact_links:[base+'/reviews']}])
    assert.throws(()=>projectFacebookNativeProfile({...profile,...patch},id,base),/INVALID/);
  assert.equal(facebookNativeContactTarget(base,source),source);
  for(const url of [base+'/reviews',source+'?x=1',source+'#x',base+'Other/directory_links'])
    assert.throws(()=>facebookNativeContactTarget(base,url),/INVALID/);
  const data={id,source_url:source,observed_profile_url:base,title:profile.display_name,contact_text:'public@example.com',contact_links:[],viewer:'private'};
  assert.equal(projectFacebookNativeContact(data,id,base,source).viewer,undefined);
  assert.throws(()=>projectFacebookNativeContact({...data,id:'999'},id,base,source),/OWNER_CONFLICT field=id/);
  for(const patch of [{source_url:base+'/directory_links'},{contact_links:['https://user:secret@example.com']},{title:'bad\0name'}])
    assert.throws(()=>projectFacebookNativeContact({...data,...patch},id,base,source),/INVALID/);
});

test('native header requires exact location and owner ID, not feed or signed-in viewer',async()=>{
  const prior={location:globalThis.location,document:globalThis.document};
  const visible={getClientRects:()=>[1],closest:()=>null};
  const follower={...visible,href:base+'/followers/',getAttribute:()=>null,innerText:'1.3M followers'};
  const main={...visible,querySelectorAll:s=>s==='a[href]'?[follower]:[{...visible,innerText:profile.display_name},{...visible,innerText:profile.display_name}]};
  let scripts=[{textContent:JSON.stringify(response())}];
  try{
    globalThis.location={origin:'https://www.facebook.com',pathname:'/LinusTech/directory_contact_info',href:source};
    globalThis.document={querySelectorAll:s=>s==='[role="main"]'?[main]:scripts};
    assert.equal((await readFacebookNativeHeader(source,id,base,facebookHeaderNodes)).id,id);
    scripts=[{textContent:JSON.stringify(response('999'))}];
    await assert.rejects(()=>readFacebookNativeHeader(source,id,base,facebookHeaderNodes),/OWNER_CONFLICT field=id/);
    scripts=[{textContent:JSON.stringify(response())}];
    globalThis.location.href=base+'/directory_links';
    await assert.rejects(()=>readFacebookNativeHeader(source,id,base,facebookHeaderNodes),/INVALID/);
    globalThis.location.pathname='/checkpoint/';
    await assert.rejects(()=>readFacebookNativeHeader(source,id,base,facebookHeaderNodes),/RISK/);
    globalThis.location.pathname='/login/';
    await assert.rejects(()=>readFacebookNativeHeader(source,id,base,facebookHeaderNodes),/AUTH/);
  }finally{Object.assign(globalThis,prior);}
});

test('injected contact reader isolates native Email section from Reviews and preserves regex escapes',async()=>{
  const prior={location:globalThis.location,document:globalThis.document,getComputedStyle:globalThis.getComputedStyle,NodeFilter:globalThis.NodeFilter};
  const visible={getClientRects:()=>[1],closest:()=>null};
  const section=(title,text)=>{const heading={...visible,innerText:title};return {...visible,innerText:text,matches:()=>false,contains:node=>node===heading,querySelector:()=>null,
    querySelectorAll:selector=>selector==='h1,h2,h3,h4,[role="heading"],dt'?[heading]:[]};};
  const sections=[section('Email','Email public@example.com'),section('Reviews','Reviews unrelated@example.com')];
  const main={...visible,querySelectorAll:s=>s==='a[href]'?[{...visible,href:base+'/followers',getAttribute:()=>null,innerText:'1.3M followers'}]
    :s==='h1,[role="button"]'?[{...visible,innerText:profile.display_name}]
    :s==='section,[role="region"],[role="tabpanel"],dl,li,[role="listitem"],tr,[role="row"]'?sections:s==='a[role="tab"][aria-selected="true"]'?[{...visible,href:source}]:[]};
  let navigated;
  try{
    globalThis.location={origin:'https://www.facebook.com',pathname:'/LinusTech/directory_contact_info',href:source};
    globalThis.getComputedStyle=()=>({visibility:'visible'});
    globalThis.NodeFilter={SHOW_TEXT:4};
    globalThis.document={querySelectorAll:s=>s==='[role="main"]'?[main]:s==='script[type="application/json"][data-sjs]'?[{textContent:JSON.stringify(response())}]:[],
      createTreeWalker:scope=>{let read=false;return {currentNode:{textContent:scope.innerText.replace(/^Email\s+/,''),parentElement:scope},nextNode:()=>read?false:(read=true)};}};
    const owned={goto:async url=>{navigated=url;},evaluate:async js=>new Function(`return ${js}`)()};
    const row=await readFacebookNativeContact({}, {id,url:base,source},async(_,run)=>run(owned));
    assert.equal(navigated,source);
    assert.equal(row.contact_text,'public@example.com');
    assert.deepEqual(row.contact_links,[]);
    main.querySelectorAll=s=>s==='a[role="tab"][href]'?[{...visible,href:source},{...visible,href:base+'/reviews'}]
      :s==='a[href]'?[{...visible,href:base+'/followers',getAttribute:()=>null,innerText:'1.3M followers'}]
      :s==='h1,[role="button"]'?[{...visible,innerText:profile.display_name}]:s==='section'?sections:[];
    globalThis.location.href=base+'/about';
    globalThis.location.pathname='/LinusTech/about';
    const row2=await readFacebookNativeProfile({}, {id,url:base},async(_,run)=>run(owned));
    assert.deepEqual(row2.contact_links,[source]);
  }finally{Object.assign(globalThis,prior);}
});
