import test from 'node:test';
import assert from 'node:assert/strict';
import {readRecentPage,readTikTokRecent} from './tiktok-language-recent.mjs';

test('recent locators come only from the verified profile list and stop at three unique posts',async()=>{
  const oldDocument=globalThis.document,oldLocation=globalThis.location;
  try {
    globalThis.location={href:'https://www.tiktok.com/@maker'};
    globalThis.document={querySelectorAll:()=>[{querySelectorAll:()=>[
      '/@maker/video/1','/@maker/video/1','/@other/video/2','/@maker/photo/3','/@maker/video/4','/@maker/video/5','/@maker/video/6'
    ].map(href=>({href}))}]};
    let checked=false;
    const rows=await readRecentPage('maker','42',(name,id)=>{assert.equal(name,'maker');assert.equal(id,'42');checked=true;},()=>{});
    assert.equal(checked,true);
    assert.deepEqual(rows.map(r=>r.source_url),[1,4,5].map(id=>`https://www.tiktok.com/@maker/video/${id}`));
    await assert.rejects(readRecentPage('maker','42',()=>{throw new Error('author mismatch');},()=>{}),/author mismatch/);
  } finally {globalThis.document=oldDocument;globalThis.location=oldLocation;}
});

test('invalid identity never opens a browser',async()=>{
  await assert.rejects(readTikTokRecent({},'../other','42',()=>assert.fail('browser accessed')),/INPUT/);
  await assert.rejects(readTikTokRecent({},'maker','0',()=>assert.fail('browser accessed')),/INPUT/);
});

test('failed-list retry is requested outside the page context, never across a verification gate',async()=>{
  const oldDocument=globalThis.document,oldLocation=globalThis.location;
  try {
    let clicks=0;
    globalThis.location={href:'https://www.tiktok.com/@maker'};
    const button={textContent:'Refresh',getClientRects:()=>[{}],click:()=>{clicks++;}};
    globalThis.document={body:{innerText:'Something went wrong. Sorry about that! Please try again later.'},
      querySelectorAll:selector=>selector==='button'?[button]:clicks?[{querySelectorAll:()=>[{href:'/@maker/video/123'}]}]:[]};
    assert.deepEqual(await readRecentPage('maker','42',()=>{},()=>{}),{retry_list:true});
    assert.equal(clicks,0);
    globalThis.document.body.innerText='verify you are human Something went wrong Please try again later';
    await assert.rejects(readRecentPage('maker','42',()=>{},()=>{}),/RISK/);
    assert.equal(clicks,0);
  } finally {globalThis.document=oldDocument;globalThis.location=oldLocation;}
});

test('failed-list navigation retries once and preserves the exact target',async()=>{
  const visits=[];
  const page={goto:async url=>visits.push(url),evaluate:async()=>({retry_list:true})};
  await assert.rejects(readTikTokRecent(page,'maker','42',(_p,run)=>run(page)),/failed_list/);
  assert.deepEqual(visits,['https://www.tiktok.com/@maker','https://www.tiktok.com/@maker']);
});
