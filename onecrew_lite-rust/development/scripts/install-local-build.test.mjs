import test from 'node:test';
import assert from 'node:assert/strict';
import {mkdtempSync,mkdirSync,writeFileSync,readFileSync,chmodSync,rmSync,existsSync} from 'node:fs';
import {tmpdir} from 'node:os';
import path from 'node:path';
import {createHash} from 'node:crypto';
import {spawnSync} from 'node:child_process';

const installer=readFileSync(new URL('./install-local-build.sh',import.meta.url),'utf8');
for (const fail of ['preflight','bootstrap','library','web','plugin','journal','none']) test(`installation fault: ${fail}`,()=>{
  const root=mkdtempSync(path.join(tmpdir(),'onecrew-install-test-'));
  const put=(name,text)=>{const file=path.join(root,name);mkdirSync(path.dirname(file),{recursive:true});writeFileSync(file,text);chmodSync(file,0o755);return file;};
  try {
    const script=put('development/scripts/install-local-build.sh',installer);
    put('development/release/version.json',JSON.stringify({cliPluginBase:'0.6.88'}));
    put('development/scripts/install-library-companion.mjs','process.exit(process.env.INJECT_FAILURE==="library"&&!process.argv.includes("--check-only")?1:0);');
    put('manifest.json',JSON.stringify({version:'0.1.4',target:'linux-x64'}));
    put('mock-bin/rustc','#!/bin/sh\necho "host: x86_64-unknown-linux-gnu"\n');
    const candidate=`#!/bin/bash
case "$1" in
 --version) echo 'onecrew_lite 0.6.88';;
 __service-preflight) [[ "$INJECT_FAILURE" != preflight ]];;
 __bootstrap-update) [[ "$INJECT_FAILURE" != bootstrap ]] || exit 1; shift; while [[ $# -gt 0 ]]; do if [[ "$1" == --install-path ]]; then cp "$0" "$2"; break; fi; shift; done;;
 web) [[ "$INJECT_FAILURE" != web ]] || exit 1; echo 'http://127.0.0.1:27206';;
 plugin) [[ "$INJECT_FAILURE" != plugin ]];;
 *) exit 2;;
esac
`;
    const artifact='dist/onecrew_lite/debug/0.6.88/x86_64-unknown-linux-gnu/';
    put(artifact+'onecrew_lite',candidate);
    put(artifact+'manifest.json',JSON.stringify({product:'onecrew_lite-cli',version:'0.6.88',target:'x86_64-unknown-linux-gnu',sha256:createHash('sha256').update(candidate).digest('hex'),size:Buffer.byteLength(candidate)}));
    const installed=put('installed/onecrew_lite','original bytes');
    if(fail==='journal') put('installed/.onecrew-upgrade.json',JSON.stringify({schema_version:999,phase:'unknown'}));
    const bash=process.platform==='win32'?'C:/Program Files/Git/bin/bash.exe':'bash';
    const result=spawnSync(bash,[script,'--target','x86_64-unknown-linux-gnu','--debug','--install-path',installed,'--library-dir',root,'--plugin-install'],{encoding:'utf8',timeout:30000,env:{...process.env,PATH:path.join(root,'mock-bin')+path.delimiter+process.env.PATH,INJECT_FAILURE:fail}});
    assert.equal(result.status,fail==='none'?0:1,result.stderr);
    assert.equal(existsSync(path.join(root,'installed/.onecrew-upgrade.lock')),false);
    if(fail==='journal') {
      assert.equal(readFileSync(installed,'utf8'),'original bytes');
      assert.deepEqual(JSON.parse(readFileSync(path.join(root,'installed/.onecrew-upgrade.json'))),{schema_version:999,phase:'unknown'});
    } else if(fail==='preflight') {
      assert.equal(readFileSync(installed,'utf8'),'original bytes');
      assert.equal(existsSync(path.join(root,'installed/.onecrew-upgrade.json')),false);
    } else {
      const journal=JSON.parse(readFileSync(path.join(root,'installed/.onecrew-upgrade.json')));
      assert.equal(journal.schema_version,1);
      assert.equal(journal.library.version,'0.1.4');
      assert.equal(journal.expected_plugin_binary_sha256,journal.candidate_cli_sha256);
      const phases={bootstrap:'cli_replacement',library:'library_installation',web:'service_start',plugin:'plugin_installation'};
      assert.equal(journal.phase,fail==='none'?'installed_pending_live_verification':`failed:${phases[fail]}`);
      if(fail==='bootstrap') assert.equal(readFileSync(installed,'utf8'),'original bytes');
      assert.equal(journal.candidate_cli_sha256,createHash('sha256').update(candidate).digest('hex'));
      if(fail==='plugin') {
        const resumed=spawnSync(bash,[script,'--target','x86_64-unknown-linux-gnu','--debug','--install-path',installed,'--library-dir',root,'--plugin-install'],{encoding:'utf8',timeout:30000,env:{...process.env,PATH:path.join(root,'mock-bin')+path.delimiter+process.env.PATH,INJECT_FAILURE:'none'}});
        assert.equal(resumed.status,0,resumed.stderr);
        const recovered=JSON.parse(readFileSync(path.join(root,'installed/.onecrew-upgrade.json')));
        assert.equal(recovered.previous_cli_sha256,journal.previous_cli_sha256);
        assert.ok(recovered.history.some(entry=>entry.phase==='failed:plugin_installation'));
        assert.equal(recovered.phase,'installed_pending_live_verification');
      }
      if(fail==='none') {
        const next=candidate+'\n# next generation\n';
        put(artifact+'onecrew_lite',next);
        put(artifact+'manifest.json',JSON.stringify({product:'onecrew_lite-cli',version:'0.6.88',target:'x86_64-unknown-linux-gnu',sha256:createHash('sha256').update(next).digest('hex'),size:Buffer.byteLength(next)}));
        const updated=spawnSync(bash,[script,'--target','x86_64-unknown-linux-gnu','--debug','--install-path',installed,'--library-dir',root,'--plugin-install'],{encoding:'utf8',timeout:30000,env:{...process.env,PATH:path.join(root,'mock-bin')+path.delimiter+process.env.PATH,INJECT_FAILURE:'none'}});
        assert.equal(updated.status,0,updated.stderr);
        const nextJournal=JSON.parse(readFileSync(path.join(root,'installed/.onecrew-upgrade.json')));
        assert.equal(nextJournal.previous_cli_sha256,journal.candidate_cli_sha256);
        assert.equal(nextJournal.previous_generations.length,1);
        assert.equal(nextJournal.previous_generations[0].candidate_cli_sha256,journal.candidate_cli_sha256);
      }
    }
  } finally {rmSync(root,{recursive:true,force:true});}
});
