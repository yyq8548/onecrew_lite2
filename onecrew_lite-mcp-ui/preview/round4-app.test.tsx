import {it,expect} from 'vitest';
import {renderToStaticMarkup} from 'react-dom/server';
import {Round4Session} from './round4-app';
import {round4Cases,round4Initial,round4Change} from '../src/test-support/round4';
import {creatorBatchSelections,initialCreatorBatchDraft,rebaseCreatorBatchDraft} from '../src/features/result-card/creator-batch-state';
it.each(round4Cases)('fourth round $id renders $title',({id})=>{expect(renderToStaticMarkup(<Round4Session id={id}/>)).toContain('模拟操作记录');});
it.each([5,6,7])('revision %i excludes stale selections and routes',id=>{const old=round4Initial(id);if(old.pending_action?.kind!=='creator_save_batch')throw Error('fixture');const draft=initialCreatorBatchDraft(old.pending_action.review.rows),next=round4Change(old,id);if(next.pending_action?.kind!=='creator_save_batch')throw Error('fixture');const rows=next.pending_action.review.rows;const selections=creatorBatchSelections(rows,rebaseCreatorBatchDraft(rows,draft));expect(BigInt(next.binding.revision)).toBe(BigInt(old.binding.revision)+1n);if(id===6)expect(selections.some(s=>s.row_id==='save.creator.1')).toBe(false);if(id===7)expect(selections.find(s=>s.row_id==='save.creator.1')?.confirmed_route_ids).toEqual(['route-profile']);expect(old.binding.revision).toBe(2);});
it('existing candidates remain pending until confirmation',()=>{const p=round4Initial(4).pending_action;if(p?.kind!=='creator_save_batch')throw Error('fixture');expect(creatorBatchSelections(p.review.rows,initialCreatorBatchDraft(p.review.rows))).toHaveLength(5);});
it('renders injected markup as inert text',()=>{const html=renderToStaticMarkup(<Round4Session id={15}/>);expect(html).toContain('&lt;img');expect(html).not.toContain('<img src=x');});
