import { AlertTriangle, LoaderCircle } from "lucide-react";
import { useEffect } from "react";
import { AnalyticsNavigation } from "@/lib/analytics-navigation";
import { flushBusinessAnalytics } from "@/lib/business-analytics";
import { Navigate, Route, Routes, useLocation, useNavigate } from "react-router";

import { Button } from "@/components/ui/button";
import { DownloadPage } from "@/features/distribution/download-page";
import { distributionReturnFromSearch, safeDistributionReturnTo } from "@/lib/distribution-return";
import { AccountPage } from "@/features/account/account-page";
import { AuthLayout } from "@/features/auth/auth-layout";
import { ForgotPasswordForm } from "@/features/auth/forgot-password-form";
import { LoginForm } from "@/features/auth/login-form";
import { RegisterForm } from "@/features/auth/register-form";
import { RouteScrollTop } from "@/features/landing/marketing/route-scroll-top";
import { PairingAuthorizationPage } from "@/features/pairing/pairing-authorization-page";
import { PairingRateLimitedPage } from "@/features/pairing/pairing-rate-limited-page";
import { isSafeSessionAuthenticated, type SafeSession } from "@/lib/access-api";
import { useAccessI18n } from "@/lib/access-i18n";
import { getCallbackErrorMessage } from "@/lib/auth-errors";
import { useAuthCapabilities, useSafeSession } from "@/lib/use-access-state";
import { DocumentLanguage } from "@/lib/document-language";
import { safeAccountReturnTo } from "../../src/auth/return-to";

export function AccessRoot() {
  return (
    <>
      <DocumentLanguage />
      <RouteScrollTop />
      <AnalyticsNavigation />
      <AccessApp />
    </>
  );
}

export function AccessApp() {
  const { refresh, state } = useSafeSession();
  const { emailOtpEnabled, googleSignInEnabled } = useAuthCapabilities();
  useEffect(() => {
    if (state.status === "ready" && isSafeSessionAuthenticated(state.session)) void flushBusinessAnalytics();
  }, [state]);

  if (state.status === "loading") return <SessionLoading />;
  if (state.status === "error") {
    return <SessionError onRetry={() => void refresh().catch(() => undefined)} />;
  }

  return (
    <AppRoutes
      googleSignInEnabled={googleSignInEnabled}
      emailOtpEnabled={emailOtpEnabled}
      session={state.session}
      refreshSession={refresh}
    />
  );
}

