import { useEffect, useMemo, useRef, useState } from "react";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { Braces, FileText, Loader2 } from "lucide-react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";

import {
  ADDRESS_INPUT_CLASS,
  AddressRow,
  ComposerBodyEditor,
  type ComposerBodyEditorHandle,
  type ComposerBodyTab,
} from "@/components/mail/composer-body-editor";
import { markdownToEmailHtml } from "@/components/mail/markdown-email";
import { invalidateEmailTemplateQueries, saveEmailTemplate } from "@/components/mail/queries";
import {
  EMAIL_TEMPLATE_BUILTIN_VARIABLES,
  templateEditorCanSave,
  templateEditorIntactHtml,
} from "@/components/mail/template-library-model";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { Input } from "@/components/ui/input";
import { cn } from "@/lib/utils";
import type { EmailTemplate } from "@/lib/creator";

export type TemplateEditorFields = {
  name: string;
  subject: string;
  text: string;
  html: string | null;
};

const BODY_ID = "mail-template-editor-text";
/** 与写信窗口同形态的留白：弹窗最宽 3xl，@lg 档位不再升级。 */
const INSET = "px-4 @md:px-8";

/**
 * 模板编辑器：模板库（新建/编辑）与写信浮层（另存为模板）共用同一个表单与
 * 保存 mutation，保存成功后统一走 invalidateEmailTemplateQueries 刷新模板族。
 * 视觉与写信窗口一致：行式字段 + 动作条 + 格式栏 + Markdown 正文 + 预览。
 */
