import test from 'node:test';
import assert from 'node:assert/strict';
import vm from 'node:vm';
import {projectRedditSearch,fetchRedditSearch} from './reddit-search.mjs';
const listing=post=>({kind:'Listing',data:{children:[{kind:'t3',data:{id:'abc',name:'t3_abc',title:'Original title',author:'Example',author_fullname:'t2_native',...post}}]}});
test('approved phrase and Boolean syntax reaches one native search unchanged',async()=>{
  const queries=['"gaming mouse" AND (comparison OR testing)', 'title:"gaming mouse" AND review', '"游戏鼠标" AND "上手体验"'];
  for(const query of queries){
    let calls=0;
    const context={location:{origin:'https://www.reddit.com'},URL,URLSearchParams,AbortSignal,
      fetch:async(path,options)=>{
        calls++;
        const url=new URL(path,'https://www.reddit.com');
        assert.equal(url.pathname,'/search.json');
        assert.equal(url.searchParams.get('q'),query);
        assert.equal(url.searchParams.get('limit'),'3');
        assert.equal(options.credentials,'include');
        return {ok:true,status:200,url:url.href,json:async()=>listing({})};
      }};
    await vm.runInNewContext(`(${fetchRedditSearch.toString()})(${JSON.stringify(query)},3)`,context);
    assert.equal(calls,1);
  }
});
test('native search identity is retained with its original post source',()=>{
  const [row]=projectRedditSearch(listing({}),5);
  assert.equal(row.author_id,'t2_native');
  assert.equal(row.author_url,'https://www.reddit.com/user/Example/');
  assert.equal(row.url,'https://www.reddit.com/comments/abc/');
});
test('missing or deleted identity stays unknown instead of deriving from a name',()=>{
  for(const data of [{author_fullname:null},{author:'[deleted]'}]){
    const [row]=projectRedditSearch(listing(data),5);
    assert.equal(row.author_id,null);assert.equal(row.author_url,null);
  }
});
test('invalid native identity and malformed response are not success',()=>{
  for(const data of [listing({author_fullname:'Example'}),listing({name:'t3_other'}),{}])
    assert.throws(()=>projectRedditSearch(data,5));
  assert.deepEqual(projectRedditSearch({kind:'Listing',data:{children:[]}},5),[]);
});

test('search keeps native post text so a question and a hands-on review remain distinguishable',()=>{
  const body='I tested the sensor and buttons for two weeks. The left button double clicks.';
  const [row]=projectRedditSearch(listing({title:'My mouse experience',selftext:body}),3);
  assert.equal(row.title,'My mouse experience\n\n'+body);
  assert.equal(row.author_id,'t2_native');
  assert.equal(row.url,'https://www.reddit.com/comments/abc/');
});

test('post preview is bounded without inventing missing or removed text',()=>{
  for(const selftext of [undefined,null,'','[removed]','[deleted]'])
    assert.equal(projectRedditSearch(listing({selftext}),3)[0].title,'Original title');
  assert.equal(Array.from(projectRedditSearch(listing({selftext:'😀'.repeat(1000)}),3)[0].title).length,500);
  for(const selftext of [{text:'untrusted shape'},'bad\0text'])
    assert.throws(()=>projectRedditSearch(listing({selftext}),3),/INVALID/);
});

test('long titles and image destinations cannot crowd actual experience out of the search excerpt',()=>{
  const selftext='https://preview.redd.it/image.png?'+ 'x'.repeat(600)+'\nI tested the buttons. [Sensor comparison](https://example.org/'+'y'.repeat(600)+')';
  const [row]=projectRedditSearch(listing({title:'Mouse review '.repeat(40),selftext}),3);
  assert.ok(row.title.includes('I tested the buttons. Sensor comparison'));
  assert.ok(!row.title.includes('preview.redd.it'));
  assert.ok(Array.from(row.title).length<=500);
});
