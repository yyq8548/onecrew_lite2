import { AlertCircle, CheckCircle2, CircleX, LogIn, RefreshCw } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { Button } from '@/components/ui/button';
import { BrandLogo } from '@/components/BrandLogo';
import type { useRoute } from '@/lib/router';
import { useAccess } from './AccessProvider';

type AccessCompletePageProps = { navigate: ReturnType<typeof useRoute>['navigate'] };

export function AccessCompletePage({ navigate }: AccessCompletePageProps) {
  const { t } = useTranslation();
  const { status, refresh } = useAccess();

  const signedIn = status?.state === 'signed_in';
  const unavailable = status?.state === 'unavailable';
  return (
    <main className="login-page">
      <div className="login-card">
        <BrandLogo className="login-logo" />
        {signedIn ? (
          <CheckCircle2 size={40} aria-hidden="true" />
        ) : unavailable ? (
          <AlertCircle size={32} aria-hidden="true" />
        ) : status?.state === 'signed_out' ? (
          <CircleX size={32} aria-hidden="true" />
        ) : (
          <RefreshCw size={32} aria-hidden="true" />
        )}
        <h1>
          {signedIn
            ? t('account.loginSuccess')
            : unavailable
              ? t('account.completionUnavailable')
              : status?.state === 'signed_out'
                ? t('account.completionFailed')
                : t('account.completionChecking')}
        </h1>
        {signedIn ? (
          <>
            <p>{t('account.completionCloseHint')}</p>
            <Button variant="default" onClick={() => navigate('/library', true)}>
              {t('account.openonecrew_lite')}
            </Button>
          </>
        ) : unavailable ? (
          <>
            <p>{t('account.completionUnavailableHint')}</p>
            <Button variant="default" onClick={() => void refresh()}>
              <RefreshCw size={16} />
              {t('account.retry')}
            </Button>
          </>
        ) : status?.state === 'signed_out' ? (
          <>
            <p>{t('account.completionFailedHint')}</p>
            <Button variant="default" onClick={() => navigate('/login', true)}>
              <LogIn size={16} />
              {t('account.returnToLogin')}
            </Button>
          </>
        ) : (
          <p>{t('account.completionCheckingHint')}</p>
        )}
      </div>
    </main>
  );
}
