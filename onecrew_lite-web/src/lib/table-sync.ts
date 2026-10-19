import { invoke } from './local-runtime';

export type TableSyncStatus = {
  enabled: boolean; status: string; error: string | null; version_id: string | null;
  capabilities: {
    consent_management_enabled: boolean; upload_enabled: boolean; consent_current: boolean;
    policy_fingerprint: string | null;
    policy: { document_url: string; deletion_url: string; purpose: string; retention_days: number | null } | null;
    consent?: { status: string; scopes: string[] } | null;
  };
};
export const tableSyncQueryKey = (credentialId: string | null, campaignId = '') => ['table-backend-sync', credentialId, campaignId] as const;
export const readTableSyncStatus = (campaignId = '') => invoke<TableSyncStatus>('onecrew_table_sync_status', { campaignId });
export const configureTableSync = (campaignId: string, enabled: boolean) =>
  invoke('onecrew_table_sync_configure', { input: { campaign_id: campaignId, enabled } });
export const changeTableSyncConsent = (action: string, operationId: string, policyFingerprint: string | null) =>
  invoke<{ consent: { status: string } }>('onecrew_table_sync_consent', {
    input: { action, operation_id: operationId, policy_fingerprint: policyFingerprint },
  });

export function hasDefaultSyncConsent(status: TableSyncStatus): boolean {
  const cap = status.capabilities;
  return cap.consent_current && cap.upload_enabled && cap.consent?.status === 'granted'
    && ['dynamic_table_records', 'dynamic_table_schema'].every(scope => cap.consent?.scopes.includes(scope));
}

/** Called only by the user's agreement button. Rendering never grants consent. */
export async function acceptDefaultSyncAgreement(status: TableSyncStatus, operationId: string): Promise<void> {
  if (hasDefaultSyncConsent(status)) return;
  const cap = status.capabilities;
  if (!cap.consent_management_enabled || !cap.policy || !cap.policy_fingerprint) {
    throw new Error('table_sync_policy_unavailable');
  }
  const result = await changeTableSyncConsent('grant', operationId, cap.policy_fingerprint);
  if (result.consent?.status !== 'granted') throw new Error('table_sync_consent_unconfirmed');
}
