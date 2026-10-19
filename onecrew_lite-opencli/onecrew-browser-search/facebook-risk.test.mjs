import test from 'node:test';
import assert from 'node:assert/strict';
import vm from 'node:vm';
import {readFacebookProfileHeader} from './facebook-profile.mjs';
import {readFacebookRecentPage} from './facebook-recent.mjs';
import {readFacebookCommentPage} from './facebook-comments.mjs';
import {readFacebookReelPage} from './facebook-reel-comments.mjs';

for (const read of [readFacebookProfileHeader,readFacebookRecentPage,readFacebookCommentPage,readFacebookReelPage]) {
  test(`${read.name} stops at verification before reading or restoring text`,async()=>{
    for (const path of ['/checkpoint/','/challenge/','/two_step_verification/two_factor/']) {
      const unexpected=()=>assert.fail('must stop without DOM read, translation or retry');
      const run=vm.runInNewContext(`(${read.toString()})`,{
        location:new URL(`https://www.facebook.com${path}`),
        document:{querySelectorAll:unexpected},setTimeout:unexpected,URL,
      });
      await assert.rejects(run('https://www.facebook.com/reel/123','123',5,unexpected,unexpected),/RISK/);
    }
  });
}
