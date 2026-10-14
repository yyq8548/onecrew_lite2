import { ArrowDownToLine, ArrowLeft, LoaderCircle, ShieldCheck } from "lucide-react";
import { useEffect, useState } from "react";
import { Link, useNavigate, useParams } from "react-router";
import { Button } from "@/components/ui/button";
import { Alert, AlertDescription } from "@/components/ui/alert";
import { AccessLanguageToggle } from "@/features/auth/access-language-toggle";
import type { SafeSessionUser, SafeSession } from "@/lib/access-api";
import { useAccessI18n } from "@/lib/access-i18n";
import { DistributionApiError, getDistributionPreview, type DistributionPreview } from "@/lib/distribution-api";
import { signOutCurrentSession } from "@/lib/auth-actions";

export function DownloadPage({ user, refreshSession }: { user?: SafeSessionUser; refreshSession: () => Promise<SafeSession> }) {
  const { token = "" } = useParams(); const { t, locale } = useAccessI18n(); const navigate = useNavigate();
  const [preview, setPreview] = useState<DistributionPreview | null>(null);
  const [actionError, setActionError] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null); const [attempt, setAttempt] = useState(0); const [pending, setPending] = useState(false);
  const valid = /^[A-Za-z0-9_-]{43}$/.test(token);
  useEffect(() => {
    if (!valid || !user) return; let active = true; setPreview(null); setError(null);
    void getDistributionPreview(token).then(value => { if (active) setPreview(value); }).catch(cause => {
      if (!active) return;
      const code = cause instanceof DistributionApiError ? cause.code : undefined;
      setError(code === "link_expired" ? "expired" : code === "entitlement_revoked" ? "revoked" : code === "release_unavailable" ? "noRelease" : code === "authentication_required" ? "sessionExpired" : code === "email_verification_required" ? "verification" : code === "link_not_found" ? "unavailable" : "requestError");
    });
    return () => { active = false; };
  }, [token, valid, attempt, user?.id]);
  const state = valid ? error : "invalid";
  async function switchAccount() {
    setPending(true); setActionError(null);
    const result = await signOutCurrentSession(locale);
    if (!result.ok) { setActionError(result.message); setPending(false); return; }
    try { await refreshSession(); navigate(`/login?${new URLSearchParams({ returnTo: `/d/${token}` })}`, { replace: true }); }
    catch { setActionError(t("distribution.error")); setPending(false); }
  }
  const date = (value: number) => new Date(value).toLocaleString(locale === "en" ? "en-US" : "zh-CN");
  return <main className="onecrew-theme account-shell download-shell">
    <header className="account-topbar"><div className="account-topbar-inner"><Link className="account-logo" to="/account"><img src="/onecrew-logo-dark.svg" alt="OneCrew" /></Link><AccessLanguageToggle inverse /></div></header>
    <div className="download-main"><Link className="download-back" to="/account"><ArrowLeft aria-hidden />{t("distribution.account")}</Link>
      <section className="download-card"><span className="distribution-eyebrow">ONECREW DESKTOP</span><h1>{t("distribution.downloadTitle")}</h1><p>{t("distribution.downloadIntro")}</p>{user && <p className="download-identity"><ShieldCheck aria-hidden />{t("distribution.signedIn", { email: user.email })}</p>}
        {actionError && <Alert variant="destructive"><AlertDescription>{actionError}</AlertDescription></Alert>}
        {!state && !preview && <p role="status"><LoaderCircle className="inline size-4 animate-spin" aria-hidden /> {t("distribution.loadingDownload")}</p>}
        {state && <div className="download-error" role="status"><h2>{t(`distribution.${state === "revoked" ? "revokedTitle" : state}`)}</h2>{state !== "sessionExpired" && state !== "verification" && <p>{t(`distribution.${state === "revoked" ? "revokedDetail" : `${state}Detail`}`)}</p>}<div className="distribution-actions"><Button asChild><Link to="/account">{t("distribution.account")}</Link></Button>{state === "unavailable" || state === "sessionExpired" ? <Button variant="outline" onClick={() => void switchAccount()} disabled={pending}>{t(`distribution.${pending ? "switching" : "switchAccount"}`)}</Button> : <Button variant="outline" onClick={() => setAttempt(value => value + 1)}>{t("distribution.retry")}</Button>}</div></div>}
        {preview && <><div className="download-release"><h2>{t("distribution.release", { version: preview.release.version })}</h2><p>{t("distribution.expires", { date: date(preview.expiresAt) })}</p></div><div className="download-assets">{preview.assets.filter(asset => asset.kind === "installer").map(asset => <div className="download-asset" key={asset.id}><div><strong>{asset.platform === "darwin" ? "macOS" : asset.platform === "windows" ? "Windows" : "Linux"} · {asset.arch === "aarch64" ? "ARM64" : "x64"}</strong><span>{asset.filename} · {(asset.size / 1024 / 1024).toFixed(1)} MB</span></div><Button asChild><a href={asset.downloadUrl} referrerPolicy="no-referrer"><ArrowDownToLine aria-hidden />{t("distribution.download")}</a></Button></div>)}</div>{!preview.assets.some(asset => asset.kind === "installer") && <Alert><AlertDescription>{t("distribution.noAssets")}</AlertDescription></Alert>}{preview.release.notes && <div className="download-notes"><h3>{t("distribution.releaseNotes")}</h3><p>{preview.release.notes}</p></div>}</>}
      </section>
    </div>
  </main>;
}
