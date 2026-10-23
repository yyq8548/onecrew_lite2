import test from 'node:test';
import assert from 'node:assert/strict';
import {readFile} from 'node:fs/promises';
import {instagramRecentTarget,selectInstagramVisiblePost} from './instagram-recent.mjs';

for(const username of ['tssitlidze_sofia','eliirax']) {
  const [profile]=JSON.parse(await readFile(new URL(`../../onecrew_lite-app/tests/fixtures/instagram-visible-profile/${username}.json`,import.meta.url),'utf8'));
  test(`真实已观察帖子 ${username} 复用数字身份及主页绑定`,()=>{
    assert.doesNotThrow(()=>instagramRecentTarget(profile.username,profile.id));
    assert.doesNotThrow(()=>instagramRecentTarget(profile.username,profile.profile_url));
    const selected=selectInstagramVisiblePost(profile.visible_posts);
    assert.equal(selected,profile.visible_posts[0]);
    assert.ok(selected.url.startsWith(profile.profile_url));
    assert.throws(()=>instagramRecentTarget(profile.username,'https://www.instagram.com/other/'),/INPUT/);
  });
}
