import { useEffect, useRef } from 'react';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { Loader2 } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { Button } from '@/components/ui/button';
import { acceptDefaultSyncAgreement, hasDefaultSyncConsent, readTableSyncStatus, tableSyncQueryKey } from '@/lib/table-sync';

export function DefaultSyncAgreement({ credentialId, disabled, onContinue }: {
  credentialId: string | null; disabled: boolean; onContinue?: () => void;
}) {
  const { t } = useTranslation('shell');
  const client = useQueryClient();
  const currentCredential = useRef(credentialId);
  currentCredential.current = credentialId;
  const mounted = useRef(true);
  useEffect(() => {
    mounted.current = true;
    return () => { mounted.current = false; };
  }, []);
  const operation = useRef<{ scope: string; id: string } | null>(null);
  const status = useQuery({ queryKey: tableSyncQueryKey(credentialId), queryFn: () => readTableSyncStatus(),
    enabled: !!credentialId, retry: false, staleTime: 30_000 });
  const accept = useMutation({
    retry: false,
    mutationFn: async () => {
      if (!status.data || !credentialId) throw new Error('table_sync_policy_unavailable');
      const scope = JSON.stringify([credentialId, status.data.capabilities.policy_fingerprint]);
      if (operation.current?.scope !== scope) operation.current = { scope, id: crypto.randomUUID() };
      await acceptDefaultSyncAgreement(status.data, operation.current.id);
      return credentialId;
    },
    onSuccess: acceptedCredential => {
      operation.current = null;
      void client.invalidateQueries({ queryKey: ['table-backend-sync'] });
      if (mounted.current && currentCredential.current === acceptedCredential) onContinue?.();
    },
  });
  const policy = status.data?.capabilities.policy;
  const confirmed = !!status.data && hasDefaultSyncConsent(status.data);
  const canAccept = !!policy && !!status.data?.capabilities.policy_fingerprint
    && status.data.capabilities.consent_management_enabled;
  return <div className="grid gap-3 sm:grid-cols-[minmax(0,1fr)_auto] sm:items-center">
    <p className="max-w-prose text-sm text-muted-foreground sm:col-start-1">{t(confirmed ? 'opencli.defaultAgreementConfirmed' : 'opencli.defaultAgreementHelp')}</p>
    {!confirmed && policy && <a className="text-sm underline underline-offset-4 sm:col-start-1" href={policy.document_url} target="_blank" rel="noreferrer">{t('opencli.defaultAgreementLink')}</a>}
    {(status.isError || (!status.isPending && !canAccept)) && <p role="alert" className="text-sm">{t('opencli.defaultAgreementUnavailable')}</p>}
    {accept.isError && <p role="alert" className="text-sm">{t('opencli.defaultAgreementFailed')}</p>}
    <div className="flex flex-wrap gap-2 sm:col-start-2 sm:row-start-1 sm:row-span-2">
      <Button className="min-h-11 w-full whitespace-normal px-5 py-3 sm:w-auto" disabled={disabled || !canAccept || status.isFetching || accept.isPending}
        onClick={() => confirmed ? onContinue?.() : accept.mutate()}>
        {accept.isPending && <Loader2 className="size-4 animate-spin" aria-hidden />}
        {t(accept.isPending ? 'opencli.defaultAgreementSaving' : confirmed ? 'opencli.enterSearchConfirmed' : 'opencli.enterSearch')}
      </Button>
      {(status.isError || accept.isError || (!status.isPending && !canAccept)) && <Button variant="outline" disabled={status.isFetching || accept.isPending}
        onClick={() => { accept.reset(); void status.refetch(); }}>{t('opencli.defaultAgreementRefresh')}</Button>}
    </div>
  </div>;
}
