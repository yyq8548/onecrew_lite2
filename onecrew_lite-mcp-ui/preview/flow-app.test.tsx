import {createElement} from 'react';
import {renderToStaticMarkup} from 'react-dom/server';
import {expect,it} from 'vitest';
import {countryOptions} from './country-picker';
import {FlowSession,resultPage} from './flow-app';
import {flowCases,initialFlow,flowReducer,type CaseId,type Phase} from '../src/test-support/flow-simulator';
function render(id:CaseId,phase?:Phase){const state=initialFlow(id);if(phase)state.phase=phase;return renderToStaticMarkup(createElement(FlowSession,{id,initialState:state}));}
it.each(flowCases)('renders entry state for $id',c=>{const html=render(c.id);if(!['search-limited','search-unavailable'].includes(c.id))expect(html).toContain('data-onecrew-brand');expect(html).not.toContain('待确认');expect(html).not.toContain('role="dialog"');});
it('offers only readback for an unknown write result',()=>{const html=render('edit-unknown-applied','unknown');expect(html).toContain('检查当前状态');expect(html).not.toContain('>保存修改<');expect(html).not.toContain('>确认计划<');});
it('does not offer continuation for unverified or denied accounts',()=>{for(const phase of ['unverified','denied','check-error'] as const)expect(render('account-denied',phase)).not.toContain('>继续搜索<');expect(render('account-missing','ready')).toContain('>继续搜索<');});
it('preserves both conflict choices and hides premature confirmation',()=>{const html=render('edit-conflict','conflict');expect(html).toContain('最新已保存的范围');expect(html).toContain('你的修改');expect(html).toContain('使用最新范围');expect(html).not.toContain('>确认计划<');});

it.each(['tiktok','instagram','youtube','x'] as const)('allows requested platform %s to be reselected',platform=>{const state=initialFlow('search-results');state.fields.platform=platform;const html=renderToStaticMarkup(createElement(FlowSession,{id:state.id,initialState:state}));const label={tiktok:'TikTok',instagram:'Instagram',youtube:'YouTube',x:'X'}[platform];expect(html).toContain(label);expect(html).toContain('>搜索达人<');expect(html).not.toContain(`在 ${label} 搜索达人`);expect(html).not.toContain('搜索平台：');expect(html).toContain('result-shell-header');expect(html).toContain('result-shell-footer');});

it('paginates 23 results without omissions',()=>{const pages=[0,1,2].map(p=>resultPage('1000','all',p));expect(pages.map(p=>p.visible.length)).toEqual([10,10,3]);expect(new Set(pages.flatMap(p=>p.visible.map(person=>person[1]))).size).toBe(23);});
it('filters follower bands within the original search scope',()=>{expect(resultPage('10000','under10k',0).matches).toHaveLength(0);expect(resultPage('1000','10k50k',0).matches.every(p=>p[2]>=10000&&p[2]<50000)).toBe(true);expect(resultPage('1000','50kplus',0).matches.every(p=>p[2]>=50000)).toBe(true);});

it('combines region, tag and follower filters using result metadata',()=>{const result=resultPage('1000','10k50k',0,'英国','家常菜','美国,英国','cooking');expect(result.matches.length).toBeGreaterThan(0);expect(result.matches.every(p=>p[3]==='英国'&&p[4].includes('家常菜')&&p[2]>=10000&&p[2]<50000)).toBe(true);expect(result.regions).toEqual(['美国','英国']);expect(resultPage('1000','all',0,'日本').matches).toHaveLength(0);});
it('renders region, tag and follower dropdowns with visible result metadata',()=>{const html=render('search-results','results');expect(html.match(/role="combobox"/g)).toHaveLength(3);for(const label of ['全部地区','全部标签','全部粉丝数','flow-result-metadata'])expect(html).toContain(label);});

it('shows editing context when returning from results',()=>{const state=initialFlow('search-results');state.attempt=1;const html=renderToStaticMarkup(createElement(FlowSession,{id:state.id,initialState:state}));expect(html).toContain('修改搜索条件');expect(html).toContain('渠道');expect(html).toContain('country__control');});

it('provides localized ISO country options',()=>{expect(countryOptions.length).toBeGreaterThan(200);expect(countryOptions.find(option=>option.value==='US')?.label).toBe('美国');expect(countryOptions.find(option=>option.value==='JP')?.label).toBe('日本');});
it('starts a new search with the changed channel and regions',()=>{let state=initialFlow('search-results');state.attempt=1;state=flowReducer(state,{type:'change',key:'platform',value:'youtube'});state=flowReducer(state,{type:'change',key:'market',value:'日本,英国'});state=flowReducer(state,{type:'start'});expect(state.phase).toBe('running');expect(state.fields.platform).toBe('youtube');expect(state.fields.market).toBe('日本,英国');});

it.each(['search-limited','search-unavailable'] as const)('renders actionable centered system message for %s',id=>{const html=render(id);expect(html).toContain('flow-system-message');expect(html).toContain('你的搜索条件已保留');expect(html).toContain("暂不搜索");expect(html).toContain(id==='search-limited'?'稍后可重试':'重新搜索');expect(html).not.toMatch(/<input|<select|role="combobox"|data-onecrew-brand/);});

it('places expired-account explanation in the header and avoids a duplicate channel label',()=>{const html=render('account-expired');const description=html.indexOf('原授权已过期，需要重新授权后再检查搜索权限。');expect(description).toBeGreaterThan(html.indexOf('data-slot="card-description"'));expect(description).toBeLessThan(html.indexOf('data-slot="card-content"'));expect(html).not.toContain('>TikTok</span>');expect(html).toContain('TikTok · Nora Kitchen');});

it.each(['tiktok','instagram','youtube','x'] as const)('shows the scope platform only once: %s',platform=>{const state=initialFlow('edit-success');state.fields.platform=platform;state.saved.platform=platform;const html=renderToStaticMarkup(createElement(FlowSession,{id:state.id,initialState:state}));const text=html.split('<details')[0].replace(/<[^>]*>/g,' ').replace(/\s+/g,' ');const label={tiktok:'TikTok',instagram:'Instagram',youtube:'YouTube',x:'X'}[platform];expect(text.split(label).length-1).toBe(1);});
it('does not duplicate matching keyword and topic tags',()=>{const result=resultPage('1000','all',0,'all','all','美国','家常菜');for(const person of result.eligible)expect(new Set(person[4]).size).toBe(person[4].length);});
