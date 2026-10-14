import {
  forwardRef,
  useEffect,
  useImperativeHandle,
  useMemo,
  useRef,
  useState,
  type CSSProperties,
  type KeyboardEvent,
} from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import {
  ChevronRight,
  FilePenLine,
  LayoutTemplate,
  Loader2,
  RotateCcw,
  Send,
  SlidersHorizontal,
  Trash2,
  X,
} from "lucide-react";
import type { TFunction } from "i18next";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";

import {
  ADDRESS_INPUT_CLASS,
  AddressRow,
  ComposerBodyEditor,
} from "@/components/mail/composer-body-editor";
import { ComposerTemplateMenu } from "@/components/mail/composer-template-menu";
import { EmailChainTurns } from "@/components/mail/email-chain";
import { parseEmailChain } from "@/components/mail/quoted-body";
import { RECIPIENT_INPUT_ID, RecipientPicker } from "@/components/mail/recipient-picker";
import type { RecipientSelection } from "@/components/mail/recipient-picker-model";
import { MailTemplateEditorDialog } from "@/components/mail/template-editor-dialog";
import { MailTemplateLibraryDialog } from "@/components/mail/template-library";
import {
  templateBannerStatus,
  templateRenderDependsOnCreator,
  templateVariableSlots,
} from "@/components/mail/template-library-model";
import { currentBrowserPlatform } from "@/lib/browser-platform";
import { cn } from "@/lib/utils";

import {
  applyRenderedEmailTemplate,
  beginTemplateRender,
  buildSaveEmailDraftInput,
  cancelTemplateRender,
  composerSendShortcutLabel,
  composerFieldsForPlainText,
  composerInitialFocusId,
  composerViewStateForPlainText,
  createComposerViewState,
  createTemplateRenderGate,
  createEmailComposerFields,
  detachTemplateFields,
  emailComposerCanSave,
  emailComposerCanSend,
  emailComposerExitDisposition,
  emailComposerFieldsFromDraft,
  emailComposerMatchesDraft,
  ensureEmailSendIntent,
  isComposerSendShortcut,
  nonEmptyTemplateVariableOverrides,
  plainTextNeedsConfirmation,
  recipientLocked,
  replySubject,
  resolveInitialEmailTemplate,
  seedEmailSendIntentFromDraft,
  settleTemplateRender,
  templateApplyNeedsConfirmation,
  templateHtmlForSave,
  templateMenuState,
  templateVariableFields,
  viewStateAfterRuntimeDraft,
  viewStateForRenderedTemplate,
  withDerivedBodyHtml,
  type ComposerViewState,
  type EmailComposerFields,
  type EmailSendIntent,
  type EmailSendIntentRegistry,
} from "@/components/mail/composer-model";
import { mailboxCanSend } from "@/components/mail/mail-workspace";
import {
  EMAIL_TEMPLATE_LIST_QUERY_KEY,
  emailDraftKey,
  getEmailDraft,
  invalidateMailboxMailQueries,
  listAllEmailTemplates,
  renderEmailTemplate,
  restoreEmailDraft,
  saveEmailDraft,
  sendEmailDraft,
  trashEmailDraft,
} from "@/components/mail/queries";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
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
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover";
import {
  Sheet,
  SheetClose,
  SheetContent,
  SheetDescription,
  SheetTitle,
} from "@/components/ui/sheet";
import { Switch } from "@/components/ui/switch";
import type {
  EmailDraft,
  EmailDraftSendResult,
  EmailDraftState,
  EmailMessageView,
  EmailTemplateSummary,
  MailboxDetail,
  RenderEmailTemplateInput,
} from "@/lib/creator";

export type EmailComposerLaunch =
  | { key: string; kind: "new"; initialTemplateId?: string }
  | { key: string; kind: "draft"; draftId: string }
  | {
    key: string;
    kind: "reply";
    to: string;
    creatorId: string | null;
    contactRouteId: string | null;
    message: EmailMessageView;
  };

type TemplateRenderOperation = {
  generation: number;
  input: RenderEmailTemplateInput;
};

/** 与 App.tsx 一致：平台只在启动时判定一次，用于快捷键提示文案。 */
const PLATFORM = currentBrowserPlatform();
const FORMAT_TOOLBAR_ID = "mail-compose-format-toolbar";
const BODY_ID = "mail-compose-body";
/** 写信窗口各段落共用的水平留白：窄画布贴边，宽画布留出阅读余量。 */
const INSET = "px-4 @md:px-8 @lg:px-12";

function fieldsForLaunch(launch: EmailComposerLaunch, mailboxId: string): EmailComposerFields {
  if (launch.kind !== "reply") return createEmailComposerFields(mailboxId);
  return createEmailComposerFields(mailboxId, {
    to: launch.to,
    creatorId: launch.creatorId,
    contactRouteId: launch.contactRouteId,
    replyToEmailId: launch.message.id,
    subject: replySubject(launch.message.subject),
  });
}

function launchKindLabel(launch: EmailComposerLaunch | null, t: TFunction<"mail">): string {
  if (launch?.kind === "reply") return t("composer.launch.reply");
  if (launch?.kind === "draft") return t("composer.launch.draft");
  return t("composer.launch.new");
}

function builtinVariableLabel(name: string, t: TFunction<"mail">): string {
  switch (name) {
    case "name": return t("templates.variables.name");
    case "first_name": return t("templates.variables.first_name");
    case "handle": return t("templates.variables.handle");
    case "email": return t("templates.variables.email");
    default: return name;
  }
}

function templateVariablePlaceholder(
  slot: ReturnType<typeof templateVariableSlots>[number],
  t: TFunction<"mail">,
): string {
  if (!slot.source) return slot.name;
  const source = slot.source === "linked"
    ? t("composer.template.variableSource.linked")
    : slot.source === "unlinked"
      ? t("composer.template.variableSource.unlinked")
      : t("composer.template.variableSource.missing");
  return t("composer.template.variablePlaceholder", {
    label: builtinVariableLabel(slot.name, t),
    source,
  });
}

export type MailComposerSheetHandle = {
  requestClose: (continueAfterClose?: () => void) => void;
};

