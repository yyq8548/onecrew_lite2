import assert from 'node:assert/strict';
import { test } from 'node:test';
import { onlineManager } from '@tanstack/react-query';
import { createLocalQueryClient } from '../src/lib/local-query-client.ts';
import { readVerifiedRuntime } from '../../onecrew_lite-app/src/service/runtime.ts';

const directory = process.env.ONECREW_PAGE_LATENCY_DATA_DIRECTORY;
test('浏览器报告离线时，本地服务的查询与操作仍实际执行', { skip: !directory }, async () => {
  const runtime = await readVerifiedRuntime(directory!);
  const client = createLocalQueryClient(), previous = onlineManager.isOnline();
  const read = async () => {
    const response = await fetch(runtime.url + '/_local/status', { headers: { authorization: `Bearer ${runtime.control_token}` }, signal: AbortSignal.timeout(5000) });
    assert.equal(response.status, 200);
    const actual = await response.json(); assert.equal(actual.instance_id, runtime.instance_id);
    return actual;
  };
  onlineManager.setOnline(false);
  let deadline: ReturnType<typeof setTimeout> | undefined;
  try {
    await Promise.race([
      (async () => {
        await client.fetchQuery({ queryKey: ['真实本地状态'], queryFn: read });
        await client.getMutationCache().build(client, { mutationFn: read }).execute(undefined);
      })(),
      new Promise((_, reject) => { deadline = setTimeout(() => reject(Error('本地请求被浏览器联网标记暂停')), 5000); }),
    ]);
  } finally {
    clearTimeout(deadline); await client.cancelQueries(); client.clear(); onlineManager.setOnline(previous);
  }
});
