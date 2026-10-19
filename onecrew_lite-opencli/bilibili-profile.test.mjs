import test from 'node:test';
import assert from 'node:assert/strict';
import vm from 'node:vm';
import { projectProfile, readBilibiliProfile } from './onecrew-bilibili-profile/profile.mjs';

const mid = '8420618';
const observation = (changes = {}) => ({ mid, name: 'Example creator', label: '粉丝数', follower_count_text: '1,534,708', follower_display: '153.4万', ...changes });

test('uses the exact follower tooltip, retaining the abbreviated display and canonical account source', () => {
  const result = projectProfile(observation(), mid);
  assert.equal(result.follower_count, 1534708);
  assert.equal(result.follower_display, '153.4万');
  assert.equal(result.follower_count_text, '1,534,708');
  assert.equal(result.profile_url, 'https://space.bilibili.com/8420618');
  assert.equal(result.evidence_source, 'profile_tooltip');
  assert.equal(projectProfile(observation({ follower_count_text: '0', follower_display: '0' }), mid).follower_count, 0);
});

test('missing, approximate, malformed and unsafe counts cannot become exact follower evidence', () => {
  for (const follower_count_text of [undefined, null, '', '153.4万', '1.5', '-1', '01', '12,34', '0,123', '9007199254740992']) {
    assert.throws(() => projectProfile(observation({ follower_count_text }), mid), /ONECREW_BILIBILI_PROFILE_INVALID/);
  }
  for (const changes of [{ mid: '123' }, { name: '' }, { name: 'x\0y' }, { label: '关注数' }, { follower_display: '' }]) {
    assert.throws(() => projectProfile(observation(changes), mid), /ONECREW_BILIBILI_PROFILE_INVALID/);
  }
});

function fixture({ href, body = '', labels = ['粉丝数'], names = ['Example creator'], tooltip = '1,534,708', signs = [], announcements = [] } = {}) {
  const calls = [];
  const stats = labels.map(text => ({ querySelectorAll: selector => {
    if (selector === '.nav-statistics__item-text') return [{ textContent: text }];
    if (selector === '.nav-statistics__item-num') return [{ textContent: '153.4万', getAttribute: key => {
      assert.equal(key, 'title'); return tooltip;
    } }];
    assert.fail('Unexpected selector: ' + selector);
  } }));
  const page = {
    goto: () => assert.fail('No navigation fallback'),
    newTab: async url => { calls.push(['newTab', url]); return 'exact-profile'; },
    setActivePage: async id => calls.push(['bind', id]),
    closeTab: async id => calls.push(['close', id]),
    wait: async options => calls.push(['wait', options]),
    evaluate: async code => {
      if (code.includes('async function restoreOriginalLanguage()')) return true;
      calls.push(['read']);
      return vm.runInNewContext(code, { URL,
        location: { href: href ?? calls[0][1] }, document: { body: { innerText: body }, querySelectorAll: selector => {
          if (selector === '.upinfo-detail__top .nickname') return names.map(textContent => ({ textContent }));
          if (selector === '.nav-statistics__item') return stats;
          if (selector === '.upinfo-detail__bottom .sign.header-sign .pure-text') return signs.map(sign => ({textContent:sign.text, getAttribute:()=>sign.title ?? null}));
          if (selector === '.ann-section') return announcements.map(text => ({
            querySelector: () => ({textContent:'公告'}),
            querySelectorAll: () => [{textContent:Array.isArray(text) ? text.join('') : text,
              querySelectorAll: () => Array.isArray(text) ? text.map(textContent=>({textContent})) : []}],
          }));
          assert.fail('Unexpected selector: ' + selector);
        } }, fetch: () => assert.fail('No API requests'),
      });
    },
  };
  return { page, calls };
}

test('reads the matched profile once; following and follower metrics remain distinct', async () => {
  const { page, calls } = fixture({ labels: ['关注数', '粉丝数'] });
  const rows = await readBilibiliProfile(page, mid);
  assert.equal(rows[0].mid, mid);
  assert.equal(rows[0].follower_count, 1534708);
  assert.deepEqual(calls, [['newTab', 'https://space.bilibili.com/8420618'], ['bind', 'exact-profile'], ['wait', { time: 3 }], ['read'], ['close', 'exact-profile']]);
});

