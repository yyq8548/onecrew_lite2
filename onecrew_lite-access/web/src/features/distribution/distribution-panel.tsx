import { ArrowUpRight, Check, Clock3, LoaderCircle, RefreshCw, Send, ShieldCheck } from "lucide-react";
import { CliInstallButton } from "./cli-install-button";
import { DesktopDownloadButton } from "./desktop-download-button";
import { useEffect, useRef, useState, type FormEvent } from "react";
import { Link, useLocation } from "react-router";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Alert, AlertDescription } from "@/components/ui/alert";
import type { SafeSessionUser } from "@/lib/access-api";
import { PUBLIC_SITE_URL } from "@/lib/public-site";
import { flushBusinessAnalytics } from "@/lib/business-analytics";
import { useAccessI18n } from "@/lib/access-i18n";
import { DistributionApiError, getDistributionApplication, submitDistributionApplication, type DistributionApplication } from "@/lib/distribution-api";

export function distributionState(data: DistributionApplication) {
  if (data.entitlement?.status === "revoked") return "revoked";
  if (data.entitlement?.status === "active") return "approved";
  return data.application?.status ?? "none";
}
type ApplicationAttempt = {
  userId: string;
  applicationId: string | null;
  revision: number | null;
  payload: string;
  key: string;
};

function matchesApplicationContext(attempt: ApplicationAttempt, userId: string, data: DistributionApplication | null) {
  return attempt.userId === userId
    && attempt.applicationId === (data?.application?.id ?? null)
    && attempt.revision === (data?.application?.revision ?? null);
}

