import { useState } from "react";
import { invoke, isRuntimeAvailable } from "@/lib/local-runtime";
import { CheckCircle2, KeyRound, Loader2, Plus } from "lucide-react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";

import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import type { ServiceCredentialConnectResult } from "@/types";

export function ResendCredentialDialog({ onChanged }: { onChanged: () => void }) {
  const { t } = useTranslation("accounts");
  const [open, setOpen] = useState(false);
  const [label, setLabel] = useState("");
  const [emailAddress, setEmailAddress] = useState("");
  const [displayName, setDisplayName] = useState("");
  const [apiKey, setApiKey] = useState("");
  const [loading, setLoading] = useState(false);
  const [tested, setTested] = useState(false);

  const changeOpen = (next: boolean) => {
    setOpen(next);
    if (!next) {
      setApiKey("");
      setTested(false);
    }
  };

  const saveAndTest = async () => {
    if (!isRuntimeAvailable()) {
      toast.error(t("resendDialog.toasts.desktopRequired"));
      return;
    }
    setLoading(true);
    setTested(false);
    try {
      const result = await invoke<ServiceCredentialConnectResult>("connect_resend_credential", {
        input: {
          label,
          email_address: emailAddress,
          display_name: displayName.trim() || null,
          api_key: apiKey,
        },
      });
      setTested(true);
      toast.success(result.health.message, {
        description: t("resendDialog.toasts.savedDescription", {
          email: result.credential.mailbox?.email_address ?? emailAddress.trim(),
        }),
      });
      onChanged();
      window.setTimeout(() => changeOpen(false), 450);
    } catch (error) {
      toast.error(t("resendDialog.toasts.validationFailed"), { description: String(error) });
    } finally {
      setApiKey("");
      setLoading(false);
    }
  };

  return (
    <Dialog open={open} onOpenChange={changeOpen}>
      <DialogTrigger asChild>
        <Button><Plus className="size-4" />{t("resendDialog.add")}</Button>
      </DialogTrigger>
      <DialogContent className="sm:max-w-lg">
        <DialogHeader>
          <DialogTitle>{t("resendDialog.title")}</DialogTitle>
          <DialogDescription>
            {t("resendDialog.description")}
          </DialogDescription>
        </DialogHeader>
        <div className="space-y-4 py-2">
          <div className="grid gap-2">
            <Label htmlFor="resend-label">{t("resendDialog.credentialName")}</Label>
            <Input id="resend-label" value={label} onChange={(event) => setLabel(event.target.value)} placeholder={t("resendDialog.credentialPlaceholder")} autoComplete="off" />
          </div>
          <div className="grid gap-2">
            <Label htmlFor="resend-email">{t("resendDialog.senderEmail")}</Label>
            <Input id="resend-email" type="email" value={emailAddress} onChange={(event) => setEmailAddress(event.target.value)} placeholder={t("resendDialog.senderEmailPlaceholder")} autoComplete="email" />
          </div>
          <div className="grid gap-2">
            <Label htmlFor="resend-display-name">{t("resendDialog.displayNameOptional")}</Label>
            <Input id="resend-display-name" value={displayName} onChange={(event) => setDisplayName(event.target.value)} placeholder={t("resendDialog.displayNamePlaceholder")} autoComplete="organization" />
          </div>
          <div className="grid gap-2">
            <Label htmlFor="resend-api-key">{t("resendDialog.apiKeyLabel")}</Label>
            <Input id="resend-api-key" type="password" value={apiKey} onChange={(event) => setApiKey(event.target.value)} placeholder={t("resendDialog.apiKeyPlaceholder")} autoComplete="new-password" />
          </div>
          <div className="rounded-lg border bg-muted/30 p-3 text-xs text-muted-foreground">
            <div className="mb-1 flex items-center gap-2 font-medium text-foreground"><KeyRound className="size-4" />{t("resendDialog.securityTitle")}</div>
            {t("resendDialog.securityDescription")}
          </div>
        </div>
        <DialogFooter>
          <Button variant="outline" onClick={() => changeOpen(false)}>{t("common.actions.cancel")}</Button>
          <Button onClick={() => void saveAndTest()} disabled={!label.trim() || !emailAddress.trim() || !apiKey.trim() || loading}>
            {loading ? <Loader2 className="size-4 animate-spin" /> : tested ? <CheckCircle2 className="size-4" /> : <KeyRound className="size-4" />}
            {loading
              ? t("resendDialog.savingAndTesting")
              : tested
                ? t("resendDialog.testPassed")
                : t("resendDialog.saveAndTest")}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
