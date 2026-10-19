import { type ReactNode, useState } from "react";
import { isRuntimeAvailable } from "@/lib/local-runtime";
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
import {
  connectYoutubeApiKey,
  renameYoutubeApiKey,
  rotateYoutubeApiKey,
} from "@/lib/service-credentials";
import type { ServiceCredential } from "@/types";
import { youtubeCredentialEditAction } from "./youtube-credential-form";

export function YoutubeKeyDialog({
  credential,
  onChanged,
  trigger,
}: {
  credential?: ServiceCredential;
  onChanged: () => void | Promise<void>;
  trigger?: ReactNode;
}) {
  const { t } = useTranslation("accounts");
  const editing = Boolean(credential);
  const [open, setOpen] = useState(false);
  const [label, setLabel] = useState("");
  const [project, setProject] = useState("");
  const [apiKey, setApiKey] = useState("");
  const [loading, setLoading] = useState(false);
  const [tested, setTested] = useState(false);

  const changeOpen = (next: boolean) => {
    setOpen(next);
    if (next) {
      setLabel(credential?.label ?? "");
      setProject(typeof credential?.metadata.project === "string" ? credential.metadata.project : "");
      setApiKey("");
      setTested(false);
    } else {
      setApiKey("");
      setTested(false);
    }
  };

  const saveAndTest = async () => {
    if (!isRuntimeAvailable()) {
      toast.error(t("youtubeDialog.toasts.desktopConnectRequired"));
      return;
    }
    setLoading(true);
    setTested(false);
    try {
      const result = await connectYoutubeApiKey({
        label: label.trim(),
        project: project.trim() || null,
        api_key: apiKey,
      });
      setTested(true);
      toast.success(result.health.message, {
        description: t("youtubeDialog.toasts.savedDescription", {
          label: result.credential.label,
        }),
      });
      await onChanged();
      window.setTimeout(() => setOpen(false), 450);
    } catch (error) {
      toast.error(t("youtubeDialog.toasts.validationFailed"), { description: String(error) });
    } finally {
      setApiKey("");
      setLoading(false);
    }
  };

  const editAction = youtubeCredentialEditAction({
    credentialId: credential?.id ?? "",
    label,
    apiKey,
  });

  const rename = async () => {
    const nextLabel = label.trim();
    if (!credential || !nextLabel || nextLabel === credential.label) return;
    if (!isRuntimeAvailable()) {
      toast.error(t("youtubeDialog.toasts.desktopEditRequired"));
      return;
    }
    setLoading(true);
    try {
      await renameYoutubeApiKey({
        credential_id: credential.id,
        label: nextLabel,
        confirmed: true,
      });
      toast.success(t("youtubeDialog.toasts.renamed", { label: nextLabel }));
      await onChanged();
      changeOpen(false);
    } catch (error) {
      toast.error(t("youtubeDialog.toasts.renameFailed"), { description: String(error) });
    } finally {
      setApiKey("");
      setLoading(false);
    }
  };

  const rotate = async () => {
    if (!credential || editAction?.kind !== "rotate") return;
    if (label.trim() !== credential.label) {
      toast.error(t("youtubeDialog.toasts.saveNameFirst"), {
        description: t("youtubeDialog.toasts.saveNameFirstDescription"),
      });
      return;
    }
    if (!isRuntimeAvailable()) {
      toast.error(t("youtubeDialog.toasts.desktopRotateRequired"));
      return;
    }
    setLoading(true);
    try {
      const result = await rotateYoutubeApiKey(editAction.input);
      toast.success(result.health.message, {
        description: t("youtubeDialog.toasts.rotatedDescription", {
          label: result.credential.label,
        }),
      });
      await onChanged();
      changeOpen(false);
    } catch (error) {
      toast.error(t("youtubeDialog.toasts.rotateFailed"), { description: String(error) });
    } finally {
      setApiKey("");
      setLoading(false);
    }
  };

  return (
    <Dialog open={open} onOpenChange={changeOpen}>
      <DialogTrigger asChild>{trigger ?? (
        <Button>
          <Plus className="size-4" /> {t("youtubeDialog.add")}
        </Button>
      )}</DialogTrigger>
      <DialogContent className="sm:max-w-lg">
        <DialogHeader>
          <DialogTitle>
            {editing ? t("youtubeDialog.editTitle") : t("youtubeDialog.addTitle")}
          </DialogTitle>
          <DialogDescription>
            {editing
              ? t("youtubeDialog.editDescription")
              : t("youtubeDialog.addDescription")}
          </DialogDescription>
        </DialogHeader>
        <div className="space-y-4 py-2">
          <div className="grid gap-2">
            <Label htmlFor="youtube-key-label">{t("youtubeDialog.credentialName")}</Label>
            <Input
              id="youtube-key-label"
              value={label}
              onChange={(event) => setLabel(event.target.value)}
              placeholder={t("youtubeDialog.credentialPlaceholder")}
              autoComplete="off"
            />
          </div>
          <div className="grid gap-2">
            <Label htmlFor="youtube-project">{t("youtubeDialog.projectLabel")}</Label>
            <Input
              id="youtube-project"
              value={project}
              onChange={(event) => setProject(event.target.value)}
              placeholder={t("youtubeDialog.projectPlaceholder")}
              autoComplete="off"
              disabled={editing}
            />
            <p className="text-xs text-muted-foreground">
              {editing ? t("youtubeDialog.projectEditingHint") : t("youtubeDialog.projectHint")}
            </p>
          </div>
          <div className="grid gap-2">
            <Label htmlFor="youtube-api-key">{t("youtubeDialog.apiKeyLabel")}</Label>
            <Input
              id="youtube-api-key"
              type="password"
              value={apiKey}
              onChange={(event) => setApiKey(event.target.value)}
              placeholder={t("youtubeDialog.apiKeyPlaceholder")}
              autoComplete="new-password"
            />
          </div>
          <div className="rounded-lg border bg-muted/30 p-3 text-xs text-muted-foreground">
            <div className="mb-1 flex items-center gap-2 font-medium text-foreground">
              <KeyRound className="size-4" /> {t("youtubeDialog.securityTitle")}
            </div>
            {editing
              ? t("youtubeDialog.editSecurityDescription")
              : t("youtubeDialog.addSecurityDescription")}
          </div>
        </div>
        <DialogFooter>
          <Button variant="outline" onClick={() => changeOpen(false)}>
            {t("common.actions.cancel")}
          </Button>
          {editing ? (
            <>
              <Button
                variant="outline"
                onClick={() => void rename()}
                disabled={!label.trim() || label.trim() === credential?.label || loading}
              >
                {loading ? <Loader2 className="size-4 animate-spin" /> : <CheckCircle2 className="size-4" />}
                {t("youtubeDialog.saveName")}
              </Button>
              <Button
                onClick={() => void rotate()}
                disabled={!editAction || editAction.kind !== "rotate" || label.trim() !== credential?.label || loading}
              >
                {loading ? <Loader2 className="size-4 animate-spin" /> : <KeyRound className="size-4" />}
                {t("youtubeDialog.verifyAndReplace")}
              </Button>
            </>
          ) : (
            <Button onClick={() => void saveAndTest()} disabled={!label.trim() || !apiKey.trim() || loading}>
              {loading ? <Loader2 className="size-4 animate-spin" /> : tested ? <CheckCircle2 className="size-4" /> : <KeyRound className="size-4" />}
              {loading
                ? t("youtubeDialog.savingAndTesting")
                : tested
                  ? t("youtubeDialog.testPassed")
                  : t("youtubeDialog.saveAndTest")}
            </Button>
          )}
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
