import test from 'node:test';
import assert from 'node:assert/strict';
import vm from 'node:vm';
import {normalizeBrowserRows, readFacebookCreatorResults} from './owned-page.mjs';

test('Facebook groups do not consume creator result slots', () => {
  const groups = Array.from({length: 6}, (_, i) => ({title: 'Headphones group', url: `https://www.facebook.com/groups/${i + 1}/`}));
  const creator = {title: 'Audio reviewer', url: 'https://www.facebook.com/AudioReviewer/'};
  assert.deepEqual(normalizeBrowserRows('facebook', [...groups, creator], 3), [creator]);
});

test('native extraction limit is applied after groups are removed without retrying the query', async () => {
  const creator = {title: 'Reviewer', url: 'https://www.facebook.com/profile.php?id=123'};
  const raw = [...Array.from({length: 8}, (_, i) => ({url: `https://www.facebook.com/groups/${i + 1}/`})), creator];
  const calls = [];
  const result = await readFacebookCreatorResults({evaluate:async()=>raw}, {query: 'headphones review', limit: 3}, async (_page, args) => {
    calls.push(args);
    return raw.slice(0, args.limit);
  });
  assert.deepEqual(calls, [{query: 'headphones review', limit: 50}]);
  assert.deepEqual(result, [creator]);
});

test('keeps exact numeric and vanity profiles, deduplicates and excludes non-account surfaces', () => {
  const paths = ['/groups/123/', '/events/123/', '/reel/123/', '/watch/?v=123', '/creator/posts/123', '/pages/', '/marketplace/', '/profile.php?id=123&id=456', '/profile.php?id=0123', '/Creator/', '/Creator', '/profile.php?id=123'];
  const rows = paths.map(path => ({url: 'https://www.facebook.com' + path}));
  assert.deepEqual(normalizeBrowserRows('facebook', rows, 3).map(row => row.url), [
    'https://www.facebook.com/Creator/', 'https://www.facebook.com/profile.php?id=123',
  ]);
});

test('groups-only results remain zero creator leads rather than fabricated identities', () => {
  assert.deepEqual(normalizeBrowserRows('facebook', [{url:'https://www.facebook.com/groups/123/'}], 3), []);
});

test('Facebook photo expansion links do not become creator accounts', () => {
  assert.deepEqual(normalizeBrowserRows('facebook', [{title:'+6',url:'https://www.facebook.com/photo'}], 3), []);
});

test('Facebook online-status anchors are not named search results', () => {
  const url = 'https://www.facebook.com/AndrewDavidWeber';
  const creator = {title: 'Andrew David Weber Music', url};
  const status = {title: 'Online status indicatorActive', url};
  assert.deepEqual(normalizeBrowserRows('facebook', [status, creator], 3), [creator]);
  assert.deepEqual(normalizeBrowserRows('facebook', [status], 3), []);
});

test('malformed data and native failures remain failures', async () => {
  for (const url of ['not a url', 'https://evil.example/Creator', 'https://user:pass@www.facebook.com/Creator', 'https://www.facebook.com/Creator#other'])
    assert.throws(() => normalizeBrowserRows('facebook', [{url}], 3), /INVALID/);
  await assert.rejects(readFacebookCreatorResults({}, {query:'audio reviewer',limit:3}, async () => {throw new Error('AUTH_REQUIRED');}), /AUTH_REQUIRED/);
  await assert.rejects(readFacebookCreatorResults({}, {query:'audio reviewer',limit:0}, async () => []), /INPUT/);
});

test('wrapper reads named visible result anchors rather than native presence-label dedup output', async () => {
  const url = 'https://www.facebook.com/HeadphoneReviewDotCom';
  const anchor = (title, href = url, visible = true) => ({href, innerText:title, textContent:title,
    getClientRects:()=>visible?[1]:[], closest:()=>null});
  const anchors = [anchor('Online status indicatorActive'), anchor('Headphone Review'),
    anchor('Hidden noise','https://www.facebook.com/Hidden',false)];
  const context = {URL, location:{href:'https://www.facebook.com/search/top?q=headphone%20reviewer'},
    getComputedStyle:()=>({visibility:'visible'}),
    document:{querySelector:selector=>selector==='[role="feed"]'?{querySelectorAll:()=>anchors}:null}};
  const page = {evaluate:async script=>JSON.parse(JSON.stringify(vm.runInNewContext(script,context)))};
  const native = async()=>[{title:'Online status indicatorActive',url}];
  const rows = await readFacebookCreatorResults(page,{query:'headphone reviewer',limit:3},native);
  assert.equal(rows.length,1);
  assert.equal(rows[0].title,'Headphone Review');
  anchors.push(anchor('Review group','https://www.facebook.com/groups/123/'),
    anchor('+6','https://www.facebook.com/photo?fbid=123'),
    {...anchor('Sidebar contact','https://www.facebook.com/Contact'),closest:()=>({})},
    anchor('Numeric reviewer','https://www.facebook.com/profile.php?id=123&__tn__=tracking'),
    anchor('Foreign','https://evil.example/Reviewer'));
  const more = await readFacebookCreatorResults(page,{query:'headphone reviewer',limit:3},native);
  assert.deepEqual(more.map(row=>row.url),[url,'https://www.facebook.com/profile.php?id=123']);
  // A post author need not have the query or a creator role in their name.
  anchors.splice(0,anchors.length,anchor('一级棒','https://www.facebook.com/GreatDesk'));
  context.location.href='https://www.facebook.com/search/top/?q=%E9%BC%A0%E6%A0%87';
  const postAuthor = await readFacebookCreatorResults(page,{query:'鼠标',limit:3},native);
  assert.equal(postAuthor[0].title,'一级棒');
  context.location.href='https://www.facebook.com/search/top?q=headphone%20reviewer';
  // Recognized result area containing only groups is genuinely zero accounts.
  anchors.splice(0,anchors.length,anchor('Review group','https://www.facebook.com/groups/123/'));
  assert.deepEqual(await readFacebookCreatorResults(page,{query:'headphone reviewer',limit:3},native),[]);
  context.document.querySelector=()=>null;
  await assert.rejects(readFacebookCreatorResults(page,{query:'headphone reviewer',limit:3},native),/INVALID/);
  context.location.href='https://www.facebook.com/search/top?q=other';
  await assert.rejects(readFacebookCreatorResults(page,{query:'headphone reviewer',limit:3},native),/INVALID/);
});
