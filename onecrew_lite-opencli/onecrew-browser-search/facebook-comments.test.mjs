import test from 'node:test';
import assert from 'node:assert/strict';
import {facebookPostTarget,projectFacebookComments,readFacebookComments} from './facebook-comments.mjs';
const source='https://www.facebook.com/creator/posts/pfbid0123456789ABC';
const data=()=>({author_url:'https://www.facebook.com/creator?tracking=ignored',text:'Original post',comments:[
  {source_url:`${source}?comment_id=123&tracking=ignored`,text:'Original audience language'},
]});
test('Facebook preserves original text and same-post provenance without commenter identity',()=>{
  const rows=projectFacebookComments(data(),source,'creator',5);
  assert.equal(rows[0].author_url,'https://www.facebook.com/creator');
  assert.deepEqual(rows[1],{type:'COMMENT',source_url:`${source}?comment_id=123`,text:'Original audience language',text_truncated:false});
});
test('Facebook rejects wrong post, author, replies and invalid text',()=>{
  for(const mutate of [d=>d.author_url='https://www.facebook.com/other',d=>d.comments[0].source_url=source.replace('creator','other')+'?comment_id=123',d=>d.comments[0].source_url+='&reply_comment_id=456',d=>d.comments[0].text='',d=>d.comments[0].text='bad\0text']) {
    const d=data();mutate(d);assert.throws(()=>projectFacebookComments(d,source,'creator',5));
  }
});
test('Facebook bounds and deduplicates comments, removes tracking',()=>{
  const d=data();d.comments.push({...d.comments[0]});
  d.comments.push({source_url:`${source}?comment_id=456`,text:'日'.repeat(2200)});
  assert.equal(projectFacebookComments(d,source,'creator',1).length,2);
  const rows=projectFacebookComments(d,source,'creator',20);
  assert.equal(rows.length,3);assert.equal(rows[2].text.length,2000);assert.equal(rows[2].text_truncated,true);
});
test('Facebook invalid input never opens browser',async()=>{
  for(const bad of [source+'?redirect=evil',source.replace('facebook.com','facebook.com.evil'),source+'/../other','https://www.facebook.com/groups/123']) {
    assert.throws(()=>facebookPostTarget(bad));
    await assert.rejects(readFacebookComments(null,{id:bad},()=>assert.fail('must not navigate')));
  }
});
