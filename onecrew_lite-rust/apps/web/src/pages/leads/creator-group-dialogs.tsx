import { useEffect, useRef, useState } from "react";
import { FolderPlus, Loader2, MoreHorizontal, Pencil, Trash2 } from "lucide-react";
import { useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";
import { useTranslation } from "react-i18next";

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
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import {
  CREATOR_GROUPS_QUERY_KEY,
  createCreatorGroup,
  deleteCreatorGroup,
  updateCreatorGroup,
} from "@/lib/creator-groups";
import { CREATORS_QUERY_KEY, type CreatorGroup } from "@/lib/creator";

type PortalContainer = HTMLElement | null;
type GroupDialogError =
  | { kind: "fixed"; key: "groups.create.nameRequired" }
  | { kind: "detail"; value: string };

export function CreateCreatorGroupDialog({ container }: { container: PortalContainer }) {
  const { t } = useTranslation("leads");
  const queryClient = useQueryClient();
  const [open, setOpen] = useState(false);
  const [name, setName] = useState("");
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<GroupDialogError | null>(null);
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    if (open) window.setTimeout(() => inputRef.current?.focus(), 0);
  }, [open]);

  const changeOpen = (next: boolean) => {
    if (pending) return;
    setOpen(next);
    if (!next) {
      setName("");
      setError(null);
    }
  };

  const submit = async () => {
    const normalized = name.trim();
    if (!normalized) {
      setError({ kind: "fixed", key: "groups.create.nameRequired" });
      inputRef.current?.focus();
      return;
    }
    setPending(true);
    setError(null);
    try {
      await createCreatorGroup({ name: normalized });
      await queryClient.invalidateQueries({ queryKey: CREATOR_GROUPS_QUERY_KEY });
      toast.success(t("groups.create.success"));
      setOpen(false);
      setName("");
    } catch (cause) {
      setError({ kind: "detail", value: String(cause) });
    } finally {
      setPending(false);
    }
  };

  const errorMessage = error?.kind === "fixed" ? t(error.key) : error?.value ?? null;

  return (
    <Dialog open={open} onOpenChange={changeOpen}>
      <DialogTrigger asChild>
        <Button variant="outline" size="sm">
          <FolderPlus className="size-4" />{t("groups.create.trigger")}
        </Button>
      </DialogTrigger>
      <DialogContent container={container} onEscapeKeyDown={(event) => pending && event.preventDefault()}>
        <DialogHeader>
          <DialogTitle>{t("groups.create.title")}</DialogTitle>
          <DialogDescription>{t("groups.create.description")}</DialogDescription>
        </DialogHeader>
        <form className="space-y-4" onSubmit={(event) => { event.preventDefault(); void submit(); }}>
          <div className="grid gap-2">
            <Label htmlFor="creator-group-name">{t("groups.create.name")}</Label>
            <Input
              ref={inputRef}
              id="creator-group-name"
              value={name}
              maxLength={64}
              autoComplete="off"
              disabled={pending}
              aria-invalid={Boolean(error)}
              aria-describedby={error ? "creator-group-name-error" : undefined}
              onChange={(event) => setName(event.target.value)}
            />
            {errorMessage && <p id="creator-group-name-error" role="alert" className="text-sm text-destructive">{errorMessage}</p>}
          </div>
          <DialogFooter>
            <Button type="button" variant="outline" disabled={pending} onClick={() => changeOpen(false)}>{t("common.actions.cancel")}</Button>
            <Button type="submit" disabled={pending || !name.trim()}>
              {pending && <Loader2 className="size-4 animate-spin" />}
              {pending ? t("groups.create.submitting") : t("groups.create.submit")}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}

