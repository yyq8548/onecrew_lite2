import { ResultStatus } from "@mcp-ui/components/ui/result-status";
import { useAppLocale } from "@mcp-ui/lib/i18n";
import type { ResultDmFailure } from "@mcp-ui/lib/result-contract";

/** Native evidence is display data; action and result authority stay in the snapshot. */
export function DmFailureNotice({ failure }: { failure: ResultDmFailure }) {
  const { t } = useAppLocale();
  const { diagnostic } = failure;
  const facts = [
    [t("dmDiagnosticStage"), failure.stage],
    [t("dmDiagnosticPlatform"), diagnostic.platform],
    [t("dmDiagnosticProtocol"), diagnostic.protocol],
    [t("dmDiagnosticOrigin"), t(diagnostic.origin === "provider" ? "dmDiagnosticProvider" : "dmDiagnosticLocal")],
    [t("dmDiagnosticCode"), diagnostic.code ?? t("dmDiagnosticNotProvided")],
    ...(diagnostic.http_status != null ? [[t("dmDiagnosticHttpStatus"), String(diagnostic.http_status)]] : []),
    ...(diagnostic.rpc_code != null ? [[t("dmDiagnosticRpcCode"), String(diagnostic.rpc_code)]] : []),
  ];
  return <div role="alert" className="mb-3 grid min-w-0 gap-3 rounded-lg border p-3">
    <div className="flex items-start gap-2">
      <span className="mt-0.5 inline-flex" aria-hidden="true"><ResultStatus tone="neutral" iconOnly label={t("dmDiagnosticTitle")} /></span>
      <div className="grid min-w-0 gap-1">
        <p className="text-sm font-medium">{t("dmDiagnosticTitle")}</p>
        <p className="whitespace-pre-wrap text-sm leading-6 [overflow-wrap:anywhere]">{diagnostic.message}</p>
      </div>
    </div>
    <dl className="grid min-w-0 gap-2 text-xs">
      {facts.map(([label, value]) => <div key={label} className="grid min-w-0 grid-cols-[minmax(0,1fr)_minmax(0,2fr)] gap-3">
        <dt className="text-muted-foreground">{label}</dt>
        <dd className="min-w-0 whitespace-pre-wrap [overflow-wrap:anywhere]">{value}</dd>
      </div>)}
    </dl>
  </div>;
}
