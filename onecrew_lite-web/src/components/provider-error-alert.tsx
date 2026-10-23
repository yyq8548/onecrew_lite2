// Display owner-provided evidence. The caller owns the operation scope and any
// explicit read retry; diagnostic codes never enable write retries.

import { RefreshCw } from "lucide-react";
import { useTranslation } from "react-i18next";

import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import { presentDmFailure, presentProviderError, type DmFailure } from "@/lib/provider-error";

type Props = {
  /** The operation or verification step, independent of its diagnostic. */
  scope: string;
  onRetry?: () => void;
  variant?: "destructive" | "warning";
} & ({ error: unknown; failure?: never } | { failure: DmFailure; error?: never });

export function ProviderErrorAlert(props: Props) {
  const { scope, onRetry, variant = "destructive" } = props;
  const { t } = useTranslation("common");
  const presentation = props.failure !== undefined
    ? presentDmFailure(props.failure) : presentProviderError(props.error);
  return (
    <Alert variant={variant} className="min-w-0 self-stretch">
      <AlertTitle className="line-clamp-none">
        {scope}
      </AlertTitle>
      <AlertDescription className="min-w-0 text-current/90">
        <p className="break-words">{presentation.summary}</p>
        {presentation.details.length > 0 && <details className="w-full min-w-0">
          <summary className="cursor-pointer text-xs opacity-80">
            {t("providerError.technicalDetails")}
          </summary>
          <dl className="mt-1 space-y-1 text-xs opacity-80">
            {presentation.details.map(({ label, value }) => <div key={label}>
              <dt>{label}</dt><dd className="break-all font-mono">{value}</dd>
            </div>)}
          </dl>
        </details>}
        {onRetry && (
          <Button variant="outline" size="sm" className="mt-1" onClick={onRetry}>
            <RefreshCw />
            {t("actions.retry")}
          </Button>
        )}
      </AlertDescription>
    </Alert>
  );
}
