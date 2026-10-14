import { ArrowLeft, LoaderCircle, Mail, MapPin, Network, ShieldCheck } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { useLocation, useNavigate } from "react-router";

import { BrandMark } from "@/components/brand-mark";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import {
  Card,
  CardContent,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import {
  AccessApiError,
  getDesktopPairingPreview,
  type PairingPreview,
  type SafeSessionUser,
} from "@/lib/access-api";

type PageState =
  | { kind: "error"; message: string }
  | { kind: "loading" }
  | { kind: "ready"; preview: PairingPreview };

export function PairingAuthorizationPage({ user }: { user: SafeSessionUser }) {
  const location = useLocation();
  const navigate = useNavigate();
  const mounted = useRef(false);
  const requestEpoch = useRef(0);
  const [state, setState] = useState<PageState>({ kind: "loading" });
  const [submitting, setSubmitting] = useState(false);

  useEffect(() => {
    mounted.current = true;
    const epoch = ++requestEpoch.current;
    const controller = new AbortController();
    setState({ kind: "loading" });
    void getDesktopPairingPreview(location.search, { signal: controller.signal })
      .then((preview) => {
        if (mounted.current && epoch === requestEpoch.current) {
          setState({ kind: "ready", preview });
        }
      })
      .catch((error: unknown) => {
        if (mounted.current && epoch === requestEpoch.current && !controller.signal.aborted) {
          setState({ kind: "error", message: pairingErrorMessage(error) });
        }
      });
    return () => {
      mounted.current = false;
      requestEpoch.current += 1;
      controller.abort();
    };
  }, [location.search]);

  return (
    <main className="onecrew-theme pairing-page">
      <header className="pairing-header">
        <div className="pairing-header-inner">
          <BrandMark />
          <Button className="pairing-back-button" onClick={() => navigate("/account", { replace: true })} size="sm" variant="ghost">
            <ArrowLeft aria-hidden="true" />
            返回账号中心
          </Button>
        </div>
      </header>
      <section className="pairing-content">
        <Card className="pairing-card">
          <CardHeader className="pairing-card-header">
            <CardTitle asChild><h1>确认连接 OneCrew Desktop</h1></CardTitle>
            <div className="pairing-card-description">
              <span className="pairing-identity">
                <Badge className="gap-1 [&_svg]:size-3" variant="secondary"><Mail aria-hidden="true" />{user.email}</Badge>
                {state.kind === "ready" ? (
                  <Badge className="gap-1 [&_svg]:size-3" variant="outline"><Network aria-hidden="true" />绑定 IP · {state.preview.request.bindingIp ?? "—"}</Badge>
                ) : null}
                {state.kind === "ready" && locationText(state.preview.request.bindingLocation) !== "—" ? (
                  <Badge className="gap-1 [&_svg]:size-3" variant="outline"><MapPin aria-hidden="true" />{locationText(state.preview.request.bindingLocation)}</Badge>
                ) : null}
              </span>
            </div>
          </CardHeader>
          <CardContent className="pairing-card-content">
            {state.kind === "loading" ? (
              <div aria-busy="true" className="flex items-center gap-2 py-8 text-sm text-muted-foreground">
                <LoaderCircle aria-hidden="true" className="animate-spin" />
                正在验证绑定请求…
              </div>
            ) : null}

            {state.kind === "error" ? (
              <Alert variant="destructive">
                <AlertTitle>无法继续绑定</AlertTitle>
                <AlertDescription>{state.message}</AlertDescription>
              </Alert>
            ) : null}

            {state.kind === "ready" ? (
              <>
                <Alert className="pairing-security-alert">
                  <AlertTitle className="flex items-center gap-2">
                    <ShieldCheck aria-hidden="true" />
                    请确认这是你刚刚从 OneCrew Desktop 发起的请求
                  </AlertTitle>
                </Alert>
                <dl className="pairing-facts">
                  <PairingDetail label="应用" value={pairingAppDisplayName(state.preview.request.appName)} />
                  <PairingDetail label="设备" value={state.preview.request.label} />
                  <PairingDetail label="平台" value={state.preview.request.platformLabel} />
                  <PairingDetail label="版本" value={state.preview.request.appVersion} />
                  <PairingDetail label="本机端口" value={state.preview.request.callbackPort} />
                  <PairingDetail label="绑定 IP" value={state.preview.request.bindingIp ?? "—"} />
                  <PairingDetail label="绑定地区" value={locationText(state.preview.request.bindingLocation)} />
                </dl>
                <form
                  action="/api/pairing/authorize"
                  className="pairing-actions"
                  method="post"
                  onSubmit={() => setSubmitting(true)}
                >
                  <input
                    name="approvalToken"
                    type="hidden"
                    value={state.preview.approvalToken}
                  />
                  <Button className="pairing-cancel-action w-full sm:w-auto" onClick={() => navigate("/account", { replace: true })} type="button" variant="outline">
                    取消
                  </Button>
                  <Button
                    aria-busy={submitting}
                    className="pairing-primary-action w-full sm:w-auto"
                    disabled={submitting}
                    size="lg"
                    type="submit"
                  >
                    {submitting ? <LoaderCircle aria-hidden="true" className="animate-spin" /> : <ShieldCheck aria-hidden="true" />}
                    {submitting ? "正在返回 OneCrew Desktop…" : "确认绑定并返回 OneCrew Desktop"}
                  </Button>
                </form>
              </>
            ) : null}
          </CardContent>
        </Card>
      </section>
    </main>
  );
}

function PairingDetail({ label, value }: { label: string; value: string }) {
  return (
    <div className="grid min-w-0 gap-1 sm:grid-cols-[7rem_minmax(0,1fr)]">
      <dt className="font-medium text-muted-foreground">{label}</dt>
      <dd className="min-w-0 [overflow-wrap:anywhere]">{value}</dd>
    </div>
  );
}

function pairingErrorMessage(error: unknown) {
  if (error instanceof AccessApiError) {
    if (error.code === "authentication_required") return "登录状态已失效，请重新登录后从 OneCrew Desktop 再次发起。";
    if (error.code === "pairing_approval_resolved") return "这个请求已经处理过，请回到 OneCrew Desktop 重新发起。";
    if (error.code === "rate_limited") return "绑定尝试过于频繁，请稍后从 OneCrew Desktop 重新发起。";
  }
  return "绑定链接无效或已过期，请回到 OneCrew Desktop 重新发起。";
}

function pairingAppDisplayName(appName: string) {
  return appName === "Agent Platform" ? "OneCrew Desktop" : appName;
}

function locationText(location: PairingPreview["request"]["bindingLocation"]) {
  const parts = [location.city, location.region, location.country].filter(Boolean);
  return parts.join(" · ") || "—";
}
