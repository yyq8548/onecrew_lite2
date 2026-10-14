import test from 'node:test';
import assert from 'node:assert/strict';
import {facebookOriginalNodes,readFacebookNativeOriginalPage,readFacebookNativeTargets} from './facebook-native-original.mjs';
const id='123',post='456',base='https://www.facebook.com/Creator',source=`https://www.facebook.com/reel/${post}`;
const story={attachments:[{media:{__typename:'Video',id:post,owner:{id,url:base}}}],actors:[{__typename:'User',id,name:'Creator',url:base}],is_reshare:false,message:{text:'Original English caption'},translated_message_for_viewer:{text:'翻译内容'}};
const response=s=>({__bbox:{result:{data:s}}});
test('only exact video-story actor and media owner produce native original evidence',()=>{
  const rows=facebookOriginalNodes({require:[response(story)],viewer:{id:'999'},feed:{...story}},post);
  assert.equal(rows.length,1);assert.equal(rows[0].text,'Original English caption');assert.equal(rows[0].viewer,undefined);
  assert.deepEqual(facebookOriginalNodes(response({...story,attachments:[{media:{...story.attachments[0].media,owner:{id:'999',url:base}}}]}),post),[]);
  assert.deepEqual(facebookOriginalNodes(response(story),'999'),[]);
});
test('native targets require same profile and selected originals; links are bounded and deduplicated',async()=>{
  const prior={location:globalThis.location,document:globalThis.document};
  const visible={getClientRects:()=>[1],closest:()=>null};
  const owner={...visible,href:base+'/followers',getAttribute:()=>null,innerText:'10 followers'};
  const name={...visible,innerText:'Creator'};
  const reel={...visible,href:source+'/?s=fb_shorts_profile&stack_idx=0',getAttribute:()=>null};
  let originals=true;
  const main={...visible,querySelectorAll:s=>s==='a[href]'?[owner,reel,reel,{...reel,href:'https://evil.example/reel/123/?s=fb_shorts_profile'}]:
    s==='h1,[role="button"]'?[name]:[{...visible,href:base+'/reels_tab'},...(originals?[{...visible,href:base+'/owner_reels'}]:[])]};
  const header={__bbox:{result:{data:{user:{profile_header_renderer:{user:{__typename:'User',id,name:'Creator',url:base}}}}}}};
  let scripts=[{textContent:JSON.stringify(header)},...Array.from({length:378},()=>({textContent:'{}'}))];
  try{
    globalThis.document={querySelectorAll:s=>s==='[role="main"]'?[main]:scripts};
    const page={goto:async()=>{globalThis.location={origin:'https://www.facebook.com',pathname:'/Creator/reels/',href:base+'/reels/'};},evaluate:async js=>(0,eval)(js)};
    assert.deepEqual(await readFacebookNativeTargets(page,{id,url:base},async(_,fn)=>fn(page)),[{author_id:id,source_url:source}]);
    scripts.push(...Array.from({length:134},()=>({textContent:'{}'})));
    await assert.rejects(()=>readFacebookNativeTargets(page,{id,url:base},async(_,fn)=>fn(page)),/PROFILE_INVALID/);
    scripts=scripts.slice(0,379);
    originals=false;
    await assert.rejects(()=>readFacebookNativeTargets(page,{id,url:base},async(_,fn)=>fn(page)),/RECENT_INVALID/);
  }finally{Object.assign(globalThis,prior);}
});
test('native original binds visible player owner; blank, reshare, drift and translated-only evidence fail',async()=>{
  const prior={location:globalThis.location,document:globalThis.document,innerWidth:globalThis.innerWidth,innerHeight:globalThis.innerHeight};
  const visible={getBoundingClientRect:()=>({width:100,height:100,top:0,bottom:100,left:0,right:100})};
  const link={href:`https://www.facebook.com/profile.php?id=${id}&sk=reels_tab`,innerText:'Creator'};
  const player={...visible,querySelectorAll:()=>[link]};let current=story;
  try{
    Object.assign(globalThis,{location:{origin:'https://www.facebook.com',pathname:'/reel/456',href:source,search:'',hash:''},innerWidth:1000,innerHeight:1000,
      document:{querySelectorAll:s=>s.includes('script')?[{textContent:JSON.stringify(response(current))}]:[player]}});
    assert.deepEqual(await readFacebookNativeOriginalPage(source,id,base,facebookOriginalNodes),{post_id:post,author_id:id,source_url:source,text:'Original English caption',text_truncated:false});
    for(const patch of [{message:null},{message:{text:' '}},{is_reshare:true},{actors:[{__typename:'User',id,name:'Creator',url:base+'Other'}],attachments:[{media:{__typename:'Video',id:post,owner:{id,url:base+'Other'}}}]}]){
      current={...story,...patch};await assert.rejects(()=>readFacebookNativeOriginalPage(source,id,base,facebookOriginalNodes),/INVALID|UNAVAILABLE/);
    }
    current=story;link.href='https://www.facebook.com/profile.php?id=999';
    await assert.rejects(()=>readFacebookNativeOriginalPage(source,id,base,facebookOriginalNodes),/INVALID/);
  }finally{Object.assign(globalThis,prior);}
});
