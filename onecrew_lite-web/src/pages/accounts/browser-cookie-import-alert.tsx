import { KeyRound } from "lucide-react";
import { useTranslation } from "react-i18next";

import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import type { OperatorBrowserStatus } from "@/lib/operator-account";
import { browserCookieImportCopy } from "./browser-environment";

export function BrowserCookieImportAlert({ status }: { status: OperatorBrowserStatus | null }) {
  const { t } = useTranslation("accounts");
  const copy = browserCookieImportCopy(status, t);
  return (
    <Alert variant="warning">
      <KeyRound />
      <AlertTitle>{copy.title}</AlertTitle>
      <AlertDescription>
        <p>{copy.description}</p>
      </AlertDescription>
    </Alert>
  );
}
