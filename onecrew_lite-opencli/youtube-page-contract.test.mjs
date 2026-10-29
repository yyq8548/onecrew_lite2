import assert from 'node:assert/strict';
import { createRequire } from 'node:module';
import test from 'node:test';
import { readVideoEvidence, readWatchPage } from './onecrew-youtube/evidence.mjs';

const { parseExpressionAt } = createRequire(new URL('../onecrew_lite-app/package.json', import.meta.url))('acorn');
const syntax = fn => parseExpressionAt(`(${fn.toString()})`, 0, { ecmaVersion: 'latest' });

function* nodes(value) {
  if (Array.isArray(value)) {
    for (const child of value) yield* nodes(child);
  } else if (value && typeof value === 'object') {
    if (typeof value.type === 'string') yield value;
    for (const child of Object.values(value)) yield* nodes(child);
  }
}

test('YouTube request contract keeps one fixed-origin bounded fetch and projects its response', () => {
  const tree = syntax(readWatchPage);
  const all = [...nodes(tree)];
  const requests = all.filter(node => node.type === 'CallExpression' && node.callee.name === 'fetch');
  assert.equal(requests.length, 1);
  const [url, options] = requests[0].arguments;
  assert.equal(url.operator, '+');
  assert.equal(url.left.value, '/watch?v=');
  assert.equal(url.right.callee.name, 'encodeURIComponent');
  assert.equal(url.right.arguments[0].name, 'videoId');
  const fields = Object.fromEntries(options.properties.map(property => [property.key.name, property.value]));
  assert.equal(fields.credentials.value, 'include');
  assert.equal(fields.redirect.value, 'error');
  assert.equal(fields.signal.callee.object.name, 'AbortSignal');
  assert.equal(fields.signal.callee.property.name, 'timeout');
  assert.equal(fields.signal.arguments[0].value, 20000);
  const originGuard = tree.body.body[0];
  assert.equal(originGuard.test.left.object.name, 'location');
  assert.equal(originGuard.test.left.property.name, 'origin');
  assert.equal(originGuard.test.right.value, 'https://www.youtube.com');
  assert.equal(originGuard.consequent.argument.arguments[0].value, 'ONECREW_ORIGIN_MISMATCH');
  const oversized = all.find(node => node.type === 'IfStatement'
    && node.consequent.type === 'ThrowStatement'
    && node.consequent.argument.arguments[0]?.value === 'ONECREW_WATCH_TOO_LARGE');
  assert.equal(oversized.test.left.name, 'bytes');
  const projected = all.filter(node => node.type === 'ReturnStatement');
  assert.equal(projected.length, 1);
  assert.equal(projected[0].argument.callee.name, 'project');
  assert.equal(projected[0].argument.arguments[1].name, 'videoId');
  const cleanup = all.find(node => node.type === 'TryStatement').finalizer;
  assert.ok([...nodes(cleanup)].some(node => node.type === 'AwaitExpression'
    && node.argument.callee.object.name === 'reader' && node.argument.callee.property.name === 'cancel'));
});

test('YouTube awaits readiness and original-language restoration before extraction and propagates guard errors', () => {
  const tree = syntax(readVideoEvidence);
  const all = [...nodes(tree)];
  const awaited = all.filter(node => node.type === 'AwaitExpression').map(node => node.argument);
  assert.deepEqual(awaited.map(call => call.callee.property.name), ['goto', 'evaluate', 'evaluate', 'evaluate']);
  assert.equal(awaited[0].arguments[0].value, 'https://www.youtube.com');
  assert.deepEqual(awaited.slice(1).map(call => [...nodes(call.arguments[0])]
    .filter(node => node.type === 'CallExpression' && node.callee.property?.name === 'toString')
    .map(node => node.callee.object.name)), [
    ['waitForYoutubePage'], ['restoreOriginalLanguage'], ['readWatchPage', 'projectWatchHtml'],
  ]);
  assert.equal(all.some(node => node.type === 'TryStatement' || node.type === 'CatchClause'), false);
});
