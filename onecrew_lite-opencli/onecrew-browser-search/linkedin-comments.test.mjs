import assert from 'node:assert/strict';
import {test} from 'node:test';
import vm from 'node:vm';
import {linkedinActivityId, projectLinkedinComments, observeLinkedinComments, readLinkedinComments} from './linkedin-comments.mjs';

const handle='native-author', account='ACoAAA123456789abcdefghijk';
const profile=`https://www.linkedin.com/in/${handle}/`;
const post='7480296707809804288';
const source=`https://www.linkedin.com/feed/update/urn:li:activity:${post}/`;
const sample=()=>({author_handle:handle,account_id:account,source_url:source,
  comments:[{id:'7480303860733911040',parent_post_id:post,text:'A detailed keyboard review'}]});

test('comments retain actual native parent and strip unrelated public or private fields',()=>{
  const projected=projectLinkedinComments({...sample(),session:'private',email:'unrelated@example.org'},handle,account,source);
  assert.equal(projected.evidence_source,'visible_same_post_comments');
  assert.deepEqual(projected.comments,sample().comments);
  assert.doesNotMatch(JSON.stringify(projected),/private|unrelated/);
  for(const wrong of [source+'?view=other',source.replace(post,'0'+post),profile,'http://www.linkedin.com/'])
    assert.throws(()=>linkedinActivityId(wrong),/INPUT/);
  for(const change of [{author_handle:'other'},{account_id:'other'},{source_url:profile},
    {comments:[{...sample().comments[0],parent_post_id:'7481027555072962560'}]},
    {comments:[sample().comments[0],sample().comments[0]]},
    {comments:Array(6).fill(sample().comments[0])},
    {comments:[{...sample().comments[0],text:'界'.repeat(667)}]},
    {comments:[{...sample().comments[0],text:'bad\0text'}]}])
    assert.throws(()=>projectLinkedinComments({...sample(),...change},handle,account,source),/INVALID/);
});

function context() {
  const visible={getClientRects:()=>[1],closest:()=>null};
  const author={...visible,href:profile}, permalink={...visible,href:source};
  const primary={querySelector:()=>null,querySelectorAll:selector=>selector==='h2'?[{}]:selector==='a[href]'?[author,permalink]:[]};
  const rows=Array.from({length:6},(_,index)=>{
    const paragraph={...visible,textContent:index===0?'':`Original comment ${index}`,closest:()=>null};
    const body={querySelector:()=>null,querySelectorAll:selector=>selector==='p'?[paragraph]:[],paragraph};
    const row={...visible,children:[{}, {children:[{},body,{}]}],
      key:`CommentComponentReference_urn:li:comment:(activity:${post},${7480303860733911040n+BigInt(index)})`,
      getAttribute(){return this.key;}};
    return row;
  });
  const card={...visible,children:[primary,{querySelectorAll:()=>[]},{querySelectorAll:()=>[]}],querySelector:selector=>selector==='h2'?{}:null,
    querySelectorAll:()=>[]};
  const main={querySelectorAll:selector=>selector==='[role="listitem"]'?[card]:[]};
  let current=card;
  for(let index=0;index<4;index++) {
    const wrapper={querySelectorAll:selector=>selector==='h2'?[{}]:selector==='[role="listitem"]'?[card]:index===3?rows:[],parentElement:main};
    current.parentElement=wrapper;current=wrapper;
  }
  const sandbox={location:{origin:'https://www.linkedin.com',pathname:`/feed/update/urn:li:activity:${post}/`,href:source},
    URL,TextEncoder,NodeFilter:{SHOW_TEXT:4},document:{querySelector:()=>main,querySelectorAll:()=>[],
      createTreeWalker:paragraph=>{let read=false;return {nextNode(){if(read)return false;read=true;this.currentNode={textContent:paragraph.textContent,parentElement:paragraph};return true;}};}}};
  const execute=()=>vm.runInNewContext(`(${observeLinkedinComments.toString()})(${JSON.stringify(source)},${JSON.stringify(profile)},${JSON.stringify(handle)},${JSON.stringify(account)})`,sandbox);
  return {execute,sandbox,author,permalink,primary,rows,card};
}

test('observed sibling layout reads first five positions including empty, never a sixth replacement',()=>{
  const setup=context();
  const result=setup.execute();
  assert.equal(result.comments.length,5);
  assert.equal(result.comments[0].text,'');
  assert.equal(result.comments[4].text,'Original comment 4');
  assert.ok(result.comments.every(c=>c.parent_post_id===post));
  assert.doesNotMatch(JSON.stringify(result),/Original comment 5/);
});

test('repost thread, other author, translated comment and blocked route fail rather than qualify',()=>{
  const setup=context();
  setup.rows[0].key=setup.rows[0].key.replace(post,'7481027555072962560');
  assert.throws(setup.execute,/INVALID/);
  setup.rows[0].key=setup.rows[0].key.replace('7481027555072962560',post);
  setup.author.href='https://www.linkedin.com/in/other/';
  assert.throws(setup.execute,/INVALID/);
  setup.author.href=profile;
  const body=setup.rows[0].children[1].children[1], original=body.querySelectorAll;
  body.querySelectorAll=selector=>selector==='button,[role="button"]'?[{getClientRects:()=>[1],closest:()=>null,textContent:'Show original'}]:original(selector);
  assert.throws(setup.execute,/ORIGINAL_TEXT_UNCONFIRMED/);
  body.querySelectorAll=original;
  setup.sandbox.location.pathname='/checkpoint/challenge/';
  assert.throws(setup.execute,/RISK/);
});

test('one owned scope rechecks profile native ID, reads exact post and always releases scope',async()=>{
  const profileData={author_handle:handle,account_id:account,author_label:'Native Author',profile_url:profile,
    follower_text:'1,606 followers',headline:'Reviews',bio:'Original bio',public_links:[],contact_links:[]};
  let released=0, calls=0; const urls=[];
  const owned={goto:async url=>urls.push(url),sleep:async()=>{},evaluate:async()=>++calls%2===1?profileData:sample()};
  const scope=async(_page,read)=>{try{return await read(owned);}finally{released++;}};
  assert.equal((await readLinkedinComments({},handle,account,source,scope)).comments.length,1);
  assert.deepEqual(urls,[profile,source]);assert.equal(released,1);
  profileData.account_id='ACoAAA987654321otheraccount';calls=0;urls.length=0;
  await assert.rejects(()=>readLinkedinComments({},handle,account,source,scope),/INVALID/);
  assert.deepEqual(urls,[profile]);assert.equal(released,2);
});
