import { useEffect, useState } from 'react';
import { CheckCircle2, ExternalLink, Languages, LogOut, RefreshCw } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { Button } from '@/components/ui/button';
import { BrandLogo } from '@/components/BrandLogo';
import { useAccess } from './AccessProvider';
import { setLanguage } from '@/i18n';
import type { useRoute } from '@/lib/router';

type LoginPageProps = {
  navigate: ReturnType<typeof useRoute>['navigate'];
  returnTo?: string;
};

export function LoginPage({ navigate, returnTo = '/library' }: LoginPageProps) {
  const { t, i18n } = useTranslation();
  const { status, login, logout, refresh } = useAccess();
  const [popupBlocked, setPopupBlocked] = useState(false);
  const [authorizeUrl, setAuthorizeUrl] = useState<string | null>(null);
  const [error, setError] = useState(false);
  useEffect(() => {
    if (status?.state !== 'signed_in') return;
    const timer = window.setTimeout(() => navigate(returnTo, true), 2000);
    return () => window.clearTimeout(timer);
  }, [navigate, returnTo, status?.state]);
  const startLogin = async () => {
    setError(false);
    setPopupBlocked(false);
    const popup = window.open('', '_blank');
    if (!popup) setPopupBlocked(true);
    try {
      setAuthorizeUrl(await login(popup));
    } catch {
      popup?.close();
      setError(true);
    }
  };
  const unavailable = status?.state === 'unavailable';
  if (status?.state === 'signed_in') {
    return (
      <main className="login-page">
        <div className="login-card">
          <BrandLogo className="login-logo" />
          <CheckCircle2 size={40} aria-hidden="true" />
          <h1>{t('account.loginSuccess')}</h1>
          <p>{t('account.returningToApp')}</p>
          <Button variant="default" onClick={() => navigate(returnTo, true)}>
            {t('account.continueToApp')}
          </Button>
        </div>
      </main>
    );
  }
  return (
    <main className="login-page">
      <div className="login-card">
        <BrandLogo className="login-logo" />
        <h1>{t('account.loginTitle')}</h1>
        <p>{unavailable ? t('account.unavailable') : t('account.loginDescription')}</p>
        {(error || (status?.state === 'signed_out' && status.issue)) && (
          <p className="account-error">{t('account.loginError')}</p>
        )}
        {popupBlocked && (
          <p className="account-error">
            {t('account.popupBlocked')}{' '}
            {authorizeUrl ? (
              <a className="account-link" href={authorizeUrl} target="_blank" rel="noreferrer">
                {t('account.openLogin')}
              </a>
            ) : (
              t('account.tryAgain')
            )}
          </p>
        )}
        {status?.state === 'authorizing' ? (
          <Button variant="default" onClick={() => void logout()}>
            <LogOut size={16} />
            {t('account.cancel')}
          </Button>
        ) : unavailable ? (
          <>
            <Button variant="default" onClick={() => void refresh()}>
              <RefreshCw size={16} />
              {t('account.retry')}
            </Button>
            <Button variant="outline" onClick={() => void logout()}>
              <LogOut size={16} />
              {t('account.logoutLocal')}
            </Button>
          </>
        ) : (
          <Button variant="default" onClick={() => void startLogin()}>
            <ExternalLink size={16} />
            {t('account.login')}
          </Button>
        )}
        <Button
          variant="ghost"
          size="sm"
          onClick={() => setLanguage(i18n.language === 'en' ? 'zh-CN' : 'en')}
        >
          <Languages size={16} />
          {i18n.language === 'en' ? '中文' : 'English'}
        </Button>
      </div>
    </main>
  );
}
