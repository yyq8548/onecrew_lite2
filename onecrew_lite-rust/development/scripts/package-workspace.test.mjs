import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';
import {mkdtemp,mkdir,readFile,rm,symlink,writeFile} from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import {test} from 'node:test';
import {spawnSync} from 'node:child_process';
import {packageWorkspace} from './package-workspace.mjs';
const fingerprint=bytes=>({size:bytes.length,sha256:createHash('sha256').update(bytes).digest('hex')});
async function fixture(temp,version='0.7.1',target='aarch64-apple-darwin') {
  const cli=path.join(temp,'cli-'+version),library=path.join(temp,'library-'+version);
  await mkdir(cli);await mkdir(library);
  const binary=target.includes('windows')?'onecrew_lite.exe':'onecrew_lite';
  const bytes=Buffer.from(`#!/bin/sh
if [ "$1" = --version ]; then printf 'onecrew_lite ${version}\\n'; exit 0; fi
printf '%s\\n' "$1" >> "$HOME/calls"
if [ "$1" = plugin ] && [ "\${FAIL_PLUGIN:-0}" = 1 ]; then exit 7; fi
if [ "$1" = __bootstrap-update ]; then
  shift
  while [ "$#" -gt 0 ]; do
    if [ "$1" = --install-path ]; then /bin/cp "$0" "$2"; /bin/chmod 755 "$2"; break; fi
    shift
  done
  if [ "\${FAIL_LIBRARY:-0}" = 1 ] && [ '${version}' = 0.7.1 ]; then printf corrupt > "$HOME/.local/bin/.onecrew_lite-install/next-library/onecrew_lite"; fi
fi
`);
  await writeFile(path.join(cli,binary),bytes);await writeFile(path.join(library,binary),bytes);
  const cliManifest={schema_version:1,product:'onecrew_lite-cli',version,target,...fingerprint(bytes)};
  const libraryManifest={version:'0.1.4',target:target.includes('windows')?'win32-x64':target.startsWith('aarch64')?'darwin-arm64':'darwin-x64',files:[{path:binary,...fingerprint(bytes)}]};
  await writeFile(path.join(cli,'manifest.json'),JSON.stringify(cliManifest));await writeFile(path.join(library,'manifest.json'),JSON.stringify(libraryManifest));
  return {cli,library,bytes,libraryManifest};
}
test('packages verified separate core/client entries and rejects mismatch, overwrite and traversal',async()=>{
  const temp=await mkdtemp(path.join(os.tmpdir(),'onecrew_lite-package-'));
  try {
    const f=await fixture(temp);const out=path.join(temp,'包 with spaces');const receipt=await packageWorkspace(f.cli,f.library,out);
    for(const name of ['install-core.sh','connect-codex.sh','open-onecrew_lite.command']) assert.ok(receipt.files.some(f=>f.path===name));
    assert.doesNotMatch(await readFile(path.join(out,'install-core.sh'),'utf8'),/plugin install|opencli repair|workspace --/);
    await assert.rejects(packageWorkspace(f.cli,f.library,out),/EEXIST/);
    const windows=await fixture(temp,'0.7.2','x86_64-pc-windows-msvc');const w=await packageWorkspace(windows.cli,windows.library,path.join(temp,'windows'));
    for(const name of ['install-core.ps1','connect-codex.ps1','windows-setup-step.ps1','首次使用.cmd','02-注册服务.cmd','06-检查Library.cmd','onecrew_lite_library/onecrew_lite.exe']) assert.ok(w.files.some(f=>f.path===name));
    f.libraryManifest.target='win32-x64';await writeFile(path.join(f.library,'manifest.json'),JSON.stringify(f.libraryManifest));
    await assert.rejects(packageWorkspace(f.cli,f.library,path.join(temp,'bad-target')),/targets must match/);
    f.libraryManifest.target='darwin-arm64';f.libraryManifest.files.push({path:'../escape',...fingerprint(f.bytes)});await writeFile(path.join(f.library,'manifest.json'),JSON.stringify(f.libraryManifest));
    await assert.rejects(packageWorkspace(f.cli,f.library,path.join(temp,'bad-path')),/Invalid Library manifest path/);
    f.libraryManifest.files.pop();await writeFile(path.join(f.library,'manifest.json'),JSON.stringify(f.libraryManifest));
    await writeFile(path.join(f.library,'onecrew_lite'),'corrupt');await assert.rejects(packageWorkspace(f.cli,f.library,path.join(temp,'bad-hash')),/integrity mismatch/);
    await rm(path.join(f.library,'onecrew_lite'));await symlink(path.join(f.cli,'onecrew_lite'),path.join(f.library,'onecrew_lite'));
    await assert.rejects(packageWorkspace(f.cli,f.library,path.join(temp,'bad-link')),/regular file/);
  } finally {await rm(temp,{recursive:true,force:true});}
});
test('Mac core install, replay, rollback and independent Codex repair preserve data', {skip:process.platform!=='darwin'},async()=>{
  const temp=await mkdtemp(path.join(os.tmpdir(),'onecrew_lite-install-'));
  try {
    const target=process.arch==='arm64'?'aarch64-apple-darwin':'x86_64-apple-darwin';
    const old=await fixture(temp,'0.7.0',target),next=await fixture(temp,'0.7.1',target);
    const before=path.join(temp,'old package'),after=path.join(temp,'new package');await packageWorkspace(old.cli,old.library,before);await packageWorkspace(next.cli,next.library,after);
    const home=path.join(temp,'客户 home');await mkdir(path.join(home,'.local/bin'),{recursive:true});await writeFile(path.join(home,'.local/bin/onecrew'),'preserve Pro');await writeFile(path.join(home,'business-data'),'preserve data');
    const run=(dir,name,extra={})=>spawnSync('/bin/sh',[path.join(dir,name)],{encoding:'utf8',env:{...process.env,HOME:home,...extra},timeout:10000});
    const initial=run(before,'install-core.sh');assert.equal(initial.status,0,initial.stderr);
    assert.equal(await readFile(path.join(home,'calls'),'utf8'),'__bootstrap-update\n');
    assert.equal(run(before,'install-core.sh').status,0);
    const failed=run(after,'install-core.sh',{FAIL_LIBRARY:'1'});assert.equal(failed.status,1);assert.match(failed.stderr,/previous files restored/);
    assert.deepEqual(await readFile(path.join(home,'.local/bin/onecrew_lite')),old.bytes);
    assert.deepEqual(await readFile(path.join(home,'.local/bin/onecrew_lite_library/onecrew_lite')),old.bytes);
    const updated=run(after,'install-core.sh');assert.equal(updated.status,0,updated.stderr);
    assert.deepEqual(await readFile(path.join(home,'.local/bin/onecrew_lite')),next.bytes);
    assert.equal(run(after,'connect-codex.sh',{FAIL_PLUGIN:'1'}).status,7);
    assert.deepEqual(await readFile(path.join(home,'.local/bin/onecrew_lite')),next.bytes);
    assert.equal(run(after,'connect-codex.sh').status,0);
    await mkdir(path.join(home,'.local/bin/.onecrew_lite-install'));
    assert.equal(run(after,'install-core.sh').status,1);
    await writeFile(path.join(after,'onecrew_lite'),'tampered');assert.equal(run(after,'install-core.sh').status,1);
    assert.equal(await readFile(path.join(home,'.local/bin/onecrew'),'utf8'),'preserve Pro');assert.equal(await readFile(path.join(home,'business-data'),'utf8'),'preserve data');
  } finally {await rm(temp,{recursive:true,force:true});}
});
