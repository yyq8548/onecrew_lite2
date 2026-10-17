import { TooltipProvider } from '@/components/ui/tooltip';
import { query, useRoute } from '@/lib/router';
import { ConnectionsPage } from '@/components/ConnectionsPage';
import { AppShell } from '@/layout/AppShell';
import { LibraryPage } from '@/features/tables/LibraryPage';
import { TableDetailPage } from '@/features/tables/TableDetailPage';
import { AccessProvider, useAccess } from '@/features/access/AccessProvider';
import { LoginPage } from '@/features/access/LoginPage';
import { AccessCompletePage } from '@/features/access/AccessCompletePage';
import { useEffect } from 'react';

type SignedInRoute = Extract<
  ReturnType<typeof useRoute>['route'],
  { kind: 'detail' | 'connections' | 'library' }
>;

export function App() {
  return (
    <AccessProvider>
      <AuthedApp />
    </AccessProvider>
  );
}

function AuthedApp() {
  const { route, navigate } = useRoute();
  const { status } = useAccess();
  useEffect(() => {
    if (!status) return;
    if (status.state !== 'signed_in' && status.state !== 'unavailable' && route.kind !== 'login' && route.kind !== 'auth-complete') {
      const returnTo =
        route.kind === 'connections'
          ? '/connections'
          : route.kind === 'detail'
            ? `/library/${encodeURIComponent(route.tableId)}${query(route.search)}`
            : `/library${query(route.search)}`;
      navigate(`/login?returnTo=${encodeURIComponent(returnTo)}`, true);
    }
  }, [navigate, route, status]);
  if (route.kind === 'auth-complete') return <AccessCompletePage navigate={navigate} />;
  if (!status) return <div className="app-loading" />;
  const returnTo = route.kind === 'login' ? route.returnTo : '/library';
  if (status.state !== 'signed_in') return <LoginPage navigate={navigate} returnTo={returnTo} />;
  if (route.kind === 'login') return <LoginPage navigate={navigate} returnTo={returnTo} />;
  const accountId = status.user?.id;
  if (!accountId) return <div className="app-loading" />;
  return <SignedInApp key={accountId} route={route} navigate={navigate} />;
}

function SignedInApp({
  route,
  navigate,
}: {
  route: SignedInRoute;
  navigate: ReturnType<typeof useRoute>['navigate'];
}) {
  return (
    <TooltipProvider>
      {route.kind === 'detail' ? (
        <TableDetailPage tableId={route.tableId} listSearch={route.search} navigate={navigate} />
      ) : route.kind === 'connections' ? (
        <AppShell navigate={navigate} active="connections">
          <ConnectionsPage />
        </AppShell>
      ) : (
        <LibraryPage initialSearch={route.search} navigate={navigate} />
      )}
    </TooltipProvider>
  );
}
