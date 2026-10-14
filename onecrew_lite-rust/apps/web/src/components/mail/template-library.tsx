import { useEffect, useMemo, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Copy, FileText, Loader2, MailPlus, MoreHorizontal, Pencil, Plus, Search, Trash2 } from "lucide-react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";

import { TemplatesButton } from "@/components/mail/folder-nav";
import {
  EMAIL_TEMPLATE_LIST_QUERY_KEY,
  deleteEmailTemplate,
  duplicateEmailTemplate,
  getEmailTemplate,
  invalidateEmailTemplateQueries,
  listAllEmailTemplates,
} from "@/components/mail/queries";
import {
  MailTemplateEditorDialog,
  type TemplateEditorFields,
} from "@/components/mail/template-editor-dialog";
import {
  filterEmailTemplates,
  templateLibraryShowsToolbarCreate,
} from "@/components/mail/template-library-model";
import { emailTemplateLoadErrorDescription } from "@/components/mail/template-error-presentation";
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
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import {
  Empty,
  EmptyContent,
  EmptyDescription,
  EmptyHeader,
  EmptyMedia,
  EmptyTitle,
} from "@/components/ui/empty";
import { Input } from "@/components/ui/input";
import { ScrollArea } from "@/components/ui/scroll-area";
import { formatDate, type EmailTemplate, type EmailTemplateSummary } from "@/lib/creator";

type EditorSession = {
  mode: "create" | "update";
  initialFields: TemplateEditorFields;
  existing: { id: string; revision: number } | null;
};

const emptyEditorFields = (): TemplateEditorFields => ({
  name: "",
  subject: "",
  text: "",
  html: null,
});

/**
 * 侧栏入口的薄壳：模板按钮 + 受控的模板库对话框。Mail.tsx 继续用这个 API。
 */
export function MailTemplateLibrary({
  collapsed,
  canCompose,
  onUseTemplate,
}: {
  collapsed: boolean;
  canCompose: boolean;
  onUseTemplate: (templateId: string) => void;
}) {
  const [open, setOpen] = useState(false);
  return (
    <>
      <TemplatesButton collapsed={collapsed} onClick={() => setOpen(true)} />
      <MailTemplateLibraryDialog
        open={open}
        onOpenChange={setOpen}
        canCompose={canCompose}
        onUseTemplate={onUseTemplate}
      />
    </>
  );
}

/**
 * 受控的模板库：卡片网格 + 新建/编辑/复制/删除。编辑器与删除确认打开时
 * 模板库自己先退到后面（open && 无子对话框），子对话框关闭后回到模板库，
 * 不需要调用方来回切换 open。
 */
