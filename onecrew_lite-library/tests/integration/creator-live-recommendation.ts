import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { recommendationRows } from '../../src/modules/tables/recommendation-result.js';
import type { DiscoveryCreatorView, TableDto } from '../../src/modules/tables/data.js';

const path = process.env.ONECREW_LIBRARY_READBACK_EVIDENCE;
assert(path, '提供真实 Library 回读文件');
const captured = JSON.parse(await readFile(path, 'utf8')) as { table: TableDto; material: DiscoveryCreatorView };
assert.equal(captured.material.state, 'current');
const qualified = captured.material.sources.filter(source => source.verdict === 'qualified');
assert.equal(qualified.length, 1);
assert.equal(qualified[0]!.contacts.length, 0);
const rows = recommendationRows(captured.table, captured.material);
assert.equal(rows.length, 1, '原账号已经核实的作者应保留在推荐名单');
assert.equal(rows[0]!.values.name.identity?.platform, 'bilibili');
assert.equal(rows[0]!.values.name.identity?.platform_id, '3546597933714079');
assert.equal(rows[0]!.values.score.level, 'good');
assert.equal(rows[0]!.values.contacts, '');
console.log('真实 Bilibili 作者、评分与空联系方式推荐检查通过');
