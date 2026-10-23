import {createElement} from 'react';
import {renderToStaticMarkup} from 'react-dom/server';
import {it,expect} from 'vitest';
import {ResultCardView} from './view';
import {creatorSavedSnapshot} from '../../test-support/creator-save-fixture';
import {localizedScenario} from '../../test-support/demo-locale';
import {parseResultSnapshot} from '../../lib/result-contract';
const handlers={busy:false,onConfirm(){},onCancel(){},onSelect(){},onRequestEdit(){}};
it('shows two useful fields without disclosure and preserves exact wire values',()=>{
 const snapshot=creatorSavedSnapshot();snapshot.presentation.sections=[{kind:'fields',heading:null,fields:[{label:'Platform',value:'X'},{label:'Followers',value:'12500'}]}];
 const before=structuredClone(snapshot);const html=renderToStaticMarkup(createElement(ResultCardView,{snapshot,...handlers}));
 expect(html).toContain('粉丝');expect(html).toContain('12500');expect(html).not.toContain('<details');expect(snapshot).toEqual(before);
});
it('does not repeat a saved resource name or expose its internal ID as business content',()=>{
 const html=renderToStaticMarkup(createElement(ResultCardView,{snapshot:creatorSavedSnapshot(),...handlers}));
 expect(html).toContain('creator-platform-badge');expect(html).toContain('@beauty_demo');expect(html.match(/Demo beauty creator/g)).toHaveLength(1);expect(html).not.toContain('creator-demo');expect(html).not.toContain('<details');
});
it.each(['zh-CN','en-US'] as const)('all 20 localized fixtures preserve valid typed snapshots in %s',locale=>{
 for(let i=0;i<20;i++){const s=localizedScenario(i,locale);if(s.snapshot)expect(parseResultSnapshot(s.snapshot)).toEqual(s.snapshot);}
});
it('keeps different resources even when their display names are identical',()=>{
 const snapshot=creatorSavedSnapshot();snapshot.presentation.sections=[];snapshot.batch.outcomes[0]!.resource_refs.push({kind:'creator',id:'creator-other',label:'Demo beauty creator'});
 const html=renderToStaticMarkup(createElement(ResultCardView,{snapshot,...handlers}));
 expect(html.match(/Demo beauty creator/g)).toHaveLength(2);
});
