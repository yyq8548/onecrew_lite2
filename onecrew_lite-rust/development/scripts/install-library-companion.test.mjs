import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';
import {spawnSync} from 'node:child_process';
import {mkdtemp,mkdir,readFile,writeFile,rm} from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import test from 'node:test';

test('Library installer verifies before install, replays safely and rejects tampering and unsafe paths',async()=>{
  const root=await mkdtemp(path.join(os.tmpdir(),'onecrew-library-install-'));
  try {
    const source=path.join(root,'artifact');await mkdir(source);
    const name=process.platform==='win32'?'onecrew_lite.exe':'onecrew_lite';
    const bytes=Buffer.from('non-executable fixture; never launched');
    const file={path:name,size:bytes.length,sha256:createHash('sha256').update(bytes).digest('hex')};
    const manifest={version:'fixture',target:`${process.platform}-${process.arch}`,capabilities:['discovery-contacts-v2','recommendation-capture-v1'],files:[file]};
    const save=()=>writeFile(path.join(source,'manifest.json'),JSON.stringify(manifest));
    await writeFile(path.join(source,name),bytes);await save();
    const installed=path.join(root,'bin',name);
    const run=()=>spawnSync(process.execPath,['development/scripts/install-library-companion.mjs','--library-dir',source,'--install-path',installed],{encoding:'utf8',windowsHide:true});
    const required=manifest.capabilities; manifest.capabilities=[]; await save();
    assert.match(run().stderr,/companion incompatible/);
    manifest.capabilities=required; await save();
    const preflight=spawnSync(process.execPath,['development/scripts/install-library-companion.mjs','--library-dir',source,'--install-path',installed,'--check-only'],{encoding:'utf8',windowsHide:true});
    assert.equal(preflight.status,0);
    await assert.rejects(readFile(path.join(root,'bin','onecrew_lite_library',name)),{code:'ENOENT'});
    assert.equal(run().status,0);assert.equal(run().status,0);
    const binary=path.join(root,'bin','onecrew_lite_library',name);
    assert.deepEqual(await readFile(binary),bytes);
    await writeFile(path.join(source,name),'tampered');
    assert.match(run().stderr,/integrity mismatch/);
    assert.deepEqual(await readFile(binary),bytes);
    await writeFile(path.join(source,name),bytes);
    manifest.files.push({...file,path:'licenses/../../outside'});await save();
    assert.match(run().stderr,/path invalid/);
    manifest.files.pop();manifest.target='wrong-target';await save();
    assert.match(run().stderr,/target or manifest invalid/);
  } finally {await rm(root,{recursive:true,force:true});}
});
