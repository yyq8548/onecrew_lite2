import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { mkdtemp, mkdir, readFile, rm, symlink, writeFile } from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import { test } from 'node:test';
import { spawnSync } from 'node:child_process';
import { packageWorkspace } from './package-workspace.mjs';

test('workspace packaging verifies both artifacts and refuses mismatches or overwrite', async (t) => {
  const temp = await mkdtemp(path.join(os.tmpdir(), 'onecrew_lite-workspace-package-'));
  try {
    const cli = path.join(temp, 'cli');
    const library = path.join(temp, 'onecrew_lite_library');
    await mkdir(cli);
    await mkdir(library);
    const bytes = Buffer.from('verified fixture executable');
    const expected = { size: bytes.length, sha256: createHash('sha256').update(bytes).digest('hex') };
    await writeFile(path.join(cli, 'onecrew_lite'), bytes);
    await writeFile(path.join(library, 'onecrew_lite'), bytes);
    const cliManifest = { schema_version: 1, product: 'onecrew_lite-cli', version: '0.6.28',
      target: 'aarch64-apple-darwin', ...expected };
    const libraryManifest = { version: '0.1.3', target: 'darwin-arm64',
      files: [{ path: 'onecrew_lite', ...expected }] };
    await writeFile(path.join(cli, 'manifest.json'), JSON.stringify(cliManifest));
    const saveLibrary = () => writeFile(path.join(library, 'manifest.json'), JSON.stringify(libraryManifest));
    await saveLibrary();
    const output = path.join(temp, 'workspace with spaces');
    const receipt = await packageWorkspace(cli, library, output);
    assert.equal(receipt.product, 'onecrew_lite-workspace');
    assert.equal(receipt.files.filter(file => file.path.endsWith('/onecrew_lite') || file.path === 'onecrew_lite').length, 2);
    assert.ok(receipt.files.some(file => file.path === '安装并打开 onecrew_lite.command'));
    assert.ok(receipt.files.some(file => file.path === '首次安装 onecrew_lite.command'));
    await assert.rejects(packageWorkspace(cli, library, output), /EEXIST/);
    await writeFile(path.join(cli, 'onecrew_lite.exe'), bytes);
    await writeFile(path.join(library, 'onecrew_lite.exe'), bytes);
    await writeFile(path.join(cli, 'manifest.json'), JSON.stringify({...cliManifest,target:'x86_64-pc-windows-gnu'}));
    await writeFile(path.join(library, 'manifest.json'), JSON.stringify({...libraryManifest,target:'win32-x64',files:[{path:'onecrew_lite.exe',...expected}]}));
    const windowsReceipt=await packageWorkspace(cli,library,path.join(temp,'windows-gnu'));
    assert.ok(windowsReceipt.files.some(file=>file.path==='onecrew_lite_library/onecrew_lite.exe'));
    assert.ok(windowsReceipt.files.some(file=>file.path==='首次使用.cmd'));
    await writeFile(path.join(cli, 'manifest.json'), JSON.stringify(cliManifest));
    libraryManifest.target = 'win32-x64';
    await saveLibrary();
    await assert.rejects(packageWorkspace(cli, library, path.join(temp, 'wrong-target')), /targets must match/);
    libraryManifest.target = 'darwin-arm64';
    libraryManifest.files.push({ path: '../cli/onecrew_lite', ...expected });
    await saveLibrary();
    await assert.rejects(packageWorkspace(cli, library, path.join(temp, 'unsafe-path')), /Invalid Library manifest path/);
    libraryManifest.files.pop();
    await saveLibrary();
    await writeFile(path.join(library, 'onecrew_lite'), 'tampered');
    await assert.rejects(packageWorkspace(cli, library, path.join(temp, 'bad-hash')), /integrity mismatch/);
    await rm(path.join(library, 'onecrew_lite'));
    try {
      await symlink(path.join(cli, 'onecrew_lite'), path.join(library, 'onecrew_lite'));
      await assert.rejects(packageWorkspace(cli, library, path.join(temp, 'symlink')), /regular file/);
    } catch (error) {
      if (process.platform !== 'win32' || error.code !== 'EPERM') throw error;
      t.diagnostic('Windows symlink privilege unavailable; symlink lane not run');
    }
  } finally {
    await rm(temp, { recursive: true, force: true });
  }
});