export function AppRoutes({
  emailOtpEnabled = false,
  googleSignInEnabled,
  session,
  refreshSession,
}: {
  emailOtpEnabled?: boolean;
  googleSignInEnabled: boolean;
  session: SafeSession;
  refreshSession: () => Promise<SafeSession>;
}) {
  const location = useLocation();
  const navigate = useNavigate();
  const { locale } = useAccessI18n();
  const callbackError = getCallbackErrorMessage(location.search, locale);
  const authenticated = isSafeSessionAuthenticated(session);

  const returnTo = distributionReturnFromSearch(location.search);
  const authSearch = returnTo ? `?${new URLSearchParams({ returnTo })}` : "";
  const accountQuery = new URLSearchParams();
  for (const key of ["panel", "apply", "lng"]) {
    const value = new URLSearchParams(location.search).get(key);
    if (value !== null) accountQuery.set(key, value);
  }
  const accountReturn = safeAccountReturnTo(`/account${accountQuery.size ? `?${accountQuery}` : ""}`) ?? "/account";

  async function handleAuthenticated() {
    await refreshSession();
    navigate(returnTo || "/account", { replace: true });
  }

  async function handlePairingAuthenticated() {
    await refreshSession();
    navigate({ pathname: "/pairing/authorize", search: location.search }, { replace: true });
  }



  const accountElement = authenticated ? (
    <AccountPage
      key={session.user.id}
      emailOtpEnabled={emailOtpEnabled}
      user={session.user}
      refreshSession={refreshSession}
    />
  ) : (
    <Navigate to={{ pathname: "/login", search: new URLSearchParams({ ...Object.fromEntries(new URLSearchParams(location.search)), returnTo: accountReturn }).toString() }} replace />
  );

  const loginElement = authenticated ? (
    <Navigate to={returnTo || "/account"} replace />
  ) : (
    <AuthLayout>
      <LoginForm
        callbackError={callbackError}
        googleSignInEnabled={googleSignInEnabled}
        emailOtpEnabled={emailOtpEnabled}
        onAuthenticated={handleAuthenticated}
        registerTo={`/register${authSearch}`}
        returnTo={returnTo ?? undefined}
      />
    </AuthLayout>
  );

  const registerElement = authenticated ? (
    <Navigate to={returnTo || "/account"} replace />
  ) : (
    <AuthLayout>
      <RegisterForm
        callbackError={callbackError}
        emailOtpEnabled={emailOtpEnabled}
        googleSignInEnabled={googleSignInEnabled}
        onAuthenticated={handleAuthenticated}
        loginTo={`/login${authSearch}`}
        returnTo={returnTo ?? undefined}
      />
    </AuthLayout>
  );

  const forgotPasswordElement = authenticated ? (
    <Navigate to="/account" replace />
  ) : (
    <AuthLayout>
      <ForgotPasswordForm emailOtpEnabled={emailOtpEnabled} />
    </AuthLayout>
  );

  const pairingElement = authenticated ? (
    <PairingAuthorizationPage user={session.user} />
  ) : (
    <AuthLayout>
      {location.hash === "#register" ? (
        <RegisterForm
          googleSignInEnabled={false}
          emailOtpEnabled={emailOtpEnabled}
          loginTo={`${location.pathname}${location.search}#login`}
          onAuthenticated={handlePairingAuthenticated}
        />
      ) : (
        <LoginForm
          googleSignInEnabled={false}
          emailOtpEnabled={emailOtpEnabled}
          onAuthenticated={handlePairingAuthenticated}
          registerTo={`${location.pathname}${location.search}#register`}
        />
      )}
    </AuthLayout>
  );

  const pairingRateLimitedElement = authenticated ? (
    <PairingRateLimitedPage />
  ) : (
    <Navigate to="/login" replace />
  );

  return (
    <Routes>
      <Route path="/" element={<Navigate to="/login" replace />} />
      <Route path="/login" element={loginElement} />
      <Route path="/register" element={registerElement} />
      <Route path="/forgot-password" element={forgotPasswordElement} />
      <Route path="/account" element={accountElement} />
      <Route path="/d/:token" element={!safeDistributionReturnTo(location.pathname) || authenticated ? <DownloadPage user={authenticated ? session.user : undefined} refreshSession={refreshSession} /> : <Navigate to={`/login?${new URLSearchParams({ ...(safeDistributionReturnTo(location.pathname) ? { returnTo: location.pathname } : {}) })}`} replace />} />
      <Route path="/pairing/authorize" element={pairingElement} />
      <Route path="/pairing/rate-limited" element={pairingRateLimitedElement} />
      <Route path="*" element={<Navigate to={authenticated ? "/account" : "/login"} replace />} />
    </Routes>
  );
}

function SessionLoading() {
  const { t } = useAccessI18n();
  return (
    <main className="flex min-h-svh items-center justify-center bg-background" aria-busy="true">
      <div className="flex items-center gap-3 text-sm text-muted-foreground">
        <LoaderCircle aria-hidden="true" className="size-5 animate-spin" />
        {t("layout.sessionLoading")}
      </div>
    </main>
  );
}

function SessionError({ onRetry }: { onRetry: () => void }) {
  const { t } = useAccessI18n();
  return (
    <main className="flex min-h-svh items-center justify-center bg-muted/40 p-6">
      <section className="w-full max-w-sm rounded-xl border border-border bg-background p-6 text-center shadow-sm" aria-labelledby="session-error-title">
        <AlertTriangle aria-hidden="true" className="mx-auto size-8 text-destructive" />
        <h1 id="session-error-title" className="mt-4 text-lg font-semibold">{t("layout.sessionErrorTitle")}</h1>
        <p className="mt-2 text-sm leading-6 text-muted-foreground">{t("layout.sessionErrorDescription")}</p>
        <Button className="mt-6 w-full" onClick={onRetry}>{t("layout.retry")}</Button>
      </section>
    </main>
  );
}
