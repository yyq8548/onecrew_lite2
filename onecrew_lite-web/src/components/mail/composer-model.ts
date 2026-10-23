import type {
  EmailDraft,
  EmailTemplateSummary,
  RenderedEmailTemplate,
  SaveEmailDraftInput,
  SaveEmailTemplateInput,
} from "@/lib/creator";
import type { BrowserPlatform } from "@/lib/browser-platform";

import {
  DEFAULT_EMAIL_BODY_STYLE,
  type EmailBodyStyle,
  hasRichMarkdown,
  isDerivedEmailHtml,
  markdownToEmailHtml,
  readEmailBodyStyle,
  stripMarkdownForPlainText,
} from "@/components/mail/markdown-email";

export type EmailComposerFields = {
  draftId: string | null;
  expectedRevision: number | null;
  mailboxRefId: string;
  to: string;
  creatorId: string | null;
  contactRouteId: string | null;
  replyToEmailId: string | null;
  subject: string;
  text: string;
  html: string | null;
  templateId: string | null;
  templateVariables: Record<string, string>;
};

export type EmailSendIntent = {
  identity: string;
  draftId: string;
  revision: number;
  idempotencyKey: string;
};

export type EmailSendIntentRegistry = ReadonlyMap<string, EmailSendIntent>;

export type TemplateRenderGate = {
  generation: number;
  pendingGeneration: number | null;
};

export type EmailComposerExitDisposition = "close" | "confirm";

export type InitialEmailTemplateLaunch = {
  key: string;
  initialTemplateId?: string | null;
};

export function createEmailComposerFields(
  mailboxRefId: string,
  overrides: Partial<EmailComposerFields> = {},
): EmailComposerFields {
  return {
    draftId: null,
    expectedRevision: null,
    mailboxRefId,
    to: "",
    creatorId: null,
    contactRouteId: null,
    replyToEmailId: null,
    subject: "",
    text: "",
    html: null,
    templateId: null,
    templateVariables: {},
    ...overrides,
  };
}

export function emailComposerFieldsFromDraft(draft: EmailDraft): EmailComposerFields {
  return {
    draftId: draft.id,
    expectedRevision: draft.revision,
    mailboxRefId: draft.mailbox_ref_id,
    to: draft.to,
    creatorId: draft.creator_id ?? null,
    contactRouteId: draft.contact_route_id ?? null,
    replyToEmailId: draft.reply_to_email_id ?? null,
    subject: draft.subject,
    text: draft.text,
    html: draft.html ?? null,
    templateId: draft.template_id ?? null,
    templateVariables: { ...draft.template_variables },
  };
}

export function buildSaveEmailDraftInput(fields: EmailComposerFields): SaveEmailDraftInput {
  return {
    id: fields.draftId,
    mailbox_ref_id: fields.mailboxRefId,
    to: fields.to.trim(),
    creator_id: fields.creatorId,
    contact_route_id: fields.contactRouteId,
    reply_to_email_id: fields.replyToEmailId,
    subject: fields.subject.trim(),
    text: fields.text.trim(),
    html: fields.html,
    template_id: fields.templateId,
    template_variables: nonEmptyTemplateVariableOverrides(fields.templateVariables),
    expected_revision: fields.expectedRevision,
  };
}

/**
 * Html worth storing on a template: only html that really came from a template
 * render and has not been edited (`templateHtmlIntact`). Markdown-derived html
 * is a snapshot of the body — it would freeze the template (runtime returns
 * template html verbatim and rejects `{{var}}` inside it), so it is dropped and
 * re-derived from `text` whenever the template is applied.
 */
export function templateHtmlForSave(fields: EmailComposerFields, view: ComposerViewState): string | null {
  return view.templateHtmlIntact && !isDerivedEmailHtml(fields.html) ? fields.html : null;
}

export function buildSaveEmailTemplateInput(
  fields: EmailComposerFields,
  view: ComposerViewState,
  name: string,
  existing: { id: string; revision: number } | null,
): SaveEmailTemplateInput {
  return {
    id: existing?.id ?? null,
    name: name.trim(),
    subject: fields.subject,
    text: fields.text,
    html: templateHtmlForSave(fields, view),
    expected_revision: existing?.revision ?? null,
  };
}

export function templateVariableFields(
  template: EmailTemplateSummary | null | undefined,
  current: Record<string, string>,
): Record<string, string> {
  if (!template) return {};
  const declared = new Set(template.variables);
  return Object.fromEntries(
    Object.entries(nonEmptyTemplateVariableOverrides(current))
      .filter(([variable]) => declared.has(variable)),
  );
}

