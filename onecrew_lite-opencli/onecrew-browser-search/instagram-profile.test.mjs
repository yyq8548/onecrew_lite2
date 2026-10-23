import test from 'node:test';
import assert from 'node:assert/strict';
import {readFile} from 'node:fs/promises';
import {instagramProfileTarget,readInstagramWebProfile} from './instagram-profile.mjs';

for(const username of ['tssitlidze_sofia','eliirax']) {
  const [profile]=JSON.parse(await readFile(new URL(`../../onecrew_lite-app/tests/fixtures/instagram-visible-profile/${username}.json`,import.meta.url),'utf8'));
  test(`真实主页 ${username} 保留大小写归一与来源绑定`,()=>{
    assert.equal(instagramProfileTarget(profile.username.toUpperCase()),profile.username);
    assert.equal(profile.profile_url,`https://www.instagram.com/${instagramProfileTarget(profile.username)}/`);
  });
}

test('无效主页或不同账号的身份在浏览器调用前明确失败',async()=>{
  for(const username of ['','../creator','creator/other','https://www.instagram.com/creator/']) assert.throws(()=>instagramProfileTarget(username),/INPUT/);
  await assert.rejects(readInstagramWebProfile(null,{username:'eliirax',author_id:'https://www.instagram.com/tssitlidze_sofia/'},null),/INPUT/);
});
