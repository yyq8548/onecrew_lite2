import test from 'node:test';
import assert from 'node:assert/strict';
import {readFile} from 'node:fs/promises';
import {projectInstagramVisibleSearch,readInstagramSearch} from './instagram-search.mjs';

const receipt=JSON.parse(await readFile(new URL('../../onecrew_lite-app/tests/fixtures/platform-optimization-20261019.json',import.meta.url),'utf8')).instagram_visible_search;

test('真实可见搜索保留五个账号的页面顺序并明确身份待独立核实',()=>{
  const rows=projectInstagramVisibleSearch(receipt,5);
  assert.equal(rows.length,5);
  assert.deepEqual(rows.map(row=>row.username),receipt.map(row=>row.username));
  assert.ok(rows.every(row=>row.execution_mode==='opencli_dom'&&!('author_id' in row)));
  assert.deepEqual(projectInstagramVisibleSearch(receipt,2).map(row=>row.url),receipt.slice(0,2).map(row=>row.url));
});

test('不能把主页名称当作原生数字账号身份',()=>{
  assert.throws(()=>projectInstagramVisibleSearch([{...receipt[0],author_id:receipt[0].username}],5),/INVALID/);
  assert.throws(()=>projectInstagramVisibleSearch(receipt,21),/INVALID/);
});

test('非法请求在取得浏览器前明确失败',async()=>{
  await assert.rejects(readInstagramSearch(null,{query:'',limit:5},null),/INPUT/);
});
