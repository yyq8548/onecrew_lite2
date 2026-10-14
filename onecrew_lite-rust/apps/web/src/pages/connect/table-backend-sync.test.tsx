import { describe, expect, it, vi } from 'vitest';
import { renderToStaticMarkup } from 'react-dom/server';
const current = vi.hoisted(() => ({ data: undefined as unknown, error: false }));
vi.mock('@tanstack/react-query', () => ({
  useQuery: () => ({data:current.data,isError:current.error}),
  useQueryClient: () => ({invalidateQueries:vi.fn()}),
  useMutation: () => ({isPending:false,isError:false,mutate:vi.fn()}),
}));
vi.mock('react-i18next', () => ({useTranslation:() => ({i18n:{language:'zh-CN'}})}));
import { TableBackendSync } from './table-backend-sync';
describe('recommendation cloud consent and status', () => {
  it('keeps account settings free of repeated consent and per-table enable prompts', () => {
    current.error=false;current.data={enabled:false,status:'disabled',error:null,capabilities:{upload_enabled:true,consent_current:true}};
    const html=renderToStaticMarkup(<TableBackendSync/>);
    expect(html).toContain('结果表默认后台同步');
    expect(html).not.toContain('确认上传授权');
    expect(html).not.toContain('开启本表同步');
    expect(html).toContain('撤回后台上传授权');
  });
  it('does not preselect consent or imply that waiting means uploaded', () => {
    current.error=false;current.data={enabled:false,status:'waiting',error:null,capabilities:{upload_enabled:false,consent_management_enabled:true,policy_fingerprint:'a'.repeat(64),policy:{document_url:'https://example.test/privacy',deletion_url:'https://example.test/delete',purpose:'usage_analytics',retention_days:null}}};
    const html=renderToStaticMarkup(<TableBackendSync campaignId="campaign"/>);
    expect(html).toContain('无固定期限');expect(html).toContain('等待同步');expect(html).not.toContain('已同步');
    expect(html).not.toContain('checked=');expect(html).toMatch(/<button[^>]*disabled=""[^>]*>确认上传授权/);
    expect(html).toContain('https://example.test/privacy');
  });
  it('keeps consent revocation accessible when capability lookup fails', () => {
    current.data=undefined;current.error=true;
    const html=renderToStaticMarkup(<TableBackendSync campaignId="campaign"/>);
    expect(html).toContain('后台能力暂时无法确认');
    const buttons=[...html.matchAll(/<button([^>]*)>(.*?)<\/button>/g)];
    const revoke=buttons.find(b=>b[2].includes('撤回后台上传授权'));expect(revoke).toBeDefined();expect(revoke![1]).not.toMatch(/\sdisabled(?:=|\s|$)/);
  });
});