export function CreatorGroupActions({
  group,
  container,
}: {
  group: CreatorGroup;
  container: PortalContainer;
}) {
  const { t } = useTranslation("leads");
  const queryClient = useQueryClient();
  const [renameOpen, setRenameOpen] = useState(false);
  const [deleteOpen, setDeleteOpen] = useState(false);
  const [name, setName] = useState(group.name);
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<GroupDialogError | null>(null);
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    if (!renameOpen) return;
    setName(group.name);
    setError(null);
    window.setTimeout(() => inputRef.current?.focus(), 0);
  }, [group.name, renameOpen]);

  const rename = async () => {
    const normalized = name.trim();
    if (!normalized) {
      setError({ kind: "fixed", key: "groups.create.nameRequired" });
      inputRef.current?.focus();
      return;
    }
    setPending(true);
    setError(null);
    try {
      await updateCreatorGroup({
        group_id: group.id,
        expected_revision: group.revision,
        name: normalized,
      });
      await queryClient.invalidateQueries({ queryKey: CREATOR_GROUPS_QUERY_KEY });
      toast.success(t("groups.rename.success"));
      setRenameOpen(false);
    } catch (cause) {
      setError({ kind: "detail", value: String(cause) });
    } finally {
      setPending(false);
    }
  };

  const errorMessage = error?.kind === "fixed" ? t(error.key) : error?.value ?? null;

  const remove = async () => {
    setPending(true);
    try {
      const result = await deleteCreatorGroup({
        group_id: group.id,
        expected_revision: group.revision,
      });
      await Promise.all([
        queryClient.invalidateQueries({ queryKey: CREATOR_GROUPS_QUERY_KEY }),
        queryClient.invalidateQueries({ queryKey: CREATORS_QUERY_KEY }),
      ]);
      toast.success(t("groups.delete.success"), {
        description: result.uncategorized_creator_count > 0
          ? t("groups.delete.movedToUncategorized", { count: result.uncategorized_creator_count })
          : undefined,
      });
      setDeleteOpen(false);
    } catch (cause) {
      toast.error(t("groups.delete.failed"), { description: String(cause) });
    } finally {
      setPending(false);
    }
  };

  return (
    <>
      <DropdownMenu>
        <DropdownMenuTrigger asChild>
          <Button variant="ghost" size="icon" className="mr-2 size-8" aria-label={t("groups.actions", { name: group.name })}>
            <MoreHorizontal className="size-4" />
          </Button>
        </DropdownMenuTrigger>
        <DropdownMenuContent align="end">
          <DropdownMenuItem onSelect={() => setRenameOpen(true)}>
            <Pencil className="size-4" />{t("groups.rename.action")}
          </DropdownMenuItem>
          <DropdownMenuSeparator />
          <DropdownMenuItem variant="destructive" onSelect={() => setDeleteOpen(true)}>
            <Trash2 className="size-4" />{t("groups.delete.action")}
          </DropdownMenuItem>
        </DropdownMenuContent>
      </DropdownMenu>

      <Dialog open={renameOpen} onOpenChange={(next) => !pending && setRenameOpen(next)}>
        <DialogContent container={container} onEscapeKeyDown={(event) => pending && event.preventDefault()}>
          <DialogHeader>
            <DialogTitle>{t("groups.rename.title")}</DialogTitle>
            <DialogDescription>{t("groups.rename.description")}</DialogDescription>
          </DialogHeader>
          <form className="space-y-4" onSubmit={(event) => { event.preventDefault(); void rename(); }}>
            <div className="grid gap-2">
              <Label htmlFor={`creator-group-rename-${group.id}`}>{t("groups.create.name")}</Label>
              <Input
                ref={inputRef}
                id={`creator-group-rename-${group.id}`}
                value={name}
                maxLength={64}
                disabled={pending}
                aria-invalid={Boolean(error)}
                onChange={(event) => setName(event.target.value)}
              />
              {errorMessage && <p role="alert" className="text-sm text-destructive">{errorMessage}</p>}
            </div>
            <DialogFooter>
              <Button type="button" variant="outline" disabled={pending} onClick={() => setRenameOpen(false)}>{t("common.actions.cancel")}</Button>
              <Button type="submit" disabled={pending || !name.trim()}>
                {pending && <Loader2 className="size-4 animate-spin" />}
                {pending ? t("groups.rename.submitting") : t("groups.rename.submit")}
              </Button>
            </DialogFooter>
          </form>
        </DialogContent>
      </Dialog>

      <Dialog open={deleteOpen} onOpenChange={(next) => !pending && setDeleteOpen(next)}>
        <DialogContent container={container} onEscapeKeyDown={(event) => pending && event.preventDefault()}>
          <DialogHeader>
            <DialogTitle>{t("groups.delete.title")}</DialogTitle>
            <DialogDescription>
              {t("groups.delete.description", { name: group.name })}
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button type="button" variant="outline" disabled={pending} onClick={() => setDeleteOpen(false)}>{t("common.actions.cancel")}</Button>
            <Button type="button" variant="destructive" disabled={pending} onClick={() => void remove()}>
              {pending && <Loader2 className="size-4 animate-spin" />}
              {pending ? t("groups.delete.submitting") : t("groups.delete.submit")}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </>
  );
}
