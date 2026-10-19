import test from 'node:test';
import assert from 'node:assert/strict';
import {projectRedditRecent} from './reddit-recent.mjs';
const post=id=>({kind:'t3',data:{id,name:`t3_${id}`,author:'Example_User',author_fullname:'t2_abc',selftext:'original'}});
const listing=rows=>({kind:'Listing',data:{children:rows}});
test('only first three native author-bound original posts are exposed',()=>{
  const crosspost=post('cross');crosspost.data.crosspost_parent='t3_other';
  const rows=projectRedditRecent(listing([crosspost,...['a','b','c','d'].map(post)]),'example_user','t2_abc');
  assert.deepEqual(rows,['a','b','c'].map(id=>({source_url:`https://www.reddit.com/comments/${id}/`})));
});
test('wrong author, duplicate or malformed feed is not empty success',()=>{
  const wrong=post('a');wrong.data.author_fullname='t2_other';
  for(const data of [listing([wrong]),listing([post('a'),post('a')]),{},listing([{kind:'t1'}])])
    assert.throws(()=>projectRedditRecent(data,'Example_User','t2_abc'));
  assert.deepEqual(projectRedditRecent(listing([]),'Example_User','t2_abc'),[]);
});
