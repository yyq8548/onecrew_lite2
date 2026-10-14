import { useState, type ReactNode } from 'react';
import { PanelLeftClose, PanelLeftOpen } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { useTranslation } from 'react-i18next';
import { AccountMenu } from '@/features/access/AccountMenu';
import oneCrewIcon from '@/assets/onecrew-icon.svg';
import { BrandLogo } from '@/components/BrandLogo';

const sidebarPreferenceKey = 'onecrew-sidebar-collapsed';
export function AppShell({
  children,
  navigate,
  active,
}: {
  children: ReactNode;
  navigate: (path: string) => void;
  active?: 'library' | 'connections';
}) {
  const { t } = useTranslation();
  const [collapsed, setCollapsed] = useState(() => {
    try {
      return window.localStorage.getItem(sidebarPreferenceKey) === 'true';
    } catch {
      return false;
    }
  });
  const toggleCollapsed = () => {
    const next = !collapsed;
    setCollapsed(next);
    try {
      window.localStorage.setItem(sidebarPreferenceKey, String(next));
    } catch {
      // Keep the current preference for this mounted app when storage is unavailable.
    }
  };
  return (
    <main className="app-shell">
      <div className="workspace" data-sidebar-collapsed={collapsed}>
        <aside className="sidebar">
          <div className="sidebar-header">
            <button className="brand" onClick={() => navigate('/library')}>
              <BrandLogo />
            </button>
            <div className="sidebar-collapse-toggle">
              <Button
                variant="sidebar-toggle"
                size="icon-xl"
                type="button"
                aria-label={t(collapsed ? 'nav.expandSidebar' : 'nav.collapseSidebar')}
                title={t(collapsed ? 'nav.expandSidebar' : 'nav.collapseSidebar')}
                aria-expanded={!collapsed}
                aria-controls="primary-sidebar-navigation"
                onClick={toggleCollapsed}
              >
                <img className="sidebar-collapse-brand" src={oneCrewIcon} alt="" />
                {collapsed ? (
                  <PanelLeftOpen aria-hidden="true" />
                ) : (
                  <PanelLeftClose aria-hidden="true" />
                )}
              </Button>
            </div>
          </div>
          <nav className="sidebar-nav" id="primary-sidebar-navigation">
            <Button
              variant="library-nav"
              navState={active === 'connections' ? 'active' : undefined}
              title={t('nav.account')}
              aria-label={t('nav.account')}
              onClick={() => navigate('/connections')}
            >
              <i aria-hidden="true" data-nav-icon="account" className="sidebar-nav-icon" />
              <span>{t('nav.account')}</span>
            </Button>
            <Button
              variant="library-nav"
              navState={active === 'library' ? 'active' : undefined}
              title={t('nav.library')}
              aria-label={t('nav.library')}
              onClick={() => navigate('/library')}
            >
              <i aria-hidden="true" data-nav-icon="creators" className="sidebar-nav-icon" />
              <span>{t('nav.library')}</span>
            </Button>
            <Button
              variant="library-nav"
              navState="disabled"
              title={t('nav.email')}
              aria-label={t('nav.email')}
              disabled
            >
              <i aria-hidden="true" data-nav-icon="mail" className="sidebar-nav-icon" />
              <span>{t('nav.email')}</span>
              <small>{t('nav.comingSoon')}</small>
            </Button>
          </nav>
          <div className="sidebar-footer">
            <AccountMenu navigate={navigate} />
          </div>
        </aside>
        {children}
      </div>
    </main>
  );
}