type MailComposerSheetProps = {
  launch: EmailComposerLaunch | null;
  mailbox: MailboxDetail | null;
  /** 模态的 Portal 目标（画布容器），见 lib/canvas-portal.ts。 */
  container?: HTMLElement | null;
  onClose: () => void;
  onSent: (result: EmailDraftSendResult) => void;
};

export const MailComposerSheet = forwardRef<MailComposerSheetHandle, MailComposerSheetProps>(function MailComposerSheet({
  launch,
  mailbox,
  container,
  onClose,
  onSent,
}, ref) {
  const { t } = useTranslation("mail");
  const queryClient = useQueryClient();
  const [fields, setFields] = useState<EmailComposerFields>(() => createEmailComposerFields(mailbox?.mailbox_ref_id ?? ""));
  const [initialFields, setInitialFields] = useState<EmailComposerFields>(() => createEmailComposerFields(mailbox?.mailbox_ref_id ?? ""));
  const [draftState, setDraftState] = useState<EmailDraftState>("draft");
  const [unresolved, setUnresolved] = useState<string[]>([]);
  const [templateDirty, setTemplateDirty] = useState(false);
  /** 上次模板渲染之后又关联了 Creator：横幅提示「重新应用可自动填充变量」，不自动重渲染以免覆盖已改的正文。 */
  const [creatorLinkedSinceRender, setCreatorLinkedSinceRender] = useState(false);
  const [sendConfirmationOpen, setSendConfirmationOpen] = useState(false);
  const [pendingSendDraft, setPendingSendDraft] = useState<EmailDraft | null>(null);
  const [pendingSendIntent, setPendingSendIntent] = useState<EmailSendIntent | null>(null);
  const [persistedDraft, setPersistedDraft] = useState<EmailDraft | null>(null);
  const [refreshingSendState, setRefreshingSendState] = useState(false);
  const sendIntentRegistry = useRef<EmailSendIntentRegistry>(new Map());
  const renderGate = useRef(createTemplateRenderGate());
  const appliedInitialTemplateLaunch = useRef<string | null>(null);
  const [renderingGeneration, setRenderingGeneration] = useState<number | null>(null);
  const [initializedLaunchKey, setInitializedLaunchKey] = useState<string | null>(null);
  const [exitConfirmationOpen, setExitConfirmationOpen] = useState(false);
  const continueAfterClose = useRef<(() => void) | null>(null);

  // 视图状态（格式栏/纯文本/字体字号/模板排版是否原样/tab）。useMutation 的
  // onSuccess 闭包会捕获旧的 view，所以同时维护一个始终最新的 ref，所有写入
  // 都走 updateView 让 ref 与 state 同步。
  const [view, setView] = useState<ComposerViewState>(createComposerViewState);
  const viewRef = useRef(view);
  const updateView = (updater: (current: ComposerViewState) => ComposerViewState) => {
    const next = updater(viewRef.current);
    viewRef.current = next;
    setView(next);
    return next;
  };
  const [plainTextConfirmOpen, setPlainTextConfirmOpen] = useState(false);
  const [templateApplyCandidate, setTemplateApplyCandidate] = useState<EmailTemplateSummary | null>(null);
  const [editorOpen, setEditorOpen] = useState(false);
  const [libraryOpen, setLibraryOpen] = useState(false);
  /**
   * 草稿启动只在首次读到草稿时重置模板状态并把焦点落到正文；之后保存/重读触发的
   * effect 重跑既不抢焦点，也不洗掉「需重新应用 / 未解析」——否则关联 Creator 后点
   * 「保存草稿」会解锁发送，横幅却还提示要重新应用。
   */
  const initializedDraftLaunch = useRef<string | null>(null);

  const draft = useQuery({
    queryKey: emailDraftKey(launch?.kind === "draft" ? launch.draftId : null),
    queryFn: () => getEmailDraft({ draft_id: (launch as Extract<EmailComposerLaunch, { kind: "draft" }>).draftId }),
    enabled: Boolean(launch?.kind === "draft"),
    retry: false,
  });
  const templates = useQuery({
    queryKey: EMAIL_TEMPLATE_LIST_QUERY_KEY,
    queryFn: () => listAllEmailTemplates(),
    enabled: Boolean(launch),
    retry: false,
  });
  const selectedTemplate = useMemo(
    () => (templates.data ?? []).find((template) => template.id === fields.templateId) ?? null,
    [fields.templateId, templates.data],
  );

  const invalidateMailboxMail = async (mailboxId: string, threadId?: string) => {
    await invalidateMailboxMailQueries(queryClient, mailboxId, threadId);
  };

  const applyRuntimeDraft = (latest: EmailDraft) => {
    const recovered = seedEmailSendIntentFromDraft(sendIntentRegistry.current, latest);
    const nextFields = emailComposerFieldsFromDraft(latest);
    sendIntentRegistry.current = recovered.registry;
    setFields(nextFields);
    setInitialFields(nextFields);
    updateView((current) => viewStateAfterRuntimeDraft(current, latest));
    setPersistedDraft(latest);
    setDraftState(latest.state);
    setPendingSendDraft(latest.sending ? latest : null);
    setPendingSendIntent(recovered.intent);
    if (!latest.sending || !recovered.intent) setSendConfirmationOpen(false);
    queryClient.setQueryData(emailDraftKey(latest.id), latest);
    return recovered.intent;
  };

  const refreshSendState = async (draftId: string, announceFailure: boolean) => {
    setRefreshingSendState(true);
    try {
      const latest = await getEmailDraft({ draft_id: draftId });
      const recovered = applyRuntimeDraft(latest);
      await invalidateMailboxMail(latest.mailbox_ref_id);
      return { latest, recovered };
    } catch (error) {
      if (announceFailure) {
        toast.error(t("composer.toasts.readSendStateFailed"), { description: String(error) });
      }
      return null;
    } finally {
      setRefreshingSendState(false);
    }
  };

  const renderTemplate = useMutation({
    mutationFn: ({ input }: TemplateRenderOperation) => renderEmailTemplate(input),
    onSuccess: (rendered, operation) => {
      const settled = settleTemplateRender(renderGate.current, operation.generation);
      renderGate.current = settled.gate;
      if (!settled.accepted) return;
      setRenderingGeneration(null);
      // MCP 写的 HTML 模板：正文未被编辑前原样发送；纯文本模式下不保留排版；
      // 带 data-mail-body 标记的 html 是当初按 Markdown 派生的，按 Markdown 处理。
      const nextView = updateView((current) => viewStateForRenderedTemplate(current, rendered));
      setFields((current) => withDerivedBodyHtml(applyRenderedEmailTemplate(current, rendered), nextView));
      setUnresolved(rendered.unresolved);
      setTemplateDirty(false);
      setCreatorLinkedSinceRender(false);
    },
    onError: (error, operation) => {
      const settled = settleTemplateRender(renderGate.current, operation.generation);
      renderGate.current = settled.gate;
      if (!settled.accepted) return;
      setRenderingGeneration(null);
      toast.error(t("composer.toasts.renderTemplateFailed"), { description: String(error) });
    },
  });
  const saveDraft = useMutation({
    mutationFn: saveEmailDraft,
    onSuccess: async (saved) => {
      applyRuntimeDraft(saved);
      await invalidateMailboxMail(saved.mailbox_ref_id);
    },
    onError: (error) => toast.error(t("composer.toasts.saveDraftFailed"), { description: String(error) }),
  });
  const trashDraft = useMutation({
    mutationFn: trashEmailDraft,
    onSuccess: async (trashed) => {
      setPersistedDraft(trashed);
      toast.success(t("composer.toasts.movedToDeleted"));
      await invalidateMailboxMail(trashed.mailbox_ref_id);
      onClose();
    },
    onError: (error) => toast.error(t("composer.toasts.deleteDraftFailed"), { description: String(error) }),
  });
  const restoreDraft = useMutation({
    mutationFn: restoreEmailDraft,
    onSuccess: async (restored) => {
      applyRuntimeDraft(restored);
      toast.success(t("composer.toasts.restoredDraft"));
      await invalidateMailboxMail(restored.mailbox_ref_id);
    },
    onError: (error) => toast.error(t("composer.toasts.restoreDraftFailed"), { description: String(error) }),
  });
  const sendDraft = useMutation({
    mutationFn: sendEmailDraft,
    onMutate: (input) => {
      if (!pendingSendDraft) return;
      const reserving = {
        ...pendingSendDraft,
        sending: true,
        pending_send_idempotency_key: input.idempotency_key,
      };
      setPersistedDraft(reserving);
      setPendingSendDraft(reserving);
    },
    onSuccess: async (result) => {
      setSendConfirmationOpen(false);
      setPendingSendDraft(null);
      setPendingSendIntent(null);
      setPersistedDraft(null);
      toast.success(t("composer.toasts.sent"), {
        description: result.replayed
          ? t("composer.toasts.replayed")
          : t("composer.toasts.providerMessageId", { id: result.email.provider_message_id }),
      });
      await invalidateMailboxMail(result.thread.mailbox_ref_id, result.thread.id);
      onSent(result);
      onClose();
    },
    onError: async (error) => {
      toast.error(t("composer.toasts.sendFailed"), { description: String(error) });
      const failedDraft = pendingSendDraft;
      if (!failedDraft) return;
      const conservativelyReserved = {
        ...failedDraft,
        sending: true,
        pending_send_idempotency_key: pendingSendIntent?.idempotencyKey
          ?? failedDraft.pending_send_idempotency_key,
      };
      setPersistedDraft(conservativelyReserved);
      setPendingSendDraft(conservativelyReserved);
      await refreshSendState(failedDraft.id, false);
    },
  });

  const abortTemplateRender = () => {
    if (renderGate.current.pendingGeneration === null) return;
    renderGate.current = cancelTemplateRender(renderGate.current);
    setRenderingGeneration(null);
  };
  const startTemplateRender = (input: RenderEmailTemplateInput) => {
    const started = beginTemplateRender(renderGate.current);
    renderGate.current = started.gate;
    setRenderingGeneration(started.generation);
    renderTemplate.mutate({ generation: started.generation, input });
  };
  useEffect(() => {
    abortTemplateRender();
    appliedInitialTemplateLaunch.current = null;
    initializedDraftLaunch.current = null;
    setInitializedLaunchKey(null);
    setUnresolved([]);
    setTemplateDirty(false);
    setCreatorLinkedSinceRender(false);
    setPendingSendDraft(null);
    setPendingSendIntent(null);
    setSendConfirmationOpen(false);
    updateView(() => createComposerViewState());
    if (!launch || !mailbox) return;
    const nextFields = launch.kind === "draft"
      ? createEmailComposerFields(mailbox.mailbox_ref_id)
      : fieldsForLaunch(launch, mailbox.mailbox_ref_id);
    setFields(nextFields);
    setInitialFields(nextFields);
    setPersistedDraft(null);
    setDraftState("draft");
    setInitializedLaunchKey(launch.key);
  }, [launch?.key, mailbox?.mailbox_ref_id]);
  useEffect(() => {
    if (launch?.kind !== "draft" || !draft.data) return;
    applyRuntimeDraft(draft.data);
    if (initializedDraftLaunch.current === launch.key) return;
    initializedDraftLaunch.current = launch.key;
    setUnresolved([]);
    setTemplateDirty(false);
    setCreatorLinkedSinceRender(false);
    // 草稿读完之前正文区还是加载态，此时再把焦点落到正文。
    requestAnimationFrame(() => document.getElementById(BODY_ID)?.focus());
  }, [draft.data, launch?.kind, launch?.key]);

  const chooseTemplate = (template: EmailTemplateSummary) => {
    const variables = templateVariableFields(template, fields.templateVariables);
    setFields((current) => ({ ...current, templateId: template.id, templateVariables: variables }));
    setUnresolved([]);
    setTemplateDirty(true);
    startTemplateRender({
      template_id: template.id,
      creator_id: fields.creatorId,
      variables: nonEmptyTemplateVariableOverrides(variables),
    });
  };
  useEffect(() => {
    const template = resolveInitialEmailTemplate(
      launch?.kind === "new" ? launch : null,
      initializedLaunchKey,
      appliedInitialTemplateLaunch.current,
      templates.data,
    );
    if (!template || !launch) return;
    appliedInitialTemplateLaunch.current = launch.key;
    chooseTemplate(template);
  }, [initializedLaunchKey, launch, templates.data]);
  const applyTemplate = () => {
    if (!selectedTemplate) return;
    setTemplateDirty(true);
    startTemplateRender({
      template_id: selectedTemplate.id,
      creator_id: fields.creatorId,
      variables: nonEmptyTemplateVariableOverrides(fields.templateVariables),
    });
  };
  /** 用户主动选择模板（菜单 / 模板库 / 覆盖确认）：应用并提示；启动路径的 initialTemplateId 不走这里。 */
  const applyTemplateFromUser = (template: EmailTemplateSummary) => {
    chooseTemplate(template);
    toast.success(t("composer.toasts.appliedTemplate", { name: template.name }));
  };
  const requestApplyTemplate = (template: EmailTemplateSummary) => {
    if (templateApplyNeedsConfirmation(fields)) {
      setTemplateApplyCandidate(template);
      return;
    }
    applyTemplateFromUser(template);
  };
  const detachTemplate = () => {
    abortTemplateRender();
    // 模板 html 随模板一起退出：否则编辑区显示 Markdown、预览/发送却还是模板 html。
    const nextView = updateView((current) => ({ ...current, templateHtmlIntact: false }));
    setFields((current) => detachTemplateFields(current, nextView));
    setUnresolved([]);
    setTemplateDirty(false);
  };
  const adoptSavedTemplate = (saved: EmailTemplateSummary) => {
    const variables = templateVariableFields(saved, fields.templateVariables);
    setFields((current) => ({
      ...current,
      templateId: saved.id,
      templateVariables: variables,
    }));
    setUnresolved([]);
    setTemplateDirty(true);
    startTemplateRender({
      template_id: saved.id,
      creator_id: fields.creatorId,
      variables: nonEmptyTemplateVariableOverrides(variables),
    });
  };
  const persistDraft = async () => {
    const saved = await saveDraft.mutateAsync(buildSaveEmailDraftInput(fields));
    toast.success(t("composer.toasts.savedDraft"));
    return saved;
  };
  const requestSend = async () => {
    if (
      !mailboxCanSend(mailbox)
      || !emailComposerCanSend(fields)
      || unresolved.length
      || templateDirty
      || renderingGeneration !== null
    ) return;
    try {
      const sendable = persistedDraft && emailComposerMatchesDraft(fields, persistedDraft)
        ? persistedDraft
        : await saveDraft.mutateAsync(buildSaveEmailDraftInput(fields));
      const ensured = ensureEmailSendIntent(sendIntentRegistry.current, sendable, () => crypto.randomUUID());
      sendIntentRegistry.current = ensured.registry;
      setPendingSendDraft(sendable);
      setPendingSendIntent(ensured.intent);
      setSendConfirmationOpen(true);
    } catch {
      // The mutation already surfaces the runtime error and keeps the sheet open.
    }
  };
  const templateRenderPending = renderingGeneration !== null;
  const busy = saveDraft.isPending
    || sendDraft.isPending
    || trashDraft.isPending
    || restoreDraft.isPending
    || refreshingSendState
    || templateRenderPending;
  const sending = Boolean(persistedDraft?.sending);
  const readOnly = draftState === "trashed" || sending;
  const pendingSendReady = Boolean(
    pendingSendDraft
    && pendingSendIntent
    && pendingSendIntent.draftId === pendingSendDraft.id
    && pendingSendIntent.revision === pendingSendDraft.revision
    && (!pendingSendDraft.sending
      || pendingSendDraft.pending_send_idempotency_key === pendingSendIntent.idempotencyKey),
  );
  const exitDisposition = emailComposerExitDisposition({
    fields,
    initialFields,
    persistedDraft,
    sending,
  });

  // ---- 正文编辑 -------------------------------------------------------------
  /** 第一次编辑正文（或改字体字号）后模板 html 不再原样保留，改为按 Markdown 派生；只提示一次。 */
  const leaveTemplateHtml = () => {
    if (!viewRef.current.templateHtmlIntact) return viewRef.current;
    const nextView = updateView((current) => ({ ...current, templateHtmlIntact: false }));
    toast.info(t("composer.toasts.markdownHtml"));
    return nextView;
  };
  const handleBodyChange = (text: string) => {
    abortTemplateRender();
    const nextView = leaveTemplateHtml();
    setFields((current) => withDerivedBodyHtml({ ...current, text }, nextView));
  };
  const changeBodyStyle = (bodyStyle: ComposerViewState["bodyStyle"]) => {
    // 字体字号包在派生 html 的外层 div 上：模板 html 原样保留时改了也不会生效，视同编辑正文。
    leaveTemplateHtml();
    const nextView = updateView((current) => ({ ...current, bodyStyle }));
    setFields((current) => withDerivedBodyHtml(current, nextView));
  };
  const handleSendShortcut = (event: KeyboardEvent<HTMLElement>) => {
    if (!isComposerSendShortcut({
      key: event.key,
      metaKey: event.metaKey,
      ctrlKey: event.ctrlKey,
      shiftKey: event.shiftKey,
      altKey: event.altKey,
      isComposing: event.nativeEvent.isComposing,
    })) return;
    event.preventDefault();
    // 与「发送邮件」按钮的 disabled 一致：保存/发送进行中不再触发，避免并发保存撞 revision。
    if (busy || readOnly) return;
    void requestSend();
  };

  // ---- 收件人 ---------------------------------------------------------------
  const handleToChange = (to: string) => {
    setFields((current) => ({ ...current, to }));
  };
  /**
   * 选中 Creator：to / creatorId / contactRouteId 一起落下（runtime 要求二者同时有或同时无）。
   * 模板用到内置变量时只标记需要重新应用：进行中的渲染用的是旧 creator_id，作废；不自动
   * 重渲染。模板不依赖 Creator（无变量或只有自定义变量）时渲染结果不变，不置 dirty——
   * 「重新应用」按钮只在有变量时出现，否则发送会被锁死。
   * 收件人输入框随 chip 出现而卸载，焦点会掉到 body，回聚焦到主题（与草稿启动聚焦同一约定）。
   */
  const handleCreatorSelect = (selection: RecipientSelection) => {
    setFields((current) => ({
      ...current,
      to: selection.to,
      creatorId: selection.creatorId,
      contactRouteId: selection.contactRouteId,
    }));
    requestAnimationFrame(() => document.getElementById("mail-compose-subject")?.focus());
    if (!selectedTemplate || !templateRenderDependsOnCreator(selectedTemplate.variables)) return;
    abortTemplateRender();
    setCreatorLinkedSinceRender(true);
    setTemplateDirty(true);
  };
  /** 解除关联：保留 to 文本，仍可手动发送；模板内置变量不再自动填，需重新应用。焦点回到收件人输入框。 */
  const handleCreatorClear = () => {
    setFields((current) => ({ ...current, creatorId: null, contactRouteId: null }));
    requestAnimationFrame(() => document.getElementById(RECIPIENT_INPUT_ID)?.focus());
    if (!selectedTemplate || !templateRenderDependsOnCreator(selectedTemplate.variables)) return;
    abortTemplateRender();
    setCreatorLinkedSinceRender(false);
    setTemplateDirty(true);
  };

  // ---- 纯文本模式 -----------------------------------------------------------
  const enablePlainText = () => {
    updateView((current) => composerViewStateForPlainText(current, true));
    setFields(composerFieldsForPlainText);
    setPlainTextConfirmOpen(false);
  };
  const togglePlainText = (enabled: boolean) => {
    if (!enabled) {
      const nextView = updateView((current) => composerViewStateForPlainText(current, false));
      setFields((current) => withDerivedBodyHtml(current, nextView));
      return;
    }
    if (plainTextNeedsConfirmation(fields, viewRef.current)) {
      setPlainTextConfirmOpen(true);
      return;
    }
    enablePlainText();
  };

  // ---- 关闭 -----------------------------------------------------------------
  const finishComposerClose = (continueNavigation?: () => void) => {
    abortTemplateRender();
    setExitConfirmationOpen(false);
    setSendConfirmationOpen(false);
    setPlainTextConfirmOpen(false);
    setTemplateApplyCandidate(null);
    setEditorOpen(false);
    setLibraryOpen(false);
    continueAfterClose.current = null;
    onClose();
    continueNavigation?.();
  };
  const requestComposerClose = (continueNavigation?: () => void) => {
    if (!launch) {
      continueNavigation?.();
      return;
    }
    if (exitDisposition === "close") {
      finishComposerClose(continueNavigation);
      return;
    }
    continueAfterClose.current = continueNavigation ?? null;
    setExitConfirmationOpen(true);
  };
  const continueEditing = () => {
    continueAfterClose.current = null;
    setExitConfirmationOpen(false);
  };
  const discardAndClose = () => {
    const continueNavigation = continueAfterClose.current ?? undefined;
    finishComposerClose(continueNavigation);
  };
  const saveAndClose = async () => {
    try {
      await persistDraft();
      const continueNavigation = continueAfterClose.current ?? undefined;
      finishComposerClose(continueNavigation);
    } catch {
      // saveDraft already displays the runtime error; keep both dialogs open.
    }
  };

  useImperativeHandle(ref, () => ({ requestClose: requestComposerClose }), [
    exitDisposition,
    launch,
    fields,
    persistedDraft,
  ]);
  useEffect(() => {
    if (!launch || exitDisposition !== "confirm") return;
    const preventUnsavedUnload = (event: BeforeUnloadEvent) => {
      event.preventDefault();
      event.returnValue = "";
    };
    window.addEventListener("beforeunload", preventUnsavedUnload);
    return () => window.removeEventListener("beforeunload", preventUnsavedUnload);
  }, [exitDisposition, launch]);

  const showAlerts = !mailboxCanSend(mailbox) || draftState === "trashed" || sending;
  const bannerStatus = templateBannerStatus({
    unresolvedCount: unresolved.length,
    dirty: templateDirty,
    creatorLinked: creatorLinkedSinceRender,
  });
  const variableSlots = templateVariableSlots({
    variables: selectedTemplate?.variables ?? [],
    unresolved,
    hasCreator: Boolean(fields.creatorId),
  });
  const senderName = mailbox?.display_name ?? mailbox?.label ?? null;
  // 字体/字号是整封邮件一个设置：编辑区跟着显示，让「所见」接近派生 html。
  const bodyEditorStyle: CSSProperties | undefined = view.plainText
    ? undefined
    : {
      fontFamily: view.bodyStyle.fontFamily ?? undefined,
      fontSize: view.bodyStyle.fontSize === null ? undefined : `${view.bodyStyle.fontSize}px`,
    };

  return (
    <>
      <Sheet open={Boolean(launch)} onOpenChange={(open) => !open && requestComposerClose()}>
        <SheetContent
          side="right"
          showCloseButton={false}
          container={container}
          // 宽度相对画布而不是视口：SheetContent 传送进 App 的画布（transform 容器），
          // fixed 的包含块是画布；画布可能比视口窄，用 vw 会把抽屉左缘裁掉。
          className="w-[min(760px,100%)] max-w-full gap-0 p-0 sm:max-w-[760px] data-[state=open]:duration-200 @container"
          onOpenAutoFocus={(event) => {
            event.preventDefault();
            // 首个 commit 里 fields 还是上一封的残留（launch effect 晚于 FocusScope 触发），
            // 所以按本次启动的初始字段决定焦点；草稿启动在读到草稿后再聚焦。
            if (!launch || launch.kind === "draft") return;
            const initial = fieldsForLaunch(launch, mailbox?.mailbox_ref_id ?? "");
            document.getElementById(composerInitialFocusId(launch.kind, initial))?.focus();
          }}
        >
          {/* macOS 红绿灯落在这条空白上；其他平台高度为 0。 */}
          <div className="flex h-12 shrink-0 items-center gap-2 border-b px-4">
            <div className="size-8 shrink-0" aria-hidden="true" />
            <SheetTitle className="min-w-0 flex-1 truncate text-center text-sm">
              {fields.subject.trim() || launchKindLabel(launch, t)}
            </SheetTitle>
            <SheetDescription className="sr-only">
              {t("composer.sendingFrom", { mailbox: mailbox?.email_address ?? t("common.noMailboxSelected") })}
            </SheetDescription>
            <SheetClose asChild>
              <Button variant="ghost" size="icon-sm" aria-label={t("composer.close")}>
                <X />
              </Button>
            </SheetClose>
          </div>

          {showAlerts && (
            <div className={cn("shrink-0 space-y-3 pt-3", INSET)}>
              {!mailboxCanSend(mailbox) && (
                <Alert variant="destructive">
                  <AlertTitle>{t("composer.alerts.mailboxTitle")}</AlertTitle>
                  <AlertDescription>{t("composer.alerts.mailboxDescription")}</AlertDescription>
                </Alert>
              )}
              {draftState === "trashed" && (
                <Alert>
                  <AlertTitle>{t("composer.alerts.deletedTitle")}</AlertTitle>
                  <AlertDescription>{t("composer.alerts.deletedDescription")}</AlertDescription>
                </Alert>
              )}
              {sending && (
                <Alert variant="warning">
                  <AlertTitle>{t("composer.alerts.sendingTitle")}</AlertTitle>
                  <AlertDescription>
                    {pendingSendReady
                      ? t("composer.alerts.sendingReady")
                      : t("composer.alerts.sendingRefresh")}
                  </AlertDescription>
                </Alert>
              )}
            </div>
          )}

          {launch?.kind === "draft" && draft.isLoading ? (
            <div className="grid min-h-0 flex-1 place-items-center"><Loader2 className="animate-spin text-muted-foreground" /></div>
          ) : draft.error ? (
            <div className={cn("py-6 text-sm text-destructive", INSET)}>
              {t("composer.errors.draft", { error: String(draft.error) })}
            </div>
          ) : (
            <>
              <div className={cn("shrink-0", INSET)}>
                <AddressRow label={t("common.sender")}>
                  <span className="flex min-w-0 items-baseline gap-1.5 text-base">
                    {mailbox ? (
                      <>
                        {senderName && <span className="truncate">{senderName}</span>}
                        <span className="truncate text-muted-foreground">{`<${mailbox.email_address}>`}</span>
                      </>
                    ) : (
                      <span className="text-muted-foreground">{t("common.noMailboxSelected")}</span>
                    )}
                  </span>
                </AddressRow>
                {/* 已关联 Creator 时这一行是 chip（role=group 自带名称），没有可 htmlFor 的输入框。 */}
                <AddressRow label={t("common.recipient")} htmlFor={fields.creatorId ? undefined : RECIPIENT_INPUT_ID}>
                  <RecipientPicker
                    to={fields.to}
                    creatorId={fields.creatorId}
                    contactRouteId={fields.contactRouteId}
                    locked={recipientLocked(launch?.kind ?? null, fields)}
                    disabled={readOnly}
                    lockedPlaceholder={fields.replyToEmailId && !fields.creatorId
                      ? t("composer.recipientLocked")
                      : undefined}
                    container={container}
                    onToChange={handleToChange}
                    onCreatorSelect={handleCreatorSelect}
                    onCreatorClear={handleCreatorClear}
                    onKeyDown={handleSendShortcut}
                  />
                </AddressRow>
                <AddressRow label={t("common.subject")} htmlFor="mail-compose-subject">
                  <Input
                    id="mail-compose-subject"
                    value={fields.subject}
                    disabled={readOnly}
                    placeholder={t("composer.subjectPlaceholder")}
                    className={ADDRESS_INPUT_CLASS}
                    onKeyDown={handleSendShortcut}
                    onChange={(event) => {
                      abortTemplateRender();
                      setFields((current) => ({ ...current, subject: event.target.value }));
                    }}
                  />
                </AddressRow>
              </div>

              <ComposerBodyEditor
                id={BODY_ID}
                formatToolbarId={FORMAT_TOOLBAR_ID}
                inset={INSET}
                container={container}
                value={fields.text}
                onChange={handleBodyChange}
                html={fields.html}
                disabled={readOnly}
                plainText={view.plainText}
                placeholder={t(
                  view.plainText ? "composer.plainBodyPlaceholder" : "composer.bodyPlaceholder",
                  { shortcut: composerSendShortcutLabel(PLATFORM) },
                )}
                formatExpanded={view.formatExpanded}
                onFormatExpandedChange={(formatExpanded) => updateView((current) => ({ ...current, formatExpanded }))}
                tab={view.tab}
                onTabChange={(tab) => updateView((current) => ({ ...current, tab }))}
                bodyStyle={view.bodyStyle}
                onBodyStyleChange={changeBodyStyle}
                editorStyle={bodyEditorStyle}
                onKeyDown={handleSendShortcut}
                actionBarExtras={(
                  <>
                    <ComposerTemplateMenu
                      state={templateMenuState(templates)}
                      activeTemplateId={fields.templateId}
                      disabled={readOnly}
                      saveDisabled={templateRenderPending}
                      onApply={requestApplyTemplate}
                      onSaveAsTemplate={() => setEditorOpen(true)}
                      onManage={() => setLibraryOpen(true)}
                    />
                    <Popover>
                      <PopoverTrigger asChild>
                        <Button
                          type="button"
                          variant="ghost"
                          size="sm"
                          aria-label={t("composer.advanced")}
                          className="data-[state=open]:bg-muted"
                        >
                          <SlidersHorizontal aria-hidden="true" />
                          <span className="@max-xl:sr-only">{t("composer.advanced")}</span>
                        </Button>
                      </PopoverTrigger>
                      <PopoverContent align="start" container={container} className="w-56 p-3">
                        <div className="flex items-center justify-between gap-3">
                          <Label htmlFor="mail-compose-plain-text">{t("composer.plainTextMode")}</Label>
                          <Switch
                            id="mail-compose-plain-text"
                            checked={view.plainText}
                            disabled={readOnly}
                            onCheckedChange={togglePlainText}
                          />
                        </div>
                      </PopoverContent>
                    </Popover>
                  </>
                )}
                aboveBody={(selectedTemplate || bannerStatus) && (
                  <div className={cn("shrink-0 pt-1.5", INSET)}>
                    <div className="rounded-lg bg-muted/40 px-3 py-1.5 text-xs">
                      <div className="flex items-center gap-2">
                        <LayoutTemplate aria-hidden="true" className="size-3.5 shrink-0 text-muted-foreground" />
                        {/* 模板被外部（MCP）删除后列表里查不到名字，但未解析/已修改状态仍会禁用发送：
                            横幅退化为无名标题，保留状态小字与「取消使用模板」让用户能解除。 */}
                        <span className="min-w-0 truncate">
                          {selectedTemplate
                            ? t("composer.appliedTemplateNamed", { name: selectedTemplate.name })
                            : t("composer.appliedTemplate")}
                        </span>
                        {bannerStatus && (
                          <span className={cn(
                            "shrink-0",
                            bannerStatus.kind === "unresolved" ? "text-destructive" : "text-muted-foreground",
                          )}>
                            {bannerStatus.kind === "unresolved"
                              ? t("composer.template.status.unresolved", { count: bannerStatus.count })
                              : bannerStatus.kind === "creator-linked"
                                ? t("composer.template.status.creatorLinked")
                                : t("composer.template.status.dirty")}
                          </span>
                        )}
                        {view.templateHtmlIntact && (
                          <span className="min-w-0 truncate text-muted-foreground">{t("composer.template.htmlIntactHint")}</span>
                        )}
                        <Button
                          type="button"
                          variant="ghost"
                          size="icon-xs"
                          aria-label={t("composer.detachTemplate")}
                          disabled={readOnly}
                          className="ml-auto"
                          onClick={detachTemplate}
                        >
                          <X />
                        </Button>
                      </div>
                      {selectedTemplate && selectedTemplate.variables.length > 0 && (
                        <div className="mt-1.5 flex flex-wrap items-center gap-2">
                          {variableSlots.map((slot) => (
                            <Input
                              key={slot.name}
                              id={`mail-template-${slot.name}`}
                              aria-label={slot.name}
                              aria-invalid={slot.unresolved}
                              placeholder={templateVariablePlaceholder(slot, t)}
                              className="h-8 w-44 text-sm"
                              value={fields.templateVariables[slot.name] ?? ""}
                              disabled={readOnly}
                              onChange={(event) => {
                                const value = event.target.value;
                                abortTemplateRender();
                                setFields((current) => ({
                                  ...current,
                                  templateVariables: { ...current.templateVariables, [slot.name]: value },
                                }));
                                setTemplateDirty(true);
                              }}
                            />
                          ))}
                          <Button variant="secondary" size="sm" disabled={readOnly || templateRenderPending} onClick={applyTemplate}>
                            {templateRenderPending ? <Loader2 className="animate-spin" /> : <RotateCcw />}{t("composer.reapplyTemplate")}
                          </Button>
                        </div>
                      )}
                      {selectedTemplate && !fields.creatorId && templateRenderDependsOnCreator(selectedTemplate.variables) && (
                        <p className="mt-1 text-xs text-muted-foreground">{t("composer.template.manualRecipientHint")}</p>
                      )}
                    </div>
                  </div>
                )}
                belowBody={launch?.kind === "reply" && <ReplyHistory message={launch.message} className="shrink-0" inset="px-0" />}
              />

              <div className={cn("flex h-14 shrink-0 items-center gap-2 border-t", INSET)}>
                {fields.draftId && draftState === "draft" && (
                  <Button
                    variant="ghost"
                    size="icon"
                    aria-label={t("composer.moveToDeleted")}
                    disabled={busy || sending}
                    onClick={() => trashDraft.mutate({ draft_id: fields.draftId!, expected_revision: fields.expectedRevision! })}
                  >
                    {trashDraft.isPending ? <Loader2 className="animate-spin" /> : <Trash2 />}
                  </Button>
                )}
                {fields.draftId && draftState === "trashed" && (
                  <Button
                    variant="outline"
                    size="sm"
                    disabled={busy || sending}
                    onClick={() => restoreDraft.mutate({ draft_id: fields.draftId!, expected_revision: fields.expectedRevision! })}
                  >
                    <RotateCcw />{t("composer.restoreDraft")}
                  </Button>
                )}
                {!fields.draftId && (
                  <Button variant="ghost" size="icon" aria-label={t("composer.discard")} onClick={() => requestComposerClose()}>
                    <Trash2 />
                  </Button>
                )}
                <div className="ml-auto flex items-center gap-2">
                  {sending && pendingSendReady && (
                    <Button variant="outline" disabled={sendDraft.isPending} onClick={() => setSendConfirmationOpen(true)}>
                      <RotateCcw />{t("composer.restoreConfirmation")}
                    </Button>
                  )}
                  {sending && !pendingSendReady && pendingSendDraft && (
                    <Button
                      variant="outline"
                      disabled={refreshingSendState}
                      onClick={() => void refreshSendState(pendingSendDraft.id, true)}
                    >
                      {refreshingSendState ? <Loader2 className="animate-spin" /> : <RotateCcw />}
                      {t("composer.refreshSendState")}
                    </Button>
                  )}
                  {!readOnly && (
                    <Button
                      variant="outline"
                      disabled={!emailComposerCanSave(fields) || busy}
                      onClick={() => void persistDraft()}
                    >
                      {saveDraft.isPending ? <Loader2 className="animate-spin" /> : <FilePenLine />}{t("composer.saveDraft")}
                    </Button>
                  )}
                  {!readOnly && (
                    <Button
                      disabled={!mailboxCanSend(mailbox) || !emailComposerCanSend(fields) || unresolved.length > 0 || templateDirty || busy}
                      onClick={() => void requestSend()}
                    >
                      {saveDraft.isPending ? <Loader2 className="animate-spin" /> : <Send />}{t("composer.send")}
                    </Button>
                  )}
                </div>
              </div>
            </>
          )}
        </SheetContent>
      </Sheet>

      <Dialog
        open={exitConfirmationOpen}
        onOpenChange={(open) => !open && !saveDraft.isPending && continueEditing()}
      >
        <DialogContent showCloseButton={false} container={container}>
          <DialogHeader>
            <DialogTitle>{t("composer.exit.title")}</DialogTitle>
            <DialogDescription>
              {t("composer.exit.description")}
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button variant="outline" disabled={saveDraft.isPending} onClick={continueEditing}>
              {t("composer.exit.continue")}
            </Button>
            <Button variant="destructive" disabled={saveDraft.isPending} onClick={discardAndClose}>
              {t("composer.exit.discard")}
            </Button>
            <Button
              disabled={
                !emailComposerCanSave(fields)
                || draftState !== "draft"
                || busy
                || sending
              }
              onClick={() => void saveAndClose()}
            >
              {saveDraft.isPending ? <Loader2 className="animate-spin" /> : <FilePenLine />}
              {t("composer.exit.saveAndClose")}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      <Dialog open={sendConfirmationOpen} onOpenChange={setSendConfirmationOpen}>
        <DialogContent container={container}>
          <DialogHeader>
            <DialogTitle>{t("composer.confirm.title")}</DialogTitle>
            <DialogDescription>
              {pendingSendDraft?.sending
                ? t("composer.confirm.resumeDescription")
                : t("composer.confirm.description", {
                    mailbox: mailbox?.email_address ?? t("common.selectMailbox"),
                    recipient: pendingSendDraft?.to ?? t("common.recipient"),
                    subject: pendingSendDraft?.subject ?? "",
                  })}
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button variant="outline" disabled={sendDraft.isPending} onClick={() => setSendConfirmationOpen(false)}>{t("common.cancel")}</Button>
            <Button
              disabled={!pendingSendReady || sendDraft.isPending}
              onClick={() => pendingSendReady && pendingSendDraft && pendingSendIntent && sendDraft.mutate({
                draft_id: pendingSendDraft.id,
                expected_revision: pendingSendDraft.revision,
                idempotency_key: pendingSendIntent.idempotencyKey,
              })}
            >
              {sendDraft.isPending ? <Loader2 className="animate-spin" /> : <Send />}{t("composer.confirm.action")}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      <Dialog open={plainTextConfirmOpen} onOpenChange={setPlainTextConfirmOpen}>
        <DialogContent container={container}>
          <DialogHeader>
            <DialogTitle>{t("composer.plainTextConfirm.title")}</DialogTitle>
            <DialogDescription>{t("composer.plainTextConfirm.description")}</DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button variant="outline" onClick={() => setPlainTextConfirmOpen(false)}>{t("common.cancel")}</Button>
            <Button onClick={enablePlainText}>{t("composer.plainTextConfirm.action")}</Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      <Dialog
        open={templateApplyCandidate !== null}
        onOpenChange={(open) => !open && setTemplateApplyCandidate(null)}
      >
        <DialogContent container={container}>
          <DialogHeader>
            <DialogTitle>{t("composer.templateConfirm.title")}</DialogTitle>
            <DialogDescription>
              {t("composer.templateConfirm.description", { name: templateApplyCandidate?.name ?? "" })}
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button variant="outline" onClick={() => setTemplateApplyCandidate(null)}>{t("common.cancel")}</Button>
            <Button
              onClick={() => {
                if (!templateApplyCandidate) return;
                const template = templateApplyCandidate;
                setTemplateApplyCandidate(null);
                applyTemplateFromUser(template);
              }}
            >
              {t("composer.templateConfirm.action")}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      <MailTemplateEditorDialog
        open={editorOpen}
        onOpenChange={setEditorOpen}
        mode="create"
        initialFields={{ name: "", subject: fields.subject, text: fields.text, html: templateHtmlForSave(fields, view) }}
        existing={null}
        container={container}
        saveDisabled={templateRenderPending}
        onSaved={adoptSavedTemplate}
      />

      <MailTemplateLibraryDialog
        open={libraryOpen}
        onOpenChange={setLibraryOpen}
        canCompose={!readOnly && mailboxCanSend(mailbox)}
        container={container}
        onUseTemplate={(templateId) => {
          setLibraryOpen(false);
          const template = (templates.data ?? []).find((item) => item.id === templateId);
          if (template) requestApplyTemplate(template);
        }}
        onDeleted={(templateId) => {
          if (templateId === fields.templateId) detachTemplate();
        }}
        onSaved={(saved) => {
          // 模板改了声明的变量集：沿用 adoptSavedTemplate 裁掉已删除变量的覆盖值再重渲染。
          if (saved.id === fields.templateId) adoptSavedTemplate(saved);
        }}
      />
    </>
  );
});