/** Empty values are omissions, not overrides: runtime built-ins may still resolve them. */
export function nonEmptyTemplateVariableOverrides(
  values: Record<string, string>,
): Record<string, string> {
  return Object.fromEntries(
    Object.entries(values)
      .filter(([, value]) => value.trim().length > 0)
      .sort(([left], [right]) => left.localeCompare(right)),
  );
}

function normalizedDraftContent(draft: EmailDraft | SaveEmailDraftInput) {
  return {
    mailbox_ref_id: draft.mailbox_ref_id,
    to: draft.to.trim(),
    creator_id: draft.creator_id ?? null,
    contact_route_id: draft.contact_route_id ?? null,
    reply_to_email_id: draft.reply_to_email_id ?? null,
    subject: draft.subject.trim(),
    text: draft.text.trim(),
    html: draft.html ?? null,
    template_id: draft.template_id ?? null,
    template_variables: nonEmptyTemplateVariableOverrides(draft.template_variables),
  };
}

export function emailDraftContentSignature(draft: EmailDraft | SaveEmailDraftInput): string {
  return JSON.stringify(normalizedDraftContent(draft));
}

export function emailComposerMatchesDraft(fields: EmailComposerFields, draft: EmailDraft): boolean {
  return fields.draftId === draft.id
    && fields.expectedRevision === draft.revision
    && emailDraftContentSignature(buildSaveEmailDraftInput(fields)) === emailDraftContentSignature(draft);
}

export function emailComposerHasUnsavedChanges(
  fields: EmailComposerFields,
  initialFields: EmailComposerFields,
  persistedDraft: EmailDraft | null,
): boolean {
  if (persistedDraft) return !emailComposerMatchesDraft(fields, persistedDraft);
  return emailDraftContentSignature(buildSaveEmailDraftInput(fields))
    !== emailDraftContentSignature(buildSaveEmailDraftInput(initialFields));
}

export function emailComposerExitDisposition({
  fields,
  initialFields,
  persistedDraft,
  sending,
}: {
  fields: EmailComposerFields;
  initialFields: EmailComposerFields;
  persistedDraft: EmailDraft | null;
  sending: boolean;
}): EmailComposerExitDisposition {
  if (sending) return "close";
  return emailComposerHasUnsavedChanges(fields, initialFields, persistedDraft)
    ? "confirm"
    : "close";
}

export function emailSendIntentIdentity(draft: EmailDraft): string {
  return `${draft.id}:${draft.revision}:${emailDraftContentSignature(draft)}`;
}

export function findEmailSendIntent(
  registry: EmailSendIntentRegistry,
  draft: EmailDraft,
): EmailSendIntent | null {
  return registry.get(emailSendIntentIdentity(draft)) ?? null;
}

/**
 * Reconciles the in-memory registry with the runtime's durable send reservation.
 * A non-sending DTO is authoritative too: it clears every old intent for that draft.
 */
export function seedEmailSendIntentFromDraft(
  registry: EmailSendIntentRegistry,
  draft: EmailDraft,
): { registry: EmailSendIntentRegistry; intent: EmailSendIntent | null } {
  const next = new Map(
    [...registry].filter(([, intent]) => intent.draftId !== draft.id),
  );
  if (!draft.sending || !draft.pending_send_idempotency_key) {
    return { registry: next, intent: null };
  }

  const identity = emailSendIntentIdentity(draft);
  const intent: EmailSendIntent = {
    identity,
    draftId: draft.id,
    revision: draft.revision,
    idempotencyKey: draft.pending_send_idempotency_key,
  };
  next.set(identity, intent);
  return { registry: next, intent };
}

export function ensureEmailSendIntent(
  registry: EmailSendIntentRegistry,
  draft: EmailDraft,
  createIdempotencyKey: () => string,
): { registry: EmailSendIntentRegistry; intent: EmailSendIntent } {
  if (draft.sending) {
    const recovered = seedEmailSendIntentFromDraft(registry, draft);
    if (recovered.intent) {
      return { registry: recovered.registry, intent: recovered.intent };
    }
    throw new Error("sending draft is missing its pending idempotency key");
  }

  const identity = emailSendIntentIdentity(draft);
  const current = registry.get(identity);
  if (current) return { registry, intent: current };

  const intent: EmailSendIntent = {
    identity,
    draftId: draft.id,
    revision: draft.revision,
    idempotencyKey: createIdempotencyKey(),
  };
  const next = new Map(registry);
  next.set(identity, intent);
  return { registry: next, intent };
}

export function createTemplateRenderGate(): TemplateRenderGate {
  return { generation: 0, pendingGeneration: null };
}

export function beginTemplateRender(gate: TemplateRenderGate): {
  gate: TemplateRenderGate;
  generation: number;
} {
  const generation = gate.generation + 1;
  return {
    gate: { generation, pendingGeneration: generation },
    generation,
  };
}

