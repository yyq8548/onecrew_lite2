import { useEffect, useRef, useState } from 'react';
import {
  Check,
  Copy,
  ExternalLink,
  Languages,
  LogIn,
  LogOut,
  RefreshCw,
  Settings,
} from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { Avatar, AvatarFallback } from '@/components/ui/avatar';
import { Button } from '@/components/ui/button';
import { Popover, PopoverContent, PopoverTitle, PopoverTrigger } from '@/components/ui/popover';
import { api, request } from '@/lib/api';
import type { InferResponseType } from 'hono/client';
import { setLanguage } from '@/i18n';
type SystemInfo = InferResponseType<typeof api.system.info.$get, 200>;

import { useAccess } from './AccessProvider';
import { unifiedRuntimeMode } from '@/lib/unified-runtime';

const requestHeaders = { 'X-OneCrew-Request': '1' };

export function AccountMenu({ navigate }: { navigate: (path: string) => void }) {
  const { t, i18n } = useTranslation();
  const { status, refresh, login, logout } = useAccess();
  const [open, setOpen] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [popupBlocked, setPopupBlocked] = useState(false);
  const [authorizeUrl, setAuthorizeUrl] = useState<string | null>(null);
  const [systemInfo, setSystemInfo] = useState<SystemInfo | null>(null);
  const [systemInfoLoading, setSystemInfoLoading] = useState(false);
  const [systemInfoError, setSystemInfoError] = useState(false);
  const [copied, setCopied] = useState(false);
  const popupRef = useRef<Window | null>(null);
  const systemInfoAttemptedRef = useRef(false);

  useEffect(() => {
    if (!open) {
      systemInfoAttemptedRef.current = false;
      return;
    }
    if (systemInfoAttemptedRef.current) return;
    systemInfoAttemptedRef.current = true;
    setSystemInfoLoading(true);
    void request(api.system.info.$get({}, { headers: requestHeaders }))
      .then((info) => {
        setSystemInfo(info);
        setSystemInfoError(false);
      })
      .catch(() => setSystemInfoError(true))
      .finally(() => setSystemInfoLoading(false));
  }, [open]);

  const startLogin = async () => {
    if (unifiedRuntimeMode) {
      setOpen(false);
      navigate('/login');
      return;
    }
    setError(null);
    setPopupBlocked(false);
    popupRef.current = window.open('', '_blank');
    if (!popupRef.current) setPopupBlocked(true);
    try {
      setAuthorizeUrl(await login(popupRef.current));
    } catch {
      popupRef.current?.close();
      popupRef.current = null;
      setError(t('account.loginError'));
    }
  };

  const copyDiagnostics = async () => {
    if (!systemInfo) return;
    try {
      await navigator.clipboard.writeText(JSON.stringify(systemInfo, null, 2));
      setCopied(true);
      window.setTimeout(() => setCopied(false), 1800);
    } catch {
      setError(t('account.copyError'));
    }
  };

  const signOut = async () => {
    setError(null);
    try {
      await logout();
      setOpen(false);
    } catch {
      setError(t('account.logoutError'));
    }
  };

  if (!status) return null;
  const signedIn = status.state === 'signed_in' && Boolean(status.user?.id);
  const identifier = status.user?.email || status.user?.id || '';
  const avatarInitial = (
    status.user?.email?.trim()?.[0] ||
    status.user?.id?.trim()?.[0] ||
    '?'
  ).toUpperCase();
  const triggerLabel = signedIn ? identifier : t('account.login');
  const architecture =
    systemInfo?.os.native_arch === 'arm64'
      ? t('account.arm64')
      : systemInfo?.os.native_arch === 'x64'
        ? t('account.x64')
        : systemInfo?.os.native_arch || t('account.architectureUnknown');

  return (
    <div className="account-menu">
      <Popover open={open} onOpenChange={setOpen}>
        <PopoverTrigger
          render={
            <Button variant="account-trigger" title={triggerLabel} aria-label={triggerLabel}>
              <span className="account-avatar-wrap">
                <Avatar size="sm">
                  <AvatarFallback>{signedIn ? avatarInitial : <LogIn size={14} />}</AvatarFallback>
                </Avatar>
              </span>
              <span className="account-trigger-label">{triggerLabel}</span>
            </Button>
          }
        />
        <PopoverContent variant="account" side="top" align="start" sideOffset={10}>
          <div className="account-popover-header">
            <span className="account-avatar-wrap account-avatar-large-wrap">
              <Avatar size="lg">
                <AvatarFallback>{signedIn ? avatarInitial : <LogIn size={18} />}</AvatarFallback>
              </Avatar>
            </span>
            <div className="account-popover-title-wrap">
              <PopoverTitle>
                {signedIn ? t('account.signedInTitle') : t('account.loginTitle')}
              </PopoverTitle>
              {signedIn ? (
                <span className="account-status">
                  <Check size={13} />
                  {t('account.signedIn')}
                </span>
              ) : status.state === 'authorizing' ? (
                <span className="account-status account-status-pending">
                  {t('account.authorizing')}
                </span>
              ) : (
                <span className="account-popover-subtitle">{t('account.notSignedIn')}</span>
              )}
            </div>
          </div>
          {signedIn && (
            <div className="account-identifier" title={identifier}>
              {identifier}
            </div>
          )}
          {error && (
            <p className="account-error" role="alert">
              {error}
            </p>
          )}
          {popupBlocked && (
            <p className="account-error">
              {t('account.popupBlocked')}{' '}
              {authorizeUrl ? (
                <a className="account-link" href={authorizeUrl} target="_blank" rel="noreferrer">
                  {t('account.openLogin')}
                </a>
              ) : (
                <button type="button" className="account-link" onClick={startLogin}>
                  {t('account.tryAgain')}
                </button>
              )}
            </p>
          )}
          <div className="account-divider" />
          {signedIn ? (
            <>
              <Button
                variant="account-action"
                onClick={() => {
                  setOpen(false);
                  navigate('/connections');
                }}
              >
                <Settings size={16} />
                {t('account.manageConnections')}
              </Button>
              <Button variant="account-action" onClick={() => void signOut()}>
                <LogOut size={16} />
                {t('account.logout')}
              </Button>
            </>
          ) : status.state === 'unavailable' ? (
            <>
              <Button variant="account-action" onClick={() => void refresh()}>
                <RefreshCw size={16} />
                {t('account.retry')}
              </Button>
              <Button variant="account-action" onClick={() => void signOut()}>
                <LogOut size={16} />
                {t('account.logoutLocal')}
              </Button>
            </>
          ) : status.state === 'authorizing' ? (
            <Button variant="account-action" onClick={() => void logout()}>
              <LogOut size={16} />
              {t('account.cancel')}
            </Button>
          ) : (
            <Button variant="account-action" onClick={() => void startLogin()}>
              <ExternalLink size={16} />
              {t('account.login')}
            </Button>
          )}
          <div className="account-device">
            <div className="account-device-heading">{t('account.device')}</div>
            {systemInfoLoading ? (
              <div className="account-device-muted">{t('account.deviceLoading')}</div>
            ) : systemInfoError ? (
              <div className="account-device-muted">{t('account.deviceUnavailable')}</div>
            ) : systemInfo ? (
              <div className="account-device-summary">
                <span>
                  {systemInfo.os.name} {systemInfo.os.version}
                </span>
                <span>{architecture}</span>
              </div>
            ) : null}
            <Button
              variant="account-action"
              onClick={() => void copyDiagnostics()}
              disabled={!systemInfo || copied}
            >
              <Copy size={15} />
              {copied ? t('account.copied') : t('account.copyDiagnostics')}
            </Button>
          </div>
        </PopoverContent>
      </Popover>
      <Button
        variant="language-toggle"
        size="icon-xl"
        aria-label={i18n.language === 'en' ? '切换为中文' : 'Switch to English'}
        title={i18n.language === 'en' ? '切换为中文' : 'Switch to English'}
        onClick={() => setLanguage(i18n.language === 'en' ? 'zh-CN' : 'en')}
      >
        <Languages size={19} />
      </Button>
    </div>
  );
}