/**
 * 回复时的历史邮件区,飞书那种形态:可编辑的正文在上,历史在下,默认
 * 折叠,而且**每一轮各自一张抬头卡加正文**。
 *
 * 关键是别把父邮件的正文原样倒出来。邮件回复是累积的,越往下 `>` 越
 * 深,原样渲染读者看到的是一堵符号墙;`parseEmailChain` 把它还原成
 * 一轮轮「谁在什么时候说了什么」,这里只负责画。
 *
 * 只读:引文不是作者写的内容,放进可编辑区里迟早被误删一半。真正的
 * 拼接发生在发送时(两条回复路径唯一共用的那个点),发出去的是纯文本
 * 与 html 两份等价表达;这里画的是同一段历史,只是用了收件人客户端
 * 折叠之后会看到的那种形态。
 */
function ReplyHistory({
  message,
  className,
  inset = "px-3",
}: {
  message: EmailMessageView;
  className?: string;
  /** 水平内边距跟随容器:撰写区贴着编辑框,预览跟着弹窗的留白。 */
  inset?: string;
}) {
  const { t } = useTranslation("mail");
  const [open, setOpen] = useState(false);
  const turns = useMemo(() => parseEmailChain(message.text), [message.text]);
  if (!(message.text ?? "").trim()) return null;
  return (
    <div className={cn("border-t", className)}>
      <button
        type="button"
        aria-expanded={open}
        onClick={() => setOpen((value) => !value)}
        className={cn(
          "flex w-full items-center gap-1.5 py-2 text-left text-xs text-muted-foreground transition-colors hover:bg-muted/40",
          inset,
        )}
      >
        <ChevronRight className={cn("size-3.5 transition-transform", open && "rotate-90")} />
        {open ? t("message.hideHistory") : t("message.showHistory")}
      </button>
      {open && (
        <div className={cn("max-h-72 space-y-3 overflow-y-auto pb-3", inset)}>
          <EmailChainTurns turns={turns} first={message} />
        </div>
      )}
    </div>
  );
}