export function cancelTemplateRender(gate: TemplateRenderGate): TemplateRenderGate {
  return { generation: gate.generation + 1, pendingGeneration: null };
}

export function settleTemplateRender(
  gate: TemplateRenderGate,
  generation: number,
): { gate: TemplateRenderGate; accepted: boolean } {
  if (gate.pendingGeneration !== generation) return { gate, accepted: false };
  return {
    gate: { generation: gate.generation, pendingGeneration: null },
    accepted: true,
  };
}

export function applyRenderedEmailTemplate(
  fields: EmailComposerFields,
  rendered: RenderedEmailTemplate,
): EmailComposerFields {
  return {
    ...fields,
    subject: rendered.subject,
    text: rendered.text,
    html: rendered.html,
    templateId: rendered.template_id,
  };
}

export function resolveInitialEmailTemplate(
  launch: InitialEmailTemplateLaunch | null,
  initializedLaunchKey: string | null,
  appliedLaunchKey: string | null,
  templates: EmailTemplateSummary[] | undefined,
): EmailTemplateSummary | null {
  if (
    !launch?.initialTemplateId
    || initializedLaunchKey !== launch.key
    || appliedLaunchKey === launch.key
  ) return null;
  return templates?.find((template) => template.id === launch.initialTemplateId) ?? null;
}

export function emailComposerCanSave(fields: EmailComposerFields): boolean {
  return Boolean(fields.mailboxRefId && (
    fields.to.trim()
    || fields.subject.trim()
    || fields.text.trim()
    || fields.templateId
  ));
}

export function emailComposerCanSend(fields: EmailComposerFields): boolean {
  return Boolean(
    fields.mailboxRefId
    && (fields.to.trim() || fields.replyToEmailId)
    && fields.subject.trim()
    && fields.text.trim(),
  );
}

export function replySubject(subject: string): string {
  const normalized = subject.trim();
  if (!normalized) return "Re:";
  return normalized.toLocaleLowerCase().startsWith("re:") ? normalized : `Re: ${normalized}`;
}

export type ComposerViewState = {
  formatExpanded: boolean;
  plainText: boolean;
  bodyStyle: EmailBodyStyle;
  /** fields.html 来自模板渲染且正文未被编辑：保存/发送原样用模板 html；编辑正文后改为按 Markdown 派生 */
  templateHtmlIntact: boolean;
  tab: "edit" | "preview";
};

export function createComposerViewState(
  overrides: Partial<ComposerViewState> = {},
): ComposerViewState {
  return {
    formatExpanded: true,
    plainText: false,
    bodyStyle: { ...DEFAULT_EMAIL_BODY_STYLE },
    templateHtmlIntact: false,
    tab: "edit",
    ...overrides,
  };
}

export function composerViewStateForPlainText(
  state: ComposerViewState,
  enabled: boolean,
): ComposerViewState {
  if (!enabled) return { ...state, plainText: false };
  return { ...state, plainText: true, formatExpanded: false, templateHtmlIntact: false };
}

/** Derives `html` from the Markdown body; the send path uses the saved draft, so this runs before every save. */
export function withDerivedBodyHtml(
  fields: EmailComposerFields,
  view: ComposerViewState,
): EmailComposerFields {
  if (view.plainText) return { ...fields, html: null };
  if (view.templateHtmlIntact) return fields;
  return { ...fields, html: markdownToEmailHtml(fields.text, view.bodyStyle) };
}

export function composerFieldsForPlainText(fields: EmailComposerFields): EmailComposerFields {
  return { ...fields, html: null, text: stripMarkdownForPlainText(fields.text) };
}

/**
 * Ask before switching to plain text only when formatting would actually be
 * lost: untouched template html, rich Markdown, or a chosen font/size. The
 * derived html alone is not a signal — it exists whenever the body is non-empty.
 */
export function plainTextNeedsConfirmation(fields: EmailComposerFields, view: ComposerViewState): boolean {
  return (view.templateHtmlIntact && fields.html !== null)
    || hasRichMarkdown(fields.text)
    || view.bodyStyle.fontFamily !== null
    || view.bodyStyle.fontSize !== null;
}

/** 取消使用模板：模板 html 不再原样保留，正文按 Markdown 重新派生。 */
export function detachTemplateFields(
  fields: EmailComposerFields,
  view: ComposerViewState,
): EmailComposerFields {
  return withDerivedBodyHtml({ ...fields, templateId: null, templateVariables: {} }, view);
}

/**
 * How a template render lands in the view: html carrying our `data-mail-body`
 * wrapper was Markdown-derived when the template was saved, so it is treated as
 * Markdown (style read back, re-derived from text) rather than frozen template
 * html. Plain text mode never keeps template html.
 */
