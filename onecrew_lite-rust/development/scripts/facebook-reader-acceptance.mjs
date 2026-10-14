// Explicit development acceptance against the same bundled OpenCLI adapters.
// No customer Campaign, Creator, mail or table writes. Stops on the first error.
import {execFile} from 'node:child_process';
import {promisify} from 'node:util';
import {writeFile} from 'node:fs/promises';
import assert from 'node:assert/strict';

const [node,cli,profile,output,consent]=process.argv.slice(2);
assert.equal(consent,'--live','Explicit live-test authorization is required');
assert.ok(node && cli && profile && output);
const run=promisify(execFile), receipts=[];
async function read(args,verify) {
  const started=Date.now();
  try {
    const {stdout}=await run(node,[cli,'--profile',profile,...args],{timeout:60000,maxBuffer:1024*1024});
    const rows=JSON.parse(stdout);
    assert.ok(Array.isArray(rows));
    verify(rows);
    receipts.push({args,elapsed_ms:Date.now()-started,status:'passed',rows});
    console.log(`PASS ${args[0]}/${args[1]}`);
    return rows;
  } catch(error) {
    // Never persist raw process stderr, which can contain browser diagnostics.
    receipts.push({args,elapsed_ms:Date.now()-started,status:'failed',code:String(error.code ?? 'assertion_or_parse')});
    throw new Error(`Acceptance failed: ${args[0]}/${args[1]}`);
  } finally {
    await writeFile(output,JSON.stringify({captured_at:new Date().toISOString(),profile,receipts},null,2)+'\n');
  }
}
for(const query of ['鼠标测评','滑鼠 評測','遊戲滑鼠 評測']) {
  await read(['onecrew-search-facebook','search',query,'--limit','5','-f','json'],rows=>{
    assert.ok(rows.length>0 && rows.length<=5);
    for(const row of rows) {
      assert.ok(row.title.trim());
      assert.ok(!/^Online status indicator|^\+\d+$/.test(row.title));
      assert.ok(!/^\/(photo|photos|groups|reels?|watch)(\/|$)/.test(new URL(row.url).pathname));
    }
  });
}
// Native IDs were independently observed in the initial diagnostic run.
for(const [handle,id] of [['technavecn','100064469360750'],['yijibang98','100067897790624'],['leiyanyo','100063572267111']]) {
  const url='https://www.facebook.com/'+handle;
  await read(['onecrew-facebook-profile','identity','-f','json','--',url],rows=>{
    assert.equal(rows.length,1);assert.equal(rows[0].id,id);assert.equal(rows[0].source_url,url);
  });
  await read(['onecrew-facebook-profile','native-read','-f','json','--',id,url],rows=>{
    assert.equal(rows.length,1);assert.equal(rows[0].id,id);assert.ok(/\d/.test(rows[0].follower_text));
    assert.equal(rows[0].observed_profile_url,url);
  });
}
for(const [handle,id,post] of [['technavecn','100064469360750','4600634696839390'],['leiyanyo','100063572267111','1140681113060657']]) {
  await read(['onecrew-facebook-profile','native-original','-f','json','--',id,'https://www.facebook.com/'+handle,'https://www.facebook.com/reel/'+post],rows=>{
    assert.equal(rows.length,1);assert.equal(rows[0].author_id,id);assert.equal(rows[0].post_id,post);assert.ok(rows[0].text.trim());
  });
}
console.log(`PASS ${receipts.length} live reads; this verifies extraction, not creator qualification.`);
