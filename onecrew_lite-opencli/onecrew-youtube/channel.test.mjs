import test from 'node:test'; import assert from 'node:assert/strict';
import {projectChannelData} from './channel.mjs';
const id='UCabcdefghijklmnopqrstuv';
const dataFor=about=>({metadata:{channelMetadataRenderer:{externalId:id,title:'Creator',description:'Official description'}},contents:{channelAboutFullMetadataRenderer:about}});
test('native hidden email entry is locale independent and never exposes an address',()=>{
 for(const label of ['View email address','查看电子邮件地址','メールアドレスを表示','Afficher l’adresse e-mail']){
  const p=projectChannelData(dataFor({businessEmailLabel:{simpleText:label}}),id);
  assert.equal(p.hiddenEmailAvailable,true);assert.equal(p.sourceUrl,'https://www.youtube.com/channel/'+id);
  assert.equal(p.description,'Official description');
 }
});
test('missing entry, prose and unrelated renderers never fabricate hidden email',()=>{
 assert.equal(projectChannelData(dataFor({description:{simpleText:'Please email me'}}),id).hiddenEmailAvailable,false);
 const data={metadata:{channelMetadataRenderer:{externalId:id,title:'Creator',description:'View email address'}},contents:{videoRenderer:{businessEmailButton:{}}}};
 assert.equal(projectChannelData(data,id).hiddenEmailAvailable,false);
 assert.throws(()=>projectChannelData(dataFor({}), 'UCxxxxxxxxxxxxxxxxxxxxxx'));
});

test('native reveal button is an entry but sign-in prompt is not one',()=>{
 assert.equal(projectChannelData(dataFor({channelId:id,businessEmailRevealButton:{buttonViewModel:{title:'查看电子邮件地址'}}}),id).hiddenEmailAvailable,true);
 assert.equal(projectChannelData(dataFor({channelId:id,signInForBusinessEmail:{content:'Sign in'}}),id).hiddenEmailAvailable,false);
 assert.throws(()=>projectChannelData(dataFor({channelId:'UCxxxxxxxxxxxxxxxxxxxxxx',businessEmailRevealButton:{}}),id));
});
test('public email and attributed external links survive bounded projection',()=>{
 const p=projectChannelData(dataFor({businessEmail:{simpleText:'studio@example.com'},primaryLinks:[{navigationEndpoint:{urlEndpoint:{url:'https://www.youtube.com/redirect?q=https%3A%2F%2Fwww.instagram.com%2Fcreator'}}}]}),id);
 assert.match(p.description,/studio@example.com/);assert.deepEqual(p.publicLinks,['https://www.instagram.com/creator']);
});
import {readChannelPage} from './channel.mjs';
test('loaded native About uses the signed-in page and never reveals email or fetches again', async()=>{
 const before={fetch:globalThis.fetch,location:globalThis.location,window:globalThis.window,document:globalThis.document};
 globalThis.location={origin:'https://www.youtube.com'};
 globalThis.window={ytInitialData:{metadata:{channelMetadataRenderer:{externalId:id,title:'Creator'}}}};
 const about={channelId:id,businessEmailRevealButton:{buttonViewModel:{title:'查看电子邮件地址'}}};
 globalThis.document={querySelectorAll:selector=>{
  assert.equal(selector,'ytd-about-channel-renderer');
  return [{data:{metadata:{aboutChannelViewModel:about}},click:()=>assert.fail('must not reveal email')}];
 }};
 globalThis.fetch=()=>assert.fail('native page owns authentication; no second request');
 try {
  const p=await readChannelPage(id,projectChannelData);
  assert.equal(p.hiddenEmailAvailable,true);assert.equal(p.aboutDataAvailable,true);assert.equal(p.channelId,id);
  about.channelId='UCxxxxxxxxxxxxxxxxxxxxxx';
  await assert.rejects(readChannelPage(id,projectChannelData),/CHANNEL_ID_MISMATCH/);
 } finally {Object.assign(globalThis,before);}
});

test('modern About action response preserves hidden entry',()=>{
 const data={metadata:{channelMetadataRenderer:{externalId:id,title:'Reid Wilking',description:'Business inquiries: email below.'}},onResponseReceivedActions:[{appendContinuationItemsAction:{continuationItems:[{aboutChannelRenderer:{metadata:{aboutChannelViewModel:{businessEmailButton:{buttonViewModel:{title:'View email address'}}}}}}]}}]};
 assert.equal(projectChannelData(data,id).hiddenEmailAvailable,true);
});

test('missing About data is unknown, distinct from confirmed absent entry',()=>{
 const data={metadata:{channelMetadataRenderer:{externalId:id,title:'Creator',description:'Business inquiries: email below.'}}};
 const unknown=projectChannelData(data,id);
 assert.equal(unknown.aboutDataAvailable,false);assert.equal(unknown.hiddenEmailAvailable,false);
 assert.equal(projectChannelData(dataFor({}),id).aboutDataAvailable,true);
});

test('channel metadata may omit empty description',()=>{
 const data={metadata:{channelMetadataRenderer:{externalId:id,title:'Creator'}},contents:{aboutChannelViewModel:{businessEmailButton:{}}}};
 assert.equal(projectChannelData(data,id).hiddenEmailAvailable,true);
});
