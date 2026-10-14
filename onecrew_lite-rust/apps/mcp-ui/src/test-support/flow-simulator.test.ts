import {expect,it} from 'vitest';
import {flowCases,flowReducer,initialFlow,type CaseId,type FlowState,type FlowEvent} from './flow-simulator';
const event=(s:FlowState,type:FlowEvent['type'])=>flowReducer(s,{type,run:s.run} as FlowEvent);
const save=(id:CaseId)=>{let s=event(initialFlow(id),'edit');s=flowReducer(s,{type:'change',key:'minimum',value:'2500'});return event(s,'save');};
it('requires explicit verification after authorization and prevents premature continuation',()=>{
 let s=event(initialFlow('account-missing'),'connect');s=event(s,'finish');expect(s.phase).toBe('unverified');expect(event(s,'continue')).toEqual(s);
 s=event(event(s,'check-account'),'finish');expect(s.phase).toBe('ready');expect(event(s,'continue').phase).toBe('continued');
});
it.each(['edit-unknown-applied','edit-unknown-missing'] as const)('queries an unknown write without submitting it again: %s',id=>{
 let s=event(save(id),'finish');expect(s.phase).toBe('unknown');expect(event(s,'save')).toEqual(s);expect(event(s,'confirm')).toEqual(s);
 s=event(event(s,'check-result'),'finish');expect(s.phase).toBe(id==='edit-unknown-applied'?'preview':'edit-error');expect(s.attempt).toBe(1);expect(s.fields.minimum).toBe('2500');
 if(id==='edit-unknown-applied')expect(s.saved.minimum).toBe('2500');else expect(s.saved.minimum).toBe('1000');
});
it('does not overwrite a newer plan until the user rebases and saves',()=>{
 let s=event(save('edit-conflict'),'finish');expect(s.saved.minimum).toBe('5000');expect(s.fields.minimum).toBe('2500');expect(event(s,'save')).toEqual(s);expect(event(s,'confirm')).toEqual(s);
 s=event(event(event(s,'rebase'),'save'),'finish');expect(s.saved.minimum).toBe('2500');expect(s.revision).toBe(3);
});
it('ignores stale results and double clicks after cancellation',()=>{
 const s=event(initialFlow('search-results'),'start');expect(event(s,'start')).toEqual(s);const cancelled=event(s,'cancel');expect(flowReducer(cancelled,{type:'finish',run:s.run})).toEqual(cancelled);
});
it.each(flowCases.filter(c=>c.flow==='search'))('runs the search branch and recovers: $id',c=>{
 let s=event(event(initialFlow(c.id),'start'),'finish');
 if(s.phase==='limited'){expect(event(s,'retry')).toEqual(s);s=event(s,'cooled');}
 if(s.phase==='empty')s=event(event(s,'edit'),'start');else if(s.phase==='error'||s.phase==='limited')s=event(s,'retry');
 if(s.phase==='running')s=event(s,'finish');expect(s.phase).toBe('results');
});
it('keeps unsaved edits after a known rejection and requires a valid changed form',()=>{
 let s=event(save('edit-failed'),'finish');expect(s.phase).toBe('edit-error');expect(s.saved.minimum).toBe('1000');expect(s.fields.minimum).toBe('2500');
 s=flowReducer(s,{type:'change',key:'minimum',value:'-1'});expect(event(s,'save')).toEqual(s);
});
it.each(['account-denied','account-timeout','account-cancel','account-expired'] as const)('recovers account branch: %s',id=>{
 let s=initialFlow(id);
 if(s.phase==='expired'||s.phase==='disconnected')s=event(event(s,'connect'),'finish');
 if(s.phase==='cancelled')s=event(event(s,'connect'),'finish');
 s=event(event(s,'check-account'),'finish');
 if(s.phase==='denied')s=event(event(event(event(s,'connect'),'finish'),'check-account'),'finish');
 if(s.phase==='check-error')s=event(event(s,'check-account'),'finish');expect(s.phase).toBe('ready');
});

it('allows discarding conflicted edits in favour of the latest saved plan',()=>{
 const s=event(save('edit-conflict'),'finish');const latest=event(s,'use-latest');expect(latest.phase).toBe('preview');expect(latest.fields).toEqual(s.saved);expect(latest.revision).toBe(2);
});
