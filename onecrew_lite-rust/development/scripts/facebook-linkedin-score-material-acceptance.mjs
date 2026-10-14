// Explicit development regression: public author-bound material, no Campaign writes.
import {execFile} from 'node:child_process';
import {promisify} from 'node:util';
import {writeFile} from 'node:fs/promises';
import assert from 'node:assert/strict';

const [node, cli, profile, output, consent] = process.argv.slice(2);
assert.equal(consent, '--live');
assert.ok(node && cli && profile && output);
const execute = promisify(execFile), receipts = [];
async function read(args, validate) {
  const started = Date.now();
  try {
    const {stdout} = await execute(node, [cli, '--profile', profile, ...args, '-f', 'json'],
      {timeout:60000, maxBuffer:1024*1024});
    const rows = JSON.parse(stdout);
    assert.ok(Array.isArray(rows) && rows.length);
    validate(rows);
    receipts.push({args, status:'passed', elapsed_ms:Date.now()-started, rows});
    console.log(`PASS ${args[0]}/${args[1]}`);
  } catch (error) {
    receipts.push({args, status:'failed', elapsed_ms:Date.now()-started,
      code:String(error.code ?? 'assertion_or_parse')});
    throw new Error(`Acceptance failed: ${args[0]}/${args[1]}`);
  } finally {
    await writeFile(output, JSON.stringify({captured_at:new Date().toISOString(), profile, receipts},null,2)+'\n');
  }
}
// These identities and source URLs were observed in the preceding live run.
// Fixed sources test extraction repeatability, not personalized search ranking.
const linkedinId = 'ACoAADI6yCQB5gUHH3WUJbBQ3V3FHezK39ugzxw';
const facebookId = '100064057232384';
for (let round=1; round<=2; round++) {
  await read(['onecrew-linkedin-reader','profile','phil-hayton'], rows=>{
    assert.equal(rows.length,1);
    assert.equal(rows[0].account_id,linkedinId);
    assert.ok(rows[0].bio.trim() && rows[0].headline.trim());
    assert.ok(Number.isSafeInteger(rows[0].follower_count));
  });
  await read(['onecrew-linkedin-reader','content','phil-hayton',linkedinId], rows=>{
    assert.equal(rows.length,1);
    assert.equal(rows[0].account_id,linkedinId);
    assert.ok(rows[0].text.trim() && rows[0].source_url.includes(rows[0].post_id));
  });
  await read(['onecrew-facebook-profile','native-read',facebookId,'https://www.facebook.com/hkepc'], rows=>{
    assert.equal(rows.length,1);
    assert.equal(rows[0].id,facebookId);
    assert.match(rows[0].follower_text,/\d/);
  });
  await read(['onecrew-facebook-profile','native-original',facebookId,'https://www.facebook.com/hkepc',
    'https://www.facebook.com/reel/2210122709845852'], rows=>{
    assert.equal(rows.length,1);
    assert.equal(rows[0].author_id,facebookId);
    assert.equal(rows[0].post_id,'2210122709845852');
    assert.ok(rows[0].text.trim());
  });
  console.log(`Round ${round} complete`);
}
console.log('PASS: author-bound scoring material; not a product-match or global stability verdict.');
