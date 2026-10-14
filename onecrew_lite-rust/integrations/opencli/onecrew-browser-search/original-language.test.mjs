import test from 'node:test';
import assert from 'node:assert/strict';
import vm from 'node:vm';
import { restoreOriginalLanguage } from './original-language.mjs';

function fixture(translated, canRestore) {
  const classes = new Set(translated ? ['translated-ltr'] : []);
  const root = {classList:{contains:key=>classes.has(key),add:key=>classes.add(key)},setAttribute(){}};
  const context = {document:{documentElement:root,querySelectorAll:()=>[]},setTimeout:callback=>callback()};
  if (canRestore) context.cr = {googleTranslate:{revert:()=>classes.delete('translated-ltr')}};
  return context;
}
test('navigation without a document root waits before checking original language', async()=>{
  const context=fixture(false,false);
  const root=context.document.documentElement;
  context.document.documentElement=null;
  let ticks=0;
  context.setTimeout=callback=>{if(++ticks===3)context.document.documentElement=root;callback();};
  assert.equal(await vm.runInNewContext('('+restoreOriginalLanguage.toString()+')()',context),true);
  assert.equal(ticks,3);
});
test('missing document root fails boundedly without inspecting or certifying content', async()=>{
  const context=fixture(false,false);
  context.document.documentElement=null;
  let ticks=0;
  context.setTimeout=callback=>{ticks++;callback();};
  context.document.querySelectorAll=()=>{throw Error('unready document must not be read');};
  await assert.rejects(vm.runInNewContext('('+restoreOriginalLanguage.toString()+')()',context),/ONECREW_ORIGINAL_TEXT_UNCONFIRMED/);
  assert.equal(ticks,20);
});
test('restores detected browser translation or stops before any translated evidence is read', async()=>{
  const code = '('+restoreOriginalLanguage.toString()+')()';
  assert.equal(await vm.runInNewContext(code,fixture(true,true)),true);
  await assert.rejects(vm.runInNewContext(code,fixture(true,false)),/ORIGINAL_TEXT_UNCONFIRMED/);
  assert.equal(await vm.runInNewContext(code,fixture(false,false)),true);
});
test('only visible exact original-text controls are activated', async()=>{
  let clicks = 0;
  const context = fixture(false,false);
  context.document.querySelectorAll=()=>[
    {textContent:'Show original',getClientRects:()=>clicks?[]:[1],click:()=>clicks++},
    {textContent:'Translate post',getClientRects:()=>{throw Error('unrelated controls must not force layout');},click:()=>{throw Error('must not translate');}},
    {textContent:'查看原文',getClientRects:()=>[],click:()=>{throw Error('hidden');}},
  ];
  await vm.runInNewContext('('+restoreOriginalLanguage.toString()+')()',context);
  assert.equal(clicks,1);
});

test('waits for asynchronous original-text restoration after clicking', async()=>{
  let pending = true;
  let ticks = 0;
  let clicks = 0;
  const context = fixture(false,false);
  context.document.querySelectorAll=()=> pending ? [{
    textContent:'查看原文',getClientRects:()=>[1],click:()=>clicks++,
  }] : [];
  context.setTimeout=callback=>{ if (++ticks === 3) pending = false; callback(); };
  assert.equal(await vm.runInNewContext('('+restoreOriginalLanguage.toString()+')()',context),true);
  assert.equal(ticks,3);
  assert.equal(clicks,1);
});

test('a click without restoration and unreverted RTL translation both stop extraction', async()=>{
  const context = fixture(false,false);
  context.document.querySelectorAll=()=> [{
    textContent:'See original',getClientRects:()=>[1],click(){},
  }];
  const code = '('+restoreOriginalLanguage.toString()+')()';
  await assert.rejects(vm.runInNewContext(code,context),/ORIGINAL_TEXT_UNCONFIRMED/);
  const rtl = fixture(false,false);
  rtl.document.documentElement.classList.add('translated-rtl');
  await assert.rejects(vm.runInNewContext(code,rtl),/ORIGINAL_TEXT_UNCONFIRMED/);
});