test('reads only the observed header biography, never unrelated page contacts', async () => {
  const sign = 'Original biography\nBusiness: studio@example.org';
  const {page} = fixture({body:'Comment: unrelated@example.org', signs:[{text:sign,title:sign}]});
  assert.equal((await readBilibiliProfile(page, mid))[0].sign, sign);
  for (const signs of [[], [{text:sign},{text:sign}], [{text:'partial',title:sign}], [{text:'x'.repeat(1001)}]]) {
    assert.equal((await readBilibiliProfile(fixture({body:'unrelated@example.org',signs}).page, mid))[0].sign, undefined);
  }
});

test('combines native biography and announcement, retaining contact remarks', async () => {
  const {page} = fixture({signs:[{text:'科技评测'}], announcements:['商务合作 GJYWS719（备注品牌来意）'], body:'comment other@example.org'});
  assert.equal((await readBilibiliProfile(page, mid))[0].sign, '科技评测\n商务合作 GJYWS719（备注品牌来意）');
  assert.equal((await readBilibiliProfile(fixture({announcements:['one','two']}).page, mid))[0].sign, undefined);
});

test('header and announcement keep business handles whole despite visual wrapping', async () => {
  const header = 'B站微博油管同名  商务wx：cherrymiaojiang （备注来历）';
  const announcement = '微博油管同名 商务wx：cherrymiaojiang （备注来历）';
  const {page} = fixture({signs:[{text:header,title:header}], announcements:[announcement]});
  assert.equal((await readBilibiliProfile(page, mid))[0].sign, header + '\n' + announcement);
  const short = '商务cyx112656 桌椅国补群553583633';
  assert.equal((await readBilibiliProfile(fixture({signs:[{text:short}]}).page, mid))[0].sign, short);
  assert.equal((await readBilibiliProfile(fixture({announcements:[['商务wx：cherrymiaojiang', '备注来历']]}).page, mid))[0].sign,
    '商务wx：cherrymiaojiang\n备注来历');
});

test('wrong account, challenge, missing or duplicate evidence stops without guessing or retry', async () => {
  for (const options of [
    { href: 'https://space.bilibili.com/123' }, { href: 'https://space.bilibili.com.evil.example/8420618' },
    { body: '安全验证' }, { body: '请先登录后查看' }, { labels: ['关注数'] },
    { labels: ['粉丝数', '粉丝数'] }, { names: [] }, { names: ['one', 'two'] }, { tooltip: null },
  ]) {
    const { page, calls } = fixture(options);
    await assert.rejects(readBilibiliProfile(page, mid));
    assert.equal(calls.length, 5);
    assert.deepEqual(calls.at(-1), ['close', 'exact-profile']);
  }
});

test('closes only its owned tab after binding, wait or original-language failure', async () => {
  for (const operation of ['setActivePage', 'wait', 'evaluate']) {
    const { page, calls } = fixture();
    page[operation] = async () => { throw new Error('read unavailable'); };
    await assert.rejects(readBilibiliProfile(page, mid), /read unavailable/);
    assert.deepEqual(calls.filter(([kind]) => kind === 'close'), [['close', 'exact-profile']]);
  }
});

test('no owned tab means no attempt to close a user tab', async () => {
  const { page, calls } = fixture();
  page.newTab = async () => null;
  await assert.rejects(readBilibiliProfile(page, mid), /ONECREW_BILIBILI_PROFILE_IDENTITY/);
  assert.equal(calls.length, 0);
});

test('cleanup failure cannot replace the profile risk classification', async () => {
  const { page } = fixture({ body: '安全验证' });
  page.closeTab = async () => { throw new Error('cleanup unavailable'); };
  await assert.rejects(readBilibiliProfile(page, mid), /ONECREW_BILIBILI_PROFILE_RISK/);
  const success = fixture();
  success.page.closeTab = page.closeTab;
  await assert.rejects(readBilibiliProfile(success.page, mid), /cleanup unavailable/);
});

test('only a positive observed MID is accepted, never an arbitrary URL or option', async () => {
  for (const id of ['', '0', '01', '../123', 'https://space.bilibili.com/123', '--help', '1'.repeat(21), 123]) {
    const { page, calls } = fixture();
    await assert.rejects(readBilibiliProfile(page, id), /ONECREW_BILIBILI_PROFILE_INPUT/);
    assert.equal(calls.length, 0);
  }
});