export function viewStateForRenderedTemplate(
  view: ComposerViewState,
  rendered: RenderedEmailTemplate,
): ComposerViewState {
  if (rendered.html === null || view.plainText) return { ...view, templateHtmlIntact: false };
  if (isDerivedEmailHtml(rendered.html)) {
    return { ...view, templateHtmlIntact: false, bodyStyle: readEmailBodyStyle(rendered.html) };
  }
  return { ...view, templateHtmlIntact: true };
}

export function viewStateFromDraft(draft: EmailDraft): ComposerViewState {
  const html = draft.html ?? null;
  if (html === null) {
    return draft.text.trim()
      ? composerViewStateForPlainText(createComposerViewState(), true)
      : createComposerViewState();
  }
  // The wrapper marker identifies html we derived ourselves; comparing against
  // a fresh derivation would flip every saved Markdown draft to "template html"
  // after any converter change. Stale derived html heals on the next edit.
  if (isDerivedEmailHtml(html)) {
    return createComposerViewState({ bodyStyle: readEmailBodyStyle(html), templateHtmlIntact: false });
  }
  return createComposerViewState({ templateHtmlIntact: true });
}

/**
 * runtime 回来的草稿只记录内容（text/html），不记录「当前 tab」「格式栏是否
 * 展开」这类纯视图状态：保存/恢复/重读之后沿用当前的，只让 html 决定
 * 纯文本 / 模板排版 / 字体字号。正文与 html 都为空时草稿对视图没有任何
 * 断言，保持原样即可。
 */
export function viewStateAfterRuntimeDraft(current: ComposerViewState, latest: EmailDraft): ComposerViewState {
  if ((latest.html ?? null) === null && !latest.text.trim()) {
    return { ...current, templateHtmlIntact: false };
  }
  const fromDraft = viewStateFromDraft(latest);
  return {
    ...fromDraft,
    tab: current.tab,
    formatExpanded: fromDraft.plainText ? false : current.formatExpanded,
  };
}

/** 应用模板前若主题或正文非空要确认覆盖——再次选中当前模板也一样，重渲染同样会覆盖编辑。 */
export function templateApplyNeedsConfirmation(fields: EmailComposerFields): boolean {
  return Boolean(fields.subject.trim() || fields.text.trim());
}

export function isComposerSendShortcut(event: {
  key: string;
  metaKey: boolean;
  ctrlKey: boolean;
  shiftKey: boolean;
  altKey: boolean;
  isComposing: boolean;
}): boolean {
  return (event.metaKey || event.ctrlKey)
    && event.shiftKey
    && event.key === "Enter"
    && !event.altKey
    && !event.isComposing;
}

export function composerSendShortcutLabel(platform: BrowserPlatform): string {
  return platform === "macos" ? "⌘+Shift+Enter" : "Ctrl+Shift+Enter";
}

export type TemplateMenuState =
  | { kind: "loading" }
  | { kind: "error"; error: unknown }
  | { kind: "empty" }
  | { kind: "ready"; templates: EmailTemplateSummary[] };

export function templateMenuState(input: {
  isLoading: boolean;
  error: unknown;
  data: EmailTemplateSummary[] | undefined;
}): TemplateMenuState {
  if (input.isLoading) return { kind: "loading" };
  if (input.error) return { kind: "error", error: input.error };
  if (!input.data || input.data.length === 0) return { kind: "empty" };
  return { kind: "ready", templates: input.data };
}

/**
 * 收件人行是否锁定（不可改 to、不可选/解除 Creator）：回复邮件的收件人由线程决定，
 * runtime 要求与 reply_to_email_id 所在线程一致。以字段为准而不只看启动类型——
 * 由回复创建并保存的草稿再从草稿列表打开时 launch.kind 是 "draft"，但 replyToEmailId 仍在。
 * 首个 commit 里 fields 还是上一封的残留，所以回复启动同时按 launchKind 兜底。
 */
export function recipientLocked(
  launchKind: "new" | "draft" | "reply" | null,
  fields: Pick<EmailComposerFields, "replyToEmailId">,
): boolean {
  return launchKind === "reply" || Boolean(fields.replyToEmailId);
}

/**
 * 初始焦点落在第一个还需要输入的字段。已关联 Creator 时收件人行是一个 chip，
 * 没有 `mail-compose-to` 这个输入框可聚焦，直接跳到主题。
 */
export function composerInitialFocusId(
  launchKind: "new" | "draft" | "reply",
  fields: EmailComposerFields,
): "mail-compose-to" | "mail-compose-subject" | "mail-compose-body" {
  if (launchKind === "reply") return "mail-compose-body";
  if (!fields.to.trim() && !fields.creatorId) return "mail-compose-to";
  if (!fields.subject.trim()) return "mail-compose-subject";
  return "mail-compose-body";
}
