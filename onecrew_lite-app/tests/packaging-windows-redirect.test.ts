import assert from 'node:assert/strict';
import { test } from 'node:test';
import { readFile, realpath } from 'node:fs/promises';
import { dirname, join, resolve } from 'node:path';
import { materializePayload, PayloadPermissions } from '../src/packaging/payload-cache.ts';
import { parsePayloadManifest } from '../src/packaging/payload-manifest.ts';
import { loadMcpAppResources } from '../src/service/mcp-app-resources.ts';

const executable = process.env.ONECREW_SEA_CANDIDATE_EXECUTABLE;
test('real Windows installed payload resolves redirected files to one physical resource root', { skip: process.platform !== 'win32' || !executable, timeout: 120_000 }, async () => {
  const candidate = dirname(resolve(executable!));
  const manifest = parsePayloadManifest(await readFile(join(candidate, 'runtime-manifest.json')));
  const base = join(process.env.LOCALAPPDATA!, 'OneCrewLite', 'cache', 'runtime');
  const script = await readFile(join(candidate, 'payload/resources/storage/private-backup.ps1'));
  const assets = new Map<string, Uint8Array>();
  for (const file of manifest.files) assets.set(file.path, await readFile(join(candidate, 'payload', file.path)));
  const root = await materializePayload(manifest, path => { const bytes = assets.get(path); assert.ok(bytes); return bytes; }, base, new PayloadPermissions(script));
  assert.equal(dirname(await realpath(join(root, '.complete'))), root);
  const resources = await loadMcpAppResources(join(root, 'resources/mcp'));
  assert.ok(resources.list().resources.length);
});