export function DistributionPanel({ user, onProjection }: { user: SafeSessionUser; onProjection?: (data: DistributionApplication) => void }) {
  const { t, locale } = useAccessI18n();
  const location = useLocation();
  const [data, setData] = useState<DistributionApplication | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [authRequired, setAuthRequired] = useState(false);
  const [formOpen, setFormOpen] = useState(new URLSearchParams(location.search).get("apply") === "1");
  const [pending, setPending] = useState(false);
  const [draft, setDraft] = useState<{ displayName: string; company: string; purpose: string } | null>(null);
  function updateDraft(field: "displayName" | "company" | "purpose", value: string) {
    setDraft(current => ({ displayName: data?.application?.displayName || user.name || "", company: data?.application?.company || "", purpose: data?.application?.purpose || "", ...current, [field]: value }));
  }
  const attempt = useRef<ApplicationAttempt | null>(null);
  useEffect(() => { if (data) onProjection?.(data); }, [data, onProjection]);
  function report(cause: unknown, fallback: string) {
    const code = cause instanceof DistributionApiError ? cause.code : undefined;
    if (code === "applications_closed") setData(value => value ? { ...value, applicationsOpen: false } : value);
    setAuthRequired(cause instanceof DistributionApiError && cause.status === 401);
    setError(code === "applications_closed" ? "applicationsClosed" : code === "email_verification_required" ? "verification" : code === "application_exists" ? "conflict" : code === "authentication_required" ? "sessionExpired" : code === "release_unavailable" ? "noReleaseDetail" : code === "entitlement_revoked" ? "revokedDetail" : fallback);
  }
  function acceptProjection(value: DistributionApplication) {
    // A readback at the same rejected revision can still be an uncertain retry.
    // A new revision, a different account/application, or a resolved submission
    // ends that intent so later reapplications cannot reuse its operation key.
    const submitted = value.application?.status === "pending" || value.application?.status === "approved";
    if (attempt.current && (!matchesApplicationContext(attempt.current, user.id, value) || submitted)) {
      attempt.current = null;
    }
    if (submitted) { setFormOpen(false); setDraft(null); }
    setData(value);
  }
  async function refresh() {
    setLoading(true); setError(null); setAuthRequired(false);
    try { acceptProjection(await getDistributionApplication()); } catch (cause) { report(cause, "error"); }
    finally { setLoading(false); }
  }
  useEffect(() => { let active = true; if (attempt.current?.userId !== user.id) attempt.current = null; setData(null); setDraft(null); setLoading(true); void getDistributionApplication().then(value => { if (active) acceptProjection(value); }).catch(cause => { if (active) report(cause, "error"); }).finally(() => { if (active) setLoading(false); }); return () => { active = false; }; }, [user.id]);
  async function submit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (!data?.applicationsOpen) { setError("applicationsClosed"); return; }
    const form = new FormData(event.currentTarget);
    const input = { displayName: String(form.get("displayName")).trim(), purpose: String(form.get("purpose")).trim(), company: String(form.get("company")).trim() };
    if (!input.displayName || !input.purpose) { setError("requiredFields"); return; }
    const payload = JSON.stringify(input);
    if (!attempt.current || attempt.current.payload !== payload || !matchesApplicationContext(attempt.current, user.id, data)) {
      attempt.current = {
        userId: user.id,
        applicationId: data?.application?.id ?? null,
        revision: data?.application?.revision ?? null,
        payload,
        key: crypto.randomUUID(),
      };
    }
    setPending(true); setError(null);
    try {
      await submitDistributionApplication(input, attempt.current.key);
      void flushBusinessAnalytics();
      // Read back persisted state before presenting an application as received.
      acceptProjection(await getDistributionApplication()); setFormOpen(false); attempt.current = null;
    } catch (cause) { report(cause, "submitError"); }
    finally { setPending(false); }
  }
  const state = data ? distributionState(data) : "none";
  const applicationsClosed = !!data && (!data.applicationsOpen || error === "applicationsClosed");
  const canApply = state === "none" || state === "rejected";
  const showForm = data && !applicationsClosed && formOpen && (state === "none" || state === "rejected");
  return <section className="distribution-card" id="early-access" aria-labelledby="distribution-title" aria-busy={loading || pending}>
    <div className="distribution-main">
      <div className="distribution-eyebrow">{t("distribution.eyebrow")}</div>
      <div className="distribution-heading"><h2 id="distribution-title">{t("distribution.title")}</h2>{data && <Badge className={`distribution-status is-${state}`} variant="secondary">{state === "pending" ? <Clock3 aria-hidden /> : state === "approved" ? <Check aria-hidden /> : null}{t(`distribution.state.${applicationsClosed && state === "none" ? "closed" : state}`)}</Badge>}</div>
      <p className="distribution-intro">{t("distribution.intro")}</p>
      {loading && !data ? <p role="status"><LoaderCircle className="inline size-4 animate-spin" aria-hidden /> {t("distribution.loading")}</p> : data && <p className="distribution-state-description" role="status">{t(`distribution.description.${state === "approved" && !data.latestRelease ? "approvedPreparing" : state}`)}</p>}
      {canApply && applicationsClosed && <p className="distribution-closed" role="status">{t("distribution.applicationsClosed")}</p>}
      {state === "rejected" && data?.application?.reviewReason && <div className="distribution-review-reason" aria-labelledby="application-review-reason"><h3 id="application-review-reason">{t("distribution.reviewReason")}</h3><p>{data.application.reviewReason}</p></div>}
      {error && <Alert variant="destructive"><AlertDescription>{t(`distribution.${error}`)}{authRequired && <Link to="/login?returnTo=%2Faccount%3Fapply%3D1">{t("distribution.signIn")}</Link>}</AlertDescription></Alert>}
      {data && !applicationsClosed && <ol className="distribution-steps" aria-label={t("distribution.stepsLabel")}>
        {[t("distribution.apply"), t("distribution.reviewStep"), locale === "en" ? "Install OneCrew" : "安装 OneCrew"].map((step, index) => <li key={step} aria-current={(state === "none" || state === "rejected" ? 0 : state === "pending" ? 1 : state === "approved" ? 2 : -1) === index ? "step" : undefined}><span>{index + 1}</span>{step}</li>)}
      </ol>}
      {showForm ? <form className="distribution-form" onSubmit={submit}>
        <div className="distribution-field"><Label htmlFor="application-email">{t("distribution.email")}</Label><Input id="application-email" type="email" value={data.email} readOnly aria-describedby="application-email-hint" /><p id="application-email-hint">{t("distribution.emailHint")}</p></div>
        <div className="distribution-form-row"><div className="distribution-field"><Label htmlFor="application-name">{t("distribution.displayName")}</Label><Input id="application-name" name="displayName" value={draft?.displayName ?? data.application?.displayName ?? user.name ?? ""} onChange={event => updateDraft("displayName", event.target.value)} required maxLength={100} autoComplete="name" disabled={pending} /></div>
        <div className="distribution-field"><Label htmlFor="application-company">{t("distribution.company")} <span>{t("distribution.optional")}</span></Label><Input id="application-company" name="company" value={draft?.company ?? data.application?.company ?? ""} onChange={event => updateDraft("company", event.target.value)} maxLength={200} autoComplete="organization" disabled={pending} /></div></div>
        <div className="distribution-field"><Label htmlFor="application-purpose">{t("distribution.purpose")}</Label><textarea id="application-purpose" name="purpose" value={draft?.purpose ?? data.application?.purpose ?? ""} onChange={event => updateDraft("purpose", event.target.value)} required maxLength={2000} rows={3} placeholder={t("distribution.purposePlaceholder")} disabled={pending} /></div>
        <p className="distribution-privacy"><ShieldCheck aria-hidden />{t("distribution.privacy")}</p>
        <div className="distribution-actions"><Button disabled={pending || loading || applicationsClosed} type="submit">{pending ? <LoaderCircle className="animate-spin" aria-hidden /> : <Send aria-hidden />}{t(`distribution.${applicationsClosed ? "applicationsClosedAction" : pending ? "submitting" : "submit"}`)}</Button><Button type="button" variant="ghost" disabled={pending} onClick={() => setFormOpen(false)}>{t("distribution.cancel")}</Button><Button type="button" variant="outline" onClick={() => void refresh()} disabled={loading || pending}><RefreshCw aria-hidden />{t("distribution.refresh")}</Button></div>
      </form> : <div className="distribution-actions">
        {data && (state === "none" || state === "rejected") && <Button onClick={() => setFormOpen(true)} disabled={applicationsClosed}>{t(`distribution.${applicationsClosed ? "applicationsClosedAction" : state === "none" ? "apply" : "reapply"}`)}<ArrowUpRight aria-hidden /></Button>}
        {applicationsClosed && <Button asChild variant="outline"><a href={`${PUBLIC_SITE_URL}/${locale}/resources/user-guide`}>{locale === "en" ? "Read the user guide" : "查看使用指南"}</a></Button>}
        <Button variant="outline" onClick={() => void refresh()} disabled={loading || pending}><RefreshCw aria-hidden />{t(`distribution.${data ? "refresh" : "retry"}`)}</Button>
      </div>}
      {state === "approved" && data && <>
        {data.latestRelease ? <DesktopDownloadButton key={`${user.id}:${data.entitlement?.revision ?? 0}`} /> : <p role="status">{t("distribution.packagePreparing")}</p>}
        <CliInstallButton key={`cli:${user.id}:${data.entitlement?.revision ?? 0}`} release={data.cliRelease} />
      </>}
    </div>
  </section>;
}
