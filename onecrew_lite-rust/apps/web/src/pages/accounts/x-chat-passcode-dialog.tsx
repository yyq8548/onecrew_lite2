import { useState } from "react";
import { invoke } from "@/lib/local-runtime";
import { KeyRound, Loader2, MessageCircleOff } from "lucide-react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";

import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { RadioGroup, RadioGroupItem } from "@/components/ui/radio-group";
import { tone } from "@/lib/ui";
import { cn } from "@/lib/utils";
import type { Account } from "@/types";

import {
  canSubmitXChatPasscodeSetting,
  type XChatConnectionChoice,
  xChatPasscodeStatusView,
} from "./x-chat-passcode-state";

const STATUS_TONE = {
  success: tone.success,
  warning: tone.warning,
  destructive: "bg-destructive text-white",
  muted: tone.muted,
} as const;

export function XChatPasscodeDialog({
  account,
  open,
  onOpenChange,
  onChanged,
}: {
  account: Account;
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onChanged: () => void;
}) {
  const { t } = useTranslation("accounts");
  const [preference, setPreference] = useState<XChatConnectionChoice>(null);
  const [passcode, setPasscode] = useState("");
  const [busy, setBusy] = useState(false);
  const status = xChatPasscodeStatusView(account.x_chat_passcode_status, t);
  const configured = account.x_chat_passcode_status === "configured";

  async function save() {
    if (!canSubmitXChatPasscodeSetting(preference, passcode)) return;
    setBusy(true);
    try {
      if (preference === "enabled") {
        await invoke("onecrew_operator_account_x_chat_passcode_set_in_app", {
          input: {
            operator_account_id: account.id,
            x_chat_passcode: passcode,
          },
        });
        toast.success(configured ? t("xChat.toasts.updated") : t("xChat.toasts.enabled"), {
          description: t("xChat.toasts.savedDescription", { username: account.username }),
        });
      } else {
        await invoke("onecrew_operator_account_x_chat_passcode_clear_in_app", {
          input: {
            operator_account_id: account.id,
            confirmed: true,
          },
        });
        toast.success(t("xChat.toasts.cleared"), {
          description: t("xChat.toasts.clearedDescription", { username: account.username }),
        });
      }
      setPasscode("");
      setPreference(null);
      onOpenChange(false);
      onChanged();
    } catch (error) {
      toast.error(t("xChat.toasts.saveFailed"), { description: String(error) });
    } finally {
      setBusy(false);
    }
  }

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-lg">
        <DialogHeader>
          <DialogTitle>{t("xChat.dialog.title")}</DialogTitle>
          <DialogDescription>
            {t("xChat.dialog.description", { username: account.username })}
          </DialogDescription>
        </DialogHeader>

        <div className="space-y-4">
          <div className="rounded-lg border p-3">
            <Badge variant="secondary" className={cn("border-transparent", STATUS_TONE[status.tone])}>
              {status.label}
            </Badge>
            <p className="mt-2 text-xs text-muted-foreground">{status.description}</p>
          </div>

          <RadioGroup
            value={preference ?? undefined}
            onValueChange={(value) => {
              const next = value as Exclude<XChatConnectionChoice, null>;
              setPreference(next);
              if (next === "disabled") setPasscode("");
            }}
            className="gap-2"
          >
            <Label className="flex cursor-pointer items-start gap-3 rounded-lg border p-3 font-normal">
              <RadioGroupItem value="enabled" className="mt-0.5" />
              <span>
                <span className="block font-medium">
                  {configured ? t("xChat.dialog.replace") : t("xChat.dialog.enableAndSet")}
                </span>
                <span className="mt-0.5 block text-xs text-muted-foreground">
                  {t("xChat.dialog.enabledDescription")}
                </span>
              </span>
            </Label>
            <Label className="flex cursor-pointer items-start gap-3 rounded-lg border p-3 font-normal">
              <RadioGroupItem value="disabled" className="mt-0.5" />
              <span>
                <span className="block font-medium">{t("xChat.dialog.disableAndClear")}</span>
                <span className="mt-0.5 block text-xs text-muted-foreground">
                  {t("xChat.dialog.disabledDescription")}
                </span>
              </span>
            </Label>
          </RadioGroup>

          {preference === "enabled" && (
            <div className="space-y-2">
              <Label htmlFor={`x-chat-passcode-${account.id}`}>{t("xChat.dialog.newPasscode")}</Label>
              <Input
                id={`x-chat-passcode-${account.id}`}
                type="password"
                autoComplete="new-password"
                value={passcode}
                onChange={(event) => setPasscode(event.target.value)}
                placeholder={t("xChat.dialog.passcodePlaceholder")}
              />
              <p className="text-xs text-muted-foreground">
                {t("xChat.dialog.passcodeHint")}
              </p>
            </div>
          )}

          {preference === "disabled" && (
            <Alert variant="warning">
              <MessageCircleOff />
              <AlertTitle>{t("xChat.dialog.disableWarningTitle")}</AlertTitle>
              <AlertDescription>
                {t("xChat.dialog.disableWarningDescription")}
              </AlertDescription>
            </Alert>
          )}
        </div>

        <DialogFooter>
          <Button type="button" variant="outline" disabled={busy} onClick={() => onOpenChange(false)}>
            {t("common.actions.cancel")}
          </Button>
          <Button
            type="button"
            variant={preference === "disabled" ? "destructive" : "default"}
            disabled={busy || !canSubmitXChatPasscodeSetting(preference, passcode)}
            onClick={() => void save()}
          >
            {busy ? <Loader2 className="size-4 animate-spin" /> : preference === "disabled" ? <MessageCircleOff /> : <KeyRound />}
            {preference === "disabled"
              ? t("xChat.dialog.disableAction")
              : configured
                ? t("xChat.dialog.saveAction")
                : t("xChat.dialog.enableAction")}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
