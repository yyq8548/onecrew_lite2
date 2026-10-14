import { useEffect, useState } from 'react';
import { api, request } from '@/lib/api';
import { useTranslation } from 'react-i18next';

type ConnectionState = 'checking' | 'online' | 'offline' | 'service-offline';
export function ConnectionsPage() {
  const { t } = useTranslation();
  const [state, setState] = useState<ConnectionState>('checking');
  useEffect(() => {
    let disposed = false;
    let timer: ReturnType<typeof setTimeout>;
    const controller = new AbortController();
    const refresh = async () => {
      try {
        const status = await request(
          api.browser.status.$get({}, { init: { signal: controller.signal } }),
        );
        if (!disposed) setState(status.connected ? 'online' : 'offline');
      } catch {
        if (!disposed) setState('service-offline');
      } finally {
        if (!disposed) timer = setTimeout(refresh, 3000);
      }
    };
    void refresh();
    return () => {
      disposed = true;
      clearTimeout(timer);
      controller.abort();
    };
  }, []);
  const label = {
    checking: t('connections.checking'),
    online: t('connections.online'),
    offline: t('connections.offline'),
    'service-offline': t('connections.serviceOffline'),
  }[state];
  return (
    <section className="content connections-content">
      <div className="page-heading">
        <h1>{t('connections.title')}</h1>
      </div>
      <div className="connection-card">
        <h2>{t('connections.extension')}</h2>
        <div
          className={`connection-state ${state === 'online' ? 'is-connected' : ''}`}
          role="status"
        >
          {label}
        </div>
        <p>{state === 'service-offline' ? t('connections.serviceHint') : t('connections.hint')}</p>
      </div>
    </section>
  );
}
