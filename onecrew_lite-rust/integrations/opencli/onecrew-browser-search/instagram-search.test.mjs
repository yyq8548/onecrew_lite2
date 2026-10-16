import test from 'node:test';
import assert from 'node:assert/strict';
import {readFile} from 'node:fs/promises';
import {parseInstagramSearchCapture, projectInstagramSearch} from './instagram-search.mjs';

const capture = JSON.parse(await readFile(new URL('./instagram-search.capture.json', import.meta.url), 'utf8'));
const entry = {
  url: capture.url,
  method: capture.method,
  requestBodyPreview: new URLSearchParams({
    fb_api_req_friendly_name: capture.operation,
    variables: JSON.stringify(capture.variables),
  }).toString(),
  responseStatus: capture.status,
  responsePreview: JSON.stringify(capture.response),
};

test('真实网页 GraphQL 响应保留原生账号绑定和返回顺序', () => {
  const payload = parseInstagramSearchCapture(entry, 'makeup artist');
  const rows = projectInstagramSearch(payload, 5);
  assert.deepEqual(rows.map(row => [row.author_id, row.username]), [
    ['4329622652', 'maribo003'], ['145399002', 'delinamedhin'], ['23379226', 'patrickta'],
    ['14531636133', 'jedimakeupartist1'], ['397949184', 'melissa.hurkman'],
  ]);
  assert.equal(rows[2].url, 'https://www.instagram.com/patrickta/');
  assert.ok(rows.every(row => !('followers' in row)));
});

test('另一组关键词不能使用已捕获的美妆搜索响应', () => {
  assert.equal(parseInstagramSearchCapture(entry, 'travel'), null);
  assert.equal(parseInstagramSearchCapture(entry, 'instagram'), null);
});

test('返回上限只截取真实响应中的前两个账号', () => {
  const rows = projectInstagramSearch(parseInstagramSearchCapture(entry, 'makeup artist'), 2);
  assert.deepEqual(rows.map(row => row.author_id), ['4329622652', '145399002']);
});