test('Mac double-click entries work with spaces, preserve failures, and separate launch from plugin install', { skip: process.platform === 'win32' }, async () => {
  const temp = await mkdtemp(path.join(os.tmpdir(), 'onecrew-mac-launch-'));
  try {
    const cli = path.join(temp, 'cli');
    const library = path.join(temp, 'onecrew_lite_library');
    await mkdir(cli);
    await mkdir(library);
    const bytes = Buffer.from(`#!/bin/sh
printf '%s\\n' "$1" >> "$HOME/calls"
if [ "\${ONECREW_FIXTURE_EXIT:-0}" != 0 ] && { [ -z "\${ONECREW_FIXTURE_FAIL_AT:-}" ] || [ "$1" = "$ONECREW_FIXTURE_FAIL_AT" ]; }; then exit "$ONECREW_FIXTURE_EXIT"; fi
if [ "$1" = __bootstrap-update ]; then
  shift
  while [ "$#" -gt 0 ]; do
    if [ "$1" = --install-path ]; then /bin/cp "$0" "$2"; /bin/chmod 755 "$2"; break; fi
    shift
  done
fi
`);
    const expected = { size: bytes.length, sha256: createHash('sha256').update(bytes).digest('hex') };
    await writeFile(path.join(cli, 'onecrew_lite'), bytes);
    await writeFile(path.join(library, 'onecrew_lite'), bytes);
    await writeFile(path.join(cli, 'manifest.json'), JSON.stringify({ schema_version: 1,
      product: 'onecrew_lite-cli', version: '0.6.28', target: 'aarch64-apple-darwin', ...expected }));
    await writeFile(path.join(library, 'manifest.json'), JSON.stringify({ version: '0.1.3',
      target: 'darwin-arm64', files: [{ path: 'onecrew_lite', ...expected }] }));
    const output = path.join(temp, 'onecrew_lite 同事 测试');
    const home = path.join(temp, '客户 主目录');
    await mkdir(home);
    const oldData = path.join(home, 'Library/Application Support/io.onecrewai.onecrew');
    const liteData = path.join(home, 'Library/Application Support/io.onecrewai.onecrew_lite');
    for (const dir of [oldData, liteData]) {
      await mkdir(dir, { recursive: true });
      await writeFile(path.join(dir, 'keep-fixture.txt'), 'existing customer data');
    }
    await mkdir(path.join(home, '.local/bin'), { recursive: true });
    await writeFile(path.join(home, '.local/bin/onecrew'), 'old advanced edition');
    await packageWorkspace(cli, library, output);
    const run = (name, code, failAt = '') => spawnSync('/bin/sh', [path.join(output, name)], {
      cwd: temp, encoding: 'utf8', env: { ...process.env, HOME: home,
        ONECREW_FIXTURE_EXIT: String(code), ONECREW_FIXTURE_FAIL_AT: failAt },
      timeout: 5000,
    });
    assert.equal(run('安装并打开 onecrew_lite.command', 0).status, 0);
    assert.equal(run('连接 Codex.command', 0).status, 0);
    const failedOpen = run('安装并打开 onecrew_lite.command', 7);
    assert.equal(failedOpen.status, 1);
    assert.match(failedOpen.stderr, /安装或启动未完成/);
    const failedConnect = run('连接 Codex.command', 7);
    assert.equal(failedConnect.status, 1);
    assert.match(failedConnect.stderr, /连接未完成/);
    assert.equal(await readFile(path.join(home, 'calls'), 'utf8'),
      '__bootstrap-update\nworkspace\nplugin\n__bootstrap-update\nplugin\n');
    const firstInstall = run('首次安装 onecrew_lite.command', 0);
    assert.equal(firstInstall.status, 0);
    assert.match(firstInstall.stdout, /完全退出并重新打开 Codex/);
    assert.match(firstInstall.stdout, /安装窗口本身不能生成卡片/);
    const failedFirstInstall = run('首次安装 onecrew_lite.command', 7);
    assert.equal(failedFirstInstall.status, 1);
    assert.doesNotMatch(failedFirstInstall.stdout, /看到聊天状态卡/);
    const failedPlugin = run('首次安装 onecrew_lite.command', 7, 'plugin');
    assert.equal(failedPlugin.status, 1);
    assert.match(failedPlugin.stderr, /连接未完成/);
    assert.doesNotMatch(failedPlugin.stdout, /看到聊天状态卡/);
    const beforeComponentFailure = await readFile(path.join(home, 'calls'), 'utf8');
    const failedComponents = run('首次安装 onecrew_lite.command', 7, 'opencli');
    assert.equal(failedComponents.status, 1);
    assert.doesNotMatch(failedComponents.stdout, /看到聊天状态卡/);
    assert.equal(await readFile(path.join(home, 'calls'), 'utf8'),
      beforeComponentFailure + '__bootstrap-update\nworkspace\nopencli\n');
    const callsBeforeTamper = await readFile(path.join(home, 'calls'), 'utf8');
    assert.equal(callsBeforeTamper,
      '__bootstrap-update\nworkspace\nplugin\n__bootstrap-update\nplugin\n' +
      '__bootstrap-update\nworkspace\nopencli\nplugin\n__bootstrap-update\n' +
      '__bootstrap-update\nworkspace\nopencli\nplugin\n' +
      '__bootstrap-update\nworkspace\nopencli\n');
    for (const dir of [oldData, liteData])
      assert.equal(await readFile(path.join(dir, 'keep-fixture.txt'), 'utf8'), 'existing customer data');
    assert.equal(await readFile(path.join(home, '.local/bin/onecrew'), 'utf8'), 'old advanced edition');
    await writeFile(path.join(output, 'onecrew_lite_library/onecrew_lite'), 'tampered');
    const tampered = run('安装并打开 onecrew_lite.command', 0);
    assert.equal(tampered.status, 1);
    assert.match(tampered.stderr, /校验失败/);
    assert.equal(await readFile(path.join(home, 'calls'), 'utf8'), callsBeforeTamper);
  } finally {
    await rm(temp, { recursive: true, force: true });
  }
});