export function MailTemplateEditorDialog({
  open,
  onOpenChange,
  mode,
  initialFields,
  existing,
  container,
  saveDisabled = false,
  onSaved,
  onCancel,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  mode: "create" | "update";
  initialFields: TemplateEditorFields;
  /** update 模式下的新鲜身份（revision 以此为准） */
  existing: { id: string; revision: number } | null;
  container?: HTMLElement | null;
  saveDisabled?: boolean;
  onSaved?: (saved: EmailTemplate) => void;
  onCancel?: () => void;
}) {
  const { t } = useTranslation("mail");
  const queryClient = useQueryClient();
  const [fields, setFields] = useState<TemplateEditorFields>(initialFields);
  // MCP 写的 HTML 模板：正文未被编辑前原样保留并原样保存；用户一改正文就丢掉，
  // 之后预览按 Markdown 派生、保存时 html 为 null（应用模板时再按 text 派生）。
  const [intactHtml, setIntactHtml] = useState<string | null>(() => templateEditorIntactHtml(initialFields.html));
  const [formatExpanded, setFormatExpanded] = useState(true);
  const [tab, setTab] = useState<ComposerBodyTab>("edit");
  const bodyEditor = useRef<ComposerBodyEditorHandle>(null);
  // 选了变量时 insertText 已把焦点移回正文 textarea，不让 Radix 在菜单关闭时
  // 把焦点归还给「插入变量」按钮（与格式栏「更多格式」同一处理）；Esc 取消仍回到触发器。
  const variableInserted = useRef(false);

  // 每次打开都从调用方给的 initialFields 重置：同一个编辑器实例会在
  // 不同模板之间复用，不能带着上一次的输入。
  useEffect(() => {
    if (!open) return;
    setFields(initialFields);
    setIntactHtml(templateEditorIntactHtml(initialFields.html));
    setFormatExpanded(true);
    setTab("edit");
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open]);

  const save = useMutation({
    mutationFn: saveEmailTemplate,
    onSuccess: async (saved) => {
      toast.success(mode === "update" ? t("templates.toasts.updated") : t("templates.toasts.created"));
      await invalidateEmailTemplateQueries(queryClient);
      onSaved?.(saved);
      onOpenChange(false);
    },
    onError: (error) => toast.error(t("templates.toasts.saveFailed"), { description: String(error) }),
  });

  // 预览只在本地派生，不写回 fields：派生 html 永远不存进模板。
  const previewHtml = useMemo(
    () => intactHtml ?? markdownToEmailHtml(fields.text),
    [intactHtml, fields.text],
  );
  const canSave = templateEditorCanSave({ ...fields, html: intactHtml }) && !saveDisabled;
  const cancel = () => {
    onCancel?.();
    onOpenChange(false);
  };
  const handleBodyChange = (text: string) => {
    setIntactHtml(null);
    setFields((current) => ({ ...current, text }));
  };

  return (
    <Dialog
      open={open}
      onOpenChange={(next) => {
        if (next) onOpenChange(true);
        else if (!save.isPending) cancel();
      }}
    >
      <DialogContent
        container={container}
        className="flex h-[min(82vh,760px)] flex-col gap-0 overflow-hidden p-0 sm:max-w-3xl @container"
      >
        {/* 纵向留白与模板库头部一致（两个弹窗从同一入口互切，头部高度不跳动）；
            右侧 pe-12 给 DialogContent 内置的关闭按钮让位，不随 INSET 走。 */}
        <DialogHeader className="shrink-0 border-b py-6 ps-4 pe-12 @md:ps-8">
          <DialogTitle>{mode === "update" ? t("templates.editor.editTitle") : t("templates.editor.createTitle")}</DialogTitle>
          <DialogDescription>
            {t("templates.editor.description")}
          </DialogDescription>
        </DialogHeader>

        <div className={cn("shrink-0", INSET)}>
          <AddressRow label={t("templates.editor.name")} htmlFor="mail-template-editor-name">
            <Input
              id="mail-template-editor-name"
              value={fields.name}
              placeholder={t("templates.editor.namePlaceholder")}
              className={ADDRESS_INPUT_CLASS}
              onChange={(event) => setFields((current) => ({ ...current, name: event.target.value }))}
            />
          </AddressRow>
          <AddressRow label={t("templates.editor.subject")} htmlFor="mail-template-editor-subject">
            <Input
              id="mail-template-editor-subject"
              value={fields.subject}
              placeholder={t("templates.editor.subjectPlaceholder", { first_name: "{{first_name}}" })}
              className={ADDRESS_INPUT_CLASS}
              onChange={(event) => setFields((current) => ({ ...current, subject: event.target.value }))}
            />
          </AddressRow>
        </div>

        <ComposerBodyEditor
          ref={bodyEditor}
          id={BODY_ID}
          inset={INSET}
          container={container}
          value={fields.text}
          onChange={handleBodyChange}
          html={previewHtml}
          disabled={false}
          plainText={false}
          placeholder={t("templates.editor.bodyPlaceholder")}
          formatExpanded={formatExpanded}
          onFormatExpandedChange={setFormatExpanded}
          tab={tab}
          onTabChange={setTab}
          actionBarExtras={(
            <DropdownMenu>
              <DropdownMenuTrigger asChild>
                <Button
                  type="button"
                  variant="ghost"
                  size="sm"
                  aria-label={t("templates.editor.insertVariable")}
                  className="data-[state=open]:bg-muted"
                >
                  <Braces aria-hidden="true" />
                  <span className="@max-xl:sr-only">{t("templates.editor.insertVariable")}</span>
                </Button>
              </DropdownMenuTrigger>
              <DropdownMenuContent
                align="start"
                className="w-72"
                onCloseAutoFocus={(event) => {
                  if (!variableInserted.current) return;
                  variableInserted.current = false;
                  event.preventDefault();
                }}
              >
                <DropdownMenuLabel className="pb-0">{t("templates.editor.builtinVariables")}</DropdownMenuLabel>
                <p className="px-2 pb-1.5 text-xs text-muted-foreground">
                  {t("templates.editor.variableDescription")}
                </p>
                {EMAIL_TEMPLATE_BUILTIN_VARIABLES.map((variable) => (
                  <DropdownMenuItem
                    key={variable.name}
                    onSelect={() => {
                      variableInserted.current = true;
                      bodyEditor.current?.insertText(`{{${variable.name}}}`);
                    }}
                  >
                    <span className="font-mono text-xs">{`{{${variable.name}}}`}</span>
                    <span className="ml-auto text-muted-foreground">
                      {t(`templates.variables.${variable.name}`)}
                    </span>
                  </DropdownMenuItem>
                ))}
              </DropdownMenuContent>
            </DropdownMenu>
          )}
          aboveBody={intactHtml !== null && (
            <p className={cn("shrink-0 pt-2 text-xs text-muted-foreground", INSET)}>
              {t("composer.template.htmlIntactHint")}
            </p>
          )}
        />

        <div className={cn("flex h-14 shrink-0 items-center justify-end gap-2 border-t", INSET)}>
          <Button variant="outline" disabled={save.isPending} onClick={cancel}>{t("common.cancel")}</Button>
          <Button
            disabled={!canSave || save.isPending}
            onClick={() => save.mutate({
              id: mode === "update" ? existing?.id ?? null : null,
              name: fields.name.trim(),
              subject: fields.subject,
              text: fields.text,
              // 编辑器没有 html 输入：Markdown 派生出的 html 是正文的快照，随正文
              // 改动会分叉，所以不随模板保存（应用时按 text 重新派生）；只有
              // MCP 写的模板 html 且正文未被编辑时才原样保留。
              html: intactHtml,
              expected_revision: mode === "update" ? existing?.revision ?? null : null,
            })}
          >
            {save.isPending ? <Loader2 className="animate-spin" /> : <FileText />}
            {mode === "update" ? t("templates.editor.saveChanges") : t("templates.editor.createAction")}
          </Button>
        </div>
      </DialogContent>
    </Dialog>
  );
}
