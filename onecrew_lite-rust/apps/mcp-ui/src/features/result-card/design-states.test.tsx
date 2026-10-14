import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { describe,it,expect } from 'vitest';
import { scenarios } from '../../test-support/design-scenarios';
import { parseResultSnapshot } from '../../lib/result-contract';
import { ResultCardView } from './view';
import { resultStatusPresentation, continuationStatusCopy } from './presentation';
import { CreatorReviewList } from './components/creator-review-list';
import { creatorBatchReview } from '../../test-support/creator-batch-fixture';
import { creatorBatchResultSnapshot,creatorBatchSnapshot } from '../../test-support/creator-batch-fixture';
import { translate } from '../../lib/i18n';
const handlers={onConfirm(){},onCancel(){},onSelect(){},onRequestEdit(){},onConfirmCreatorSaveBatch(){}};
describe('User and machine result separation',()=>{
 it.each(scenarios.filter(s=>s.snapshot))('$question: keeps the exact machine payload while rendering',s=>{
  const snapshot=s.snapshot!; const original=structuredClone(snapshot);
  expect(parseResultSnapshot(snapshot)).toEqual(snapshot);
  const html=renderToStaticMarkup(createElement(ResultCardView,{snapshot,busy:false,...handlers,revisionBlocked:s.recovery,recoveryFailure:s.recovery?{kind:'transport'}:null,onRecover(){}}));
  expect(html).toContain('data-onecrew-brand');expect(snapshot).toEqual(original);
  const text=html.replace(/<[^>]*>/g,' ').replace(/\s+/g,' ');expect(text).not.toMatch(/\b(TikTok|Instagram|YouTube)\s+\1\b/);
  if(snapshot.state.kind!=='available')expect(html).not.toMatch(/>确认添加<\/button>/);
 });
 it('does not infer all-success from a partially completed batch',()=>{
  const snapshot=creatorBatchResultSnapshot();
  expect(resultStatusPresentation(snapshot).terminal?.tone).toBe('warning');
  const html=renderToStaticMarkup(createElement(ResultCardView,{snapshot,busy:false,...handlers}));
  expect(html).toContain('aria-label="已新增"');expect(html).toContain('aria-label="失败"');
  for(const label of ['结果未知','未执行','未选择'])expect(html).toContain(label);
 });
 it('retains success when continuation is paused and does not repeat delivered notices',()=>{
  expect(continuationStatusCopy('delivered','zh-CN')).toBeNull();
  const s=scenarios[18]!.snapshot!;
  expect(resultStatusPresentation(s).terminal?.tone).toBe('success');expect(continuationStatusCopy('paused','zh-CN')).toBeTruthy();
 });
 it('shows only owner-projected identity blockers and preserves machine evidence',()=>{
  const rows=creatorBatchReview().rows;const before=structuredClone(rows);
  const render=(selectedIds:string[],optionalIds:Record<string,string[]>)=>renderToStaticMarkup(createElement(CreatorReviewList,{rows,selectedIds,optionalIds,onSelect(){},onRoute(){},disabled:false}));
  expect(render([],{})).not.toContain('data-review-warning');
  expect(render([rows[0]!.row_id],{})).not.toContain('data-review-warning');
  expect(render([rows[0]!.row_id],{[rows[0]!.row_id]:['route-email']})).not.toContain('data-review-warning');
  const pendingRows=rows.map((row,index)=>index===0?{...row,preview:{...row.preview,status:'identity_pending' as const,reason:'identity_ambiguous' as const}}:row);
  const pending=renderToStaticMarkup(createElement(CreatorReviewList,{rows:pendingRows,selectedIds:[rows[0]!.row_id],optionalIds:{},onSelect(){},onRoute(){},disabled:false}));
  expect(pending).toContain(translate('en-US','creatorSaveReasonIdentityAmbiguous'));
  expect(rows).toEqual(before);
 });
});
