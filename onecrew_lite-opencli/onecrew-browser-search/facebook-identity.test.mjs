import test from 'node:test';
import assert from 'node:assert/strict';
import {facebookVanitySource,projectFacebookIdentity,facebookHeaderNodes,readFacebookIdentityPage,readFacebookIdentity} from './facebook-identity.mjs';
const source='https://www.facebook.com/LinusTech';
const node={id:'100044572203205',display_name:'Linus Tech Tips',profile_url:source};
const response=(id=node.id,url=source)=>({__bbox:{result:{data:{user:{profile_header_renderer:{user:{__typename:'User',id,name:node.display_name,url}}}}}}});

test('only saved vanity profile sources are accepted before navigation',async()=>{
  assert.equal(facebookVanitySource(source+'/'),source);
  for(const url of ['https://www.facebook.com/search','https://www.facebook.com/LinusTech/posts/123',
    source+'?x=1',source+'#x','https://evil.example/LinusTech','https://user:secret@www.facebook.com/LinusTech',
    'https://www.facebook.com/123','https://www.facebook.com/checkpoint','https://www.facebook.com/login.php',source+'//']) {
    let navigated=false;
    await assert.rejects(()=>readFacebookIdentity({},{url},async()=>{navigated=true;}),/INPUT/);
    assert.equal(navigated,false);
  }
});

test('only the observed native profile-header subtree supplies identity',()=>{
  const payload={require:[response()],feed:[{__typename:'User',id:'999',name:node.display_name,url:source}],viewer:{id:'888'},cookie:'private'};
  assert.deepEqual(facebookHeaderNodes(payload),[node]);
  assert.deepEqual(facebookHeaderNodes({__bbox:{result:{data:{actor:{__typename:'User',id:node.id,name:node.display_name,url:source}}}}}),[]);
});

test('identity projection excludes private payload and rejects another profile',()=>{
  const value=projectFacebookIdentity({...node,source_url:source,cookie:'private',viewer_id:'888'},source);
  assert.deepEqual(value,{adapter_version:'1.0.0',source_url:source,...node,evidence_source:'native_profile_header_response'});
  for(const change of [{id:'LinusTech'},{profile_url:'https://www.facebook.com/AnotherPage'},
    {source_url:'https://www.facebook.com/AnotherPage'},{display_name:'bad\0name'}])
    assert.throws(()=>projectFacebookIdentity({...node,source_url:source,...change},source),/INVALID/);
});

test('public native ID must agree with visible same-profile header; conflicts and drift fail',async()=>{
  const prior={location:globalThis.location,document:globalThis.document};
  const visible={getClientRects:()=>[1],closest:()=>null};
  const name={...visible,innerText:node.display_name};
  const owner={...visible,href:source+'/followers/',getAttribute:()=>null};
  const tab={...visible,href:source+'/followers',getAttribute:()=> 'tab'};
  const names=[name];
  const main={...visible,querySelectorAll:s=>s==='a[href]'?[owner,tab]:names};
  let scripts=[{textContent:JSON.stringify(response())}];
  try{
    globalThis.location={origin:'https://www.facebook.com',pathname:'/LinusTech',href:source};
    globalThis.document={querySelectorAll:s=>s==='[role="main"]'?[main]:scripts};
    assert.deepEqual(await readFacebookIdentityPage(source,source,facebookHeaderNodes),{...node,source_url:source});
    // TechNave renders two matching header buttons; both refer to the same
    // native owner. Repeated presentation is not conflicting identity.
    names.push({...name});
    assert.deepEqual(await readFacebookIdentityPage(source,source,facebookHeaderNodes),{...node,source_url:source});
    scripts=[...scripts,{textContent:JSON.stringify(response('999'))}];
    await assert.rejects(()=>readFacebookIdentityPage(source,source,facebookHeaderNodes),/INVALID/);
    scripts=[{textContent:JSON.stringify(response())}];
    globalThis.location.href='https://www.facebook.com/AnotherPage';
    await assert.rejects(()=>readFacebookIdentityPage(source,source,facebookHeaderNodes),/INVALID/);
    globalThis.location.pathname='/login/';
    await assert.rejects(()=>readFacebookIdentityPage(source,source,facebookHeaderNodes),/AUTH/);
    globalThis.location.pathname='/checkpoint/';
    await assert.rejects(()=>readFacebookIdentityPage(source,source,facebookHeaderNodes),/RISK/);
  }finally{globalThis.location=prior.location;globalThis.document=prior.document;}
});
