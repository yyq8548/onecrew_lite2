import { ArrowRight, Download, Home, LayoutGrid, LoaderCircle, LogOut, Mail, ShieldCheck, Smartphone, UserRound } from "lucide-react";
import { useState } from "react";
import { useNavigate, useSearchParams } from "react-router";
import { Alert, AlertDescription } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import { AccessLanguageToggle } from "@/features/auth/access-language-toggle";
import { DistributionPanel, distributionState } from "@/features/distribution/distribution-panel";
import type { DistributionApplication } from "@/lib/distribution-api";
import type { SafeSession, SafeSessionUser } from "@/lib/access-api";
import { useAccessI18n } from "@/lib/access-i18n";
import { PUBLIC_SITE_URL } from "@/lib/public-site";
import { signOutCurrentSession } from "@/lib/auth-actions";
import { AccountSettingsPanel } from "./account-settings";
import styles from "./AccountPage.module.css";

const panels = ["overview", "access", "profile", "security", "email", "devices"] as const;
type Panel = typeof panels[number];
const icons = { overview: LayoutGrid, access: Download, profile: UserRound, security: ShieldCheck, email: Mail, devices: Smartphone };

export function AccountPage({ user, refreshSession, emailOtpEnabled = false }: {
  user: SafeSessionUser; refreshSession: () => Promise<SafeSession>; emailOtpEnabled?: boolean;
}) {
  const { locale, t } = useAccessI18n();
  const [params, setParams] = useSearchParams();
  const navigate = useNavigate();
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [application, setApplication] = useState<DistributionApplication | null>(null);
  const requested = params.get("apply") === "1" ? "access" : params.get("panel");
  const active: Panel = panels.includes(requested as Panel) ? requested as Panel : "overview";
  const labels: Record<Panel, string> = {
    overview: t("account.overviewNav"), access: t("account.accessNav"),
    profile: t("account.profileNav"), security: t("account.securityNav"),
    email: t("account.emailNav"), devices: t("account.devicesNav"),
  };
  function select(panel: Panel) {
    const next = new URLSearchParams(params);
    next.delete("apply"); next.set("panel", panel);
    setParams(next, { replace: true });
  }
  async function signOut() {
    setError(null); setPending(true);
    const result = await signOutCurrentSession(locale);
    if (!result.ok) { setError(result.message); setPending(false); return; }
    try { await refreshSession(); navigate("/login", { replace: true }); }
    catch { setPending(false); }
  }
  const home = `${PUBLIC_SITE_URL}/${locale}`;
  const settings = active !== "overview" && active !== "access" ? active : null;
  const values = { profile: user.name || t("account.defaultUser"), email: user.email, security: t("account.securityTitle"), devices: t("account.devicesTitle") };
  return <main className={`onecrew-theme ${styles.page}`}>
    <aside className={styles.sidebar}>
      <a className={styles.logo} href={home} aria-label={t("layout.homeAria")}><img src="/onecrew-logo-dark.svg" alt="OneCrew" width="740" height="161" /></a>
      <nav className={styles.sideNav} aria-label={t("account.settingsTitle")}>
        {panels.map(panel => { const Icon = icons[panel]; return <button key={panel} type="button" aria-current={active === panel ? "page" : undefined} onClick={() => select(panel)}><Icon aria-hidden /><span>{labels[panel]}</span></button>; })}
      </nav>
      <a className={styles.homeLink} href={home}><Home aria-hidden /><span>{t("account.backHome")}</span></a>
    </aside>
    <section className={styles.mainArea}>
      <header className={styles.topbar}>
        <div><span>{t("account.center")}</span><b>/</b><strong>{labels[active]}</strong></div>
        <nav aria-label={t("account.center")}><AccessLanguageToggle /><Button variant="outline" onClick={() => void signOut()} disabled={pending}>{pending ? <LoaderCircle className="animate-spin" aria-hidden /> : <LogOut aria-hidden />}{t("account.signOut")}</Button></nav>
      </header>
      <div className={styles.content}>
        {error && <Alert variant="destructive"><AlertDescription>{error}</AlertDescription></Alert>}
        {active === "overview" ? <section className={styles.accountHero} aria-labelledby="account-heading">
          <span className={styles.accountAvatar} aria-hidden><img src="/onecrew-watermark-mark.svg" alt="" /></span>
          <div className={styles.accountIdentity}><h1 id="account-heading">{t("account.greeting", { name: user.name || t("account.defaultUser") })}</h1><strong>{user.name}</strong><span>{user.email}</span></div>
          <button type="button" onClick={() => select("profile")}>{t("account.viewProfile")}<ArrowRight aria-hidden /></button>
        </section> : <header className={styles.pageIntro}><p>{t("account.overviewNav")}</p><h1>{labels[active]}</h1></header>}
        {active === "overview" && <>
          <section className={styles.accessSummary} aria-labelledby="access-summary-heading"><div><h2 id="access-summary-heading">{t("account.accessNav")}</h2><p role="status">{application ? t(`distribution.${!application.applicationsOpen && !application.application && !application.entitlement ? "applicationsClosed" : `description.${distributionState(application) === "approved" && !application.latestRelease ? "approvedPreparing" : distributionState(application)}`}`) : t("account.accessSummaryHint")}</p></div><Button onClick={() => select("access")}>{t("account.openPanel")}<ArrowRight aria-hidden /></Button></section>
          <section className={styles.snapshot} aria-labelledby="snapshot-heading"><header><div><h2 id="snapshot-heading">{t("account.snapshot")}</h2><p>{t("account.snapshotHint")}</p></div></header><div className={styles.cardGrid}>
            {(["profile", "email", "security", "devices"] as const).map(panel => { const Icon = icons[panel]; return <button className={styles.summaryCard} type="button" key={panel} onClick={() => select(panel)}><span className={styles.cardIcon}><Icon aria-hidden /></span><span className={styles.cardText}><small>{labels[panel]}</small><strong>{values[panel]}</strong><span>{t(`account.${panel}Description`)}</span></span><span className={styles.cardArrow}><ArrowRight aria-hidden /></span></button>; })}
          </div></section>
        </>}
        {/* Keep mounted so switching panels/languages does not discard draft or operation keys. */}
        <div hidden={active !== "access"}><DistributionPanel user={user} onProjection={setApplication} /></div>
        {settings && <AccountSettingsPanel section={settings} emailOtpEnabled={emailOtpEnabled} refreshSession={refreshSession} user={user} />}
      </div>
    </section>
  </main>;
}
