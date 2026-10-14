import { useRef, useState } from "react";
import { useNavigate } from "react-router";
import { ArrowDownToLine, LoaderCircle } from "lucide-react";

import { Button } from "@/components/ui/button";
import { useAccessI18n } from "@/lib/access-i18n";
import { createDistributionLink } from "@/lib/distribution-api";

export function DesktopDownloadButton() {
  const { t } = useAccessI18n();
  const navigate = useNavigate();
  const attempt = useRef<{ requestId: string; expiresAt?: number } | null>(null);
  const [pending, setPending] = useState(false);
  const [error, setError] = useState(false);

  async function openDownload() {
    if (pending) return;
    if (!attempt.current || (attempt.current.expiresAt && attempt.current.expiresAt <= Date.now())) {
      attempt.current = { requestId: crypto.randomUUID() };
    }
    setPending(true);
    setError(false);
    try {
      const link = await createDistributionLink(attempt.current.requestId);
      attempt.current.expiresAt = link.expiresAt;
      navigate(link.url);
    } catch {
      // Keep the same idempotency key: the server may have created the link.
      setError(true);
    } finally {
      setPending(false);
    }
  }

  return <div className="distribution-cli">
    <Button disabled={pending} onClick={() => void openDownload()}>
      {pending ? <LoaderCircle className="animate-spin" aria-hidden /> : <ArrowDownToLine aria-hidden />}
      {t(`distribution.${pending ? "generating" : "openDownload"}`)}
    </Button>
    {error && <p role="alert">{t("distribution.fetchError")}</p>}
  </div>;
}