export function MailTemplateLibraryDialog({
  open,
  onOpenChange,
  canCompose,
  container,
  onUseTemplate,
  onSaved,
  onDeleted,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  canCompose: boolean;
  container?: HTMLElement | null;
  onUseTemplate: (templateId: string) => void;
  onSaved?: (saved: EmailTemplate) => void;
  onDeleted?: (templateId: string) => void;
}) {
  const { t } = useTranslation("mail");
  const queryClient = useQueryClient();
  const [search, setSearch] = useState("");
  const [editor, setEditor] = useState<EditorSession | null>(null);
  const [templateToDelete, setTemplateToDelete] = useState<EmailTemplateSummary | null>(null);

  // 调用方关掉模板库时，子对话框也一并收起，下次打开从列表开始。
  useEffect(() => {
    if (!open) {
      setEditor(null);
      setTemplateToDelete(null);
    }
  }, [open]);

  const templates = useQuery({
    queryKey: EMAIL_TEMPLATE_LIST_QUERY_KEY,
    queryFn: () => listAllEmailTemplates(),
    retry: false,
  });
  const allTemplates = useMemo(() => templates.data ?? [], [templates.data]);
  const filteredTemplates = useMemo(
    () => filterEmailTemplates(allTemplates, search),
    [search, allTemplates],
  );
  const showToolbarCreate = templateLibraryShowsToolbarCreate({
    total: allTemplates.length,
    query: search,
  });

  const openCreateEditor = () => {
    setEditor({ mode: "create", initialFields: emptyEditorFields(), existing: null });
  };
  const openEditEditor = async (template: EmailTemplateSummary) => {
    let full;
    try {
      full = await getEmailTemplate({ template_id: template.id });
    } catch (error) {
      // Never open the editor with a fabricated body: saving it would
      // silently overwrite the stored template content.
      toast.error(t("templates.toasts.readContentFailed"), { description: String(error) });
      return;
    }
    setEditor({
      mode: "update",
      initialFields: {
        name: full.name,
        subject: full.subject,
        text: full.text,
        html: full.html,
      },
      existing: { id: full.id, revision: full.revision },
    });
  };
  const returnToLibrary = () => {
    setEditor(null);
    setTemplateToDelete(null);
  };

  const removeTemplate = useMutation({
    mutationFn: deleteEmailTemplate,
    onSuccess: async (_result, input) => {
      setTemplateToDelete(null);
      toast.success(t("templates.toasts.deleted"));
      await invalidateEmailTemplateQueries(queryClient);
      onDeleted?.(input.template_id);
    },
    onError: (error) => toast.error(t("templates.toasts.deleteFailed"), { description: String(error) }),
  });
  const copyTemplate = useMutation({
    mutationFn: (template: EmailTemplateSummary) => (
      duplicateEmailTemplate(
        template.id,
        allTemplates.map((item) => item.name),
        (name, index) => t(
          index === 1 ? "templates.copyName" : "templates.copyNameNumbered",
          { name, index },
        ),
      )
    ),
    onSuccess: async () => {
      toast.success(t("templates.toasts.copied"));
      await invalidateEmailTemplateQueries(queryClient);
    },
    onError: (error) => toast.error(t("templates.toasts.copyFailed"), { description: String(error) }),
  });

  const libraryVisible = open && !editor && !templateToDelete;
  const useTemplateTitle = canCompose ? t("templates.useTitle") : t("templates.useUnavailableTitle");

  return (
    <>
      <Dialog open={libraryVisible} onOpenChange={onOpenChange}>
        <DialogContent
          container={container}
          className="flex h-[min(82vh,760px)] flex-col gap-0 p-0 sm:max-w-3xl"
        >
          <DialogHeader className="shrink-0 border-b p-6">
            <DialogTitle>{t("templates.libraryTitle")}</DialogTitle>
            <DialogDescription>
              {t("templates.libraryDescription")}
            </DialogDescription>
          </DialogHeader>
          <div className="flex shrink-0 items-center gap-2 p-4">
            <div className="relative min-w-0 flex-1">
              <Search className="pointer-events-none absolute left-3 top-1/2 size-4 -translate-y-1/2 text-muted-foreground" />
              <Input
                value={search}
                onChange={(event) => setSearch(event.target.value)}
                placeholder={t("templates.searchPlaceholder")}
                className="pl-9"
                aria-label={t("templates.searchLabel")}
              />
            </div>
            {showToolbarCreate && <Button onClick={openCreateEditor}><Plus />{t("templates.create")}</Button>}
          </div>
          {!canCompose && (
            <p className="mx-4 mb-3 shrink-0 rounded-lg bg-muted/60 px-3 py-2 text-xs leading-5 text-muted-foreground">
              {t("templates.cannotCompose")}
            </p>
          )}
          <ScrollArea className="min-h-0 flex-1 border-t">
            {templates.isLoading ? (
              <div className="grid h-48 place-items-center"><Loader2 className="animate-spin text-muted-foreground" /></div>
            ) : templates.error ? (
              <div className="space-y-1 p-6 text-sm text-destructive">
                <p className="font-medium">{t("templates.errors.libraryTitle")}</p>
                <p>{emailTemplateLoadErrorDescription(templates.error, t)}</p>
              </div>
            ) : filteredTemplates.length ? (
              <div className="grid gap-3 p-4 md:grid-cols-2">
                {filteredTemplates.map((template) => (
                  <article key={template.id} className="flex flex-col rounded-xl border bg-card p-4 text-card-foreground">
                    <div className="flex-1">
                      <div className="flex items-start gap-3">
                        <span className="grid size-10 shrink-0 place-items-center rounded-lg bg-muted text-muted-foreground">
                          <FileText className="size-5" />
                        </span>
                        <div className="min-w-0 flex-1">
                          <h3 className="truncate text-sm font-semibold">{template.name}</h3>
                          <p className="mt-0.5 truncate text-xs text-muted-foreground">{template.subject}</p>
                        </div>
                        <DropdownMenu>
                          <DropdownMenuTrigger asChild>
                            <Button variant="ghost" size="icon-sm" className="shrink-0" aria-label={t("templates.actionsLabel")}>
                              <MoreHorizontal />
                            </Button>
                          </DropdownMenuTrigger>
                          <DropdownMenuContent align="end">
                            <DropdownMenuItem onSelect={() => openEditEditor(template)}>
                              <Pencil />{t("common.edit")}
                            </DropdownMenuItem>
                            <DropdownMenuItem
                              disabled={copyTemplate.isPending}
                              onSelect={() => copyTemplate.mutate(template)}
                            >
                              <Copy />{t("common.copy")}
                            </DropdownMenuItem>
                            <DropdownMenuItem
                              variant="destructive"
                              className="text-destructive"
                              onSelect={() => setTemplateToDelete(template)}
                            >
                              <Trash2 />{t("common.delete")}
                            </DropdownMenuItem>
                          </DropdownMenuContent>
                        </DropdownMenu>
                      </div>
                      {template.variables.length > 0 && (
                        <div className="mt-3 flex flex-wrap gap-1">
                          {template.variables.map((variable) => <Badge key={variable} variant="outline">{variable}</Badge>)}
                        </div>
                      )}
                    </div>
                    <div className="mt-4 flex items-center justify-between gap-2 border-t pt-3">
                      <span className="truncate text-xs text-muted-foreground">
                        {t("templates.updatedAt", { date: formatDate(template.updated_at) })}
                      </span>
                      <Button
                        size="sm"
                        disabled={!canCompose}
                        title={useTemplateTitle}
                        onClick={() => {
                          onOpenChange(false);
                          onUseTemplate(template.id);
                        }}
                      >
                        {t("templates.use")}
                      </Button>
                    </div>
                  </article>
                ))}
              </div>
            ) : search.trim() ? (
              <Empty className="h-56">
                <EmptyHeader>
                  <EmptyMedia variant="icon"><FileText /></EmptyMedia>
                  <EmptyTitle>{t("templates.noMatchesTitle")}</EmptyTitle>
                  <EmptyDescription>{t("templates.noMatchesDescription")}</EmptyDescription>
                </EmptyHeader>
              </Empty>
            ) : (
              <Empty className="min-h-72">
                <EmptyHeader>
                  <EmptyMedia variant="icon"><MailPlus /></EmptyMedia>
                  <EmptyTitle>{t("templates.emptyTitle")}</EmptyTitle>
                  <EmptyDescription>{t("templates.emptyDescription")}</EmptyDescription>
                </EmptyHeader>
                <EmptyContent>
                  <Button onClick={openCreateEditor}><Plus />{t("templates.add")}</Button>
                </EmptyContent>
              </Empty>
            )}
          </ScrollArea>
        </DialogContent>
      </Dialog>

      <MailTemplateEditorDialog
        open={open && Boolean(editor)}
        onOpenChange={(next) => {
          if (!next) returnToLibrary();
        }}
        mode={editor?.mode ?? "create"}
        initialFields={editor?.initialFields ?? emptyEditorFields()}
        existing={editor?.existing ?? null}
        container={container}
        onSaved={onSaved}
      />

      <Dialog
        open={open && Boolean(templateToDelete)}
        onOpenChange={(next) => !next && !removeTemplate.isPending && returnToLibrary()}
      >
        <DialogContent container={container}>
          <DialogHeader>
            <DialogTitle>{t("templates.deleteTitle")}</DialogTitle>
            <DialogDescription>
              {t("templates.deleteDescription", { name: templateToDelete?.name ?? "" })}
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button variant="outline" disabled={removeTemplate.isPending} onClick={returnToLibrary}>{t("common.cancel")}</Button>
            <Button
              variant="destructive"
              disabled={!templateToDelete || removeTemplate.isPending}
              onClick={() => templateToDelete && removeTemplate.mutate({
                template_id: templateToDelete.id,
                expected_revision: templateToDelete.revision,
              })}
            >
              {removeTemplate.isPending ? <Loader2 className="animate-spin" /> : <Trash2 />}{t("templates.deleteAction")}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </>
  );
}
