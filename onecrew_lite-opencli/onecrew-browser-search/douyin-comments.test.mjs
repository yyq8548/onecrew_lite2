import test from 'node:test';
import assert from 'node:assert/strict';
import {projectDouyinComments,observeDouyinComments,readDouyinComments} from './douyin-comments.mjs';
import {withOwnedSearchPage} from './owned-page.mjs';
const video='7688639864627214835',author='MS4wLjABAAAA_native_author_123456789';
const source=`https://www.douyin.com/video/${video}`;
const raw=()=>({post_id:video,author_id:author,source_url:source,text:'Original caption',text_truncated:false,
  comments:['','この説明は分かりやすいです','この製品を使ってみたいです','紹介ありがとうございます','English comment'].map((text,i)=>({id:String(101+i),parent_post_id:video,text}))});
test('comments preserve the first five including blank, original caption and actual parent',()=>{
  const result=projectDouyinComments(raw(),video,author);
  assert.equal(result.comments.length,5);assert.equal(result.comments[0].text,'');
  assert.equal(result.evidence_source,'visible_same_video_comments');
  for(const mutate of [d=>d.author_id+='x',d=>d.post_id='99',d=>d.source_url+='?x=1',
    d=>d.comments[0].parent_post_id='99',d=>d.comments[0].id='position_1',d=>d.comments[1].id=d.comments[0].id,
    d=>d.comments.push({id:'106',parent_post_id:video,text:'第六条'}),d=>d.comments[0].text='語'.repeat(700)]) {
    const data=raw();mutate(data);assert.throws(()=>projectDouyinComments(data,video,author));
  }
});
function fixture() {
  const node=(tag='DIV')=>({tagName:tag,getClientRects:()=>[1],closest:()=>null,querySelectorAll:()=>[],children:[]});
  const rows=raw().comments.map(c=>{
    const body=node();body.firstElementChild=node('SPAN');body.firstElementChild.textContent=c.text;
    const more=node();more.children=[{id:`tooltip_${c.id}`}];more.parentElement={nextElementSibling:body};
    const row=node();row.parentElement={closest:()=>null};row.querySelectorAll=s=>s==='[data-e2e="video-comment-more"]'?[more]:[];
    return {row,more,body};
  });
  const sixth=rows[1].row;
  const list=node();list.querySelectorAll=()=>[...rows.map(r=>r.row),sixth];
  const link={...node('A'),href:`https://www.douyin.com/search/topic?source=comment_related_search&aweme_id=${video}&gid=${video}`};
  const panel=node();panel.querySelectorAll=s=>s==='[data-e2e="comment-list"]'?[list]:s==='a[href]'?[link]:[];list.parentElement=panel;
  const document={querySelectorAll:()=>[list],createTreeWalker:root=>{let once=false;return {nextNode:()=>!once&&(once=true),currentNode:{parentElement:root,textContent:root.textContent}};}};
  return {rows,list,link,document};
}
function dom(task) {
  const saved={document:globalThis.document,location:globalThis.location,NodeFilter:globalThis.NodeFilter};
  const f=fixture();Object.assign(globalThis,{document:f.document,location:{origin:'https://www.douyin.com',href:source},NodeFilter:{SHOW_TEXT:4}});
  try {return task(f);} finally {Object.assign(globalThis,saved);}
}
test('native panel binds both video IDs, excludes sixth/replies, retains blank position',()=>dom(f=>{
  const result=observeDouyinComments(source,video,author,raw());
  assert.equal(result.comments.length,5);assert.equal(result.comments[0].text,'');
  assert.equal(result.comments[4].id,'105');
  f.link.href=f.link.href.replace(`gid=${video}`,'gid=99');
  assert.throws(()=>observeDouyinComments(source,video,author,raw()),/INVALID/);
}));
test('missing native IDs or translated comments do not synthesize evidence',()=>dom(f=>{
  f.rows[0].more.children[0].id='random';assert.equal(observeDouyinComments(source,video,author,raw()),null);
  f.rows[0].more.children[0].id='tooltip_101';
  const original=f.rows[0].row.querySelectorAll;
  f.rows[0].row.querySelectorAll=s=>s==='button,[role="button"]'?[{getClientRects:()=>[1],closest:()=>null,textContent:'查看原文'}]:original(s);
  assert.throws(()=>observeDouyinComments(source,video,author,raw()),/ORIGINAL_TEXT_UNCONFIRMED/);
}));
test('wrong video author and duplicate panels fail without reading elsewhere',()=>dom(f=>{
  assert.throws(()=>observeDouyinComments(source,video,author+'x',raw()),/INVALID/);
  f.document.querySelectorAll=()=>[f.list,f.list];assert.equal(observeDouyinComments(source,video,author,raw()),null);
}));
test('comments command owns one tab, polls same video and closes after failure',async()=>{
  let closed=0,reads=0;const opened=[];
  const page={newTab:async url=>{opened.push(url);return 'owned';},setActivePage:async()=>{},
    evaluate:async script=>{if(script.includes('function restoreOriginalLanguage'))return null;return ++reads===1?null:{session:'private',data:raw()};},
    sleep:async()=>{},closeTab:async()=>closed++};
  const result=await readDouyinComments(page,video,author,withOwnedSearchPage);
  assert.equal(result.comments.length,5);assert.equal(reads,2);assert.deepEqual(opened,[source]);assert.equal(closed,1);
  page.evaluate=async script=>{if(script.includes('function restoreOriginalLanguage'))return null;throw new Error('ONECREW_DOUYIN_RISK');};
  await assert.rejects(readDouyinComments(page,video,author,withOwnedSearchPage),/RISK/);assert.equal(closed,2);
});
