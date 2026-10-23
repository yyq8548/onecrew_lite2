import { describe, expect, it } from "vitest";

import {
  applyRenderedEmailTemplate,
  beginTemplateRender,
  buildSaveEmailDraftInput,
  buildSaveEmailTemplateInput,
  cancelTemplateRender,
  composerFieldsForPlainText,
  composerInitialFocusId,
  recipientLocked,
  composerSendShortcutLabel,
  composerViewStateForPlainText,
  createComposerViewState,
  createTemplateRenderGate,
  createEmailComposerFields,
  detachTemplateFields,
  emailComposerCanSave,
  emailComposerCanSend,
  emailComposerExitDisposition,
  emailComposerFieldsFromDraft,
  emailComposerHasUnsavedChanges,
  emailComposerMatchesDraft,
  ensureEmailSendIntent,
  findEmailSendIntent,
  isComposerSendShortcut,
  nonEmptyTemplateVariableOverrides,
  plainTextNeedsConfirmation,
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
  viewStateFromDraft,
  withDerivedBodyHtml,
} from "@/components/mail/composer-model";
import {
  DEFAULT_EMAIL_BODY_STYLE,
  markdownToEmailHtml,
} from "@/components/mail/markdown-email";
import type { EmailDraft, EmailTemplate } from "@/lib/creator";

const template: EmailTemplate = {
  id: "template-1",
  name: "Intro",
  subject: "Hello {{name}}",
  text: "Hi {{name}}, {{offer}}",
  html: null,
  variables: ["name", "offer"],
  revision: 3,
  created_at: 1,
  updated_at: 2,
};

const persistedDraft: EmailDraft = {
  id: "draft-1",
  mailbox_ref_id: "mailbox-1",
  to: " creator@example.com ",
  creator_id: "creator-1",
  contact_route_id: "route-1",
  reply_to_email_id: null,
  subject: " Subject ",
  text: " Body ",
  html: null,
  template_id: "template-1",
  template_variables: { name: "Ada" },
  state: "draft",
  sending: false,
  pending_send_idempotency_key: null,
  revision: 4,
  created_at: 1,
  updated_at: 2,
};

describe("email composer model", () => {
  it("maps a persisted draft back to optimistic-concurrency fields", () => {
    const fields = emailComposerFieldsFromDraft(persistedDraft);
    expect(fields.draftId).toBe("draft-1");
    expect(fields.expectedRevision).toBe(4);
    expect(buildSaveEmailDraftInput(fields)).toMatchObject({
      id: "draft-1",
      expected_revision: 4,
      to: "creator@example.com",
      subject: "Subject",
      text: "Body",
    });
  });

  it("submits only explicit, non-empty overrides declared by the selected template", () => {
    expect(templateVariableFields(template, {
      name: "Ada",
      offer: "   ",
      stale: "remove",
    })).toEqual({ name: "Ada" });
    expect(nonEmptyTemplateVariableOverrides({ handle: "", name: "Ada", note: "  " })).toEqual({
      name: "Ada",
    });

    const fields = createEmailComposerFields("mailbox-1", {
      templateVariables: { handle: "", name: "Ada" },
    });
    expect(buildSaveEmailDraftInput(fields).template_variables).toEqual({ name: "Ada" });
  });

  it("applies server-rendered content without changing recipients", () => {
    const initial = createEmailComposerFields("mailbox-1", { to: "creator@example.com" });
    const rendered = applyRenderedEmailTemplate(initial, {
      template_id: "template-1",
      revision: 3,
      subject: "Hello Ada",
      text: "Hi Ada",
      html: null,
      unresolved: [],
    });
    expect(rendered).toMatchObject({
      to: "creator@example.com",
      templateId: "template-1",
      subject: "Hello Ada",
      text: "Hi Ada",
    });
  });

  it("loads an initial template only after launch initialization and only once per launch", () => {
    const launch = { key: "launch-1", initialTemplateId: template.id };

    expect(resolveInitialEmailTemplate(launch, null, null, [template])).toBeNull();
    expect(resolveInitialEmailTemplate(launch, launch.key, null, undefined)).toBeNull();
    expect(resolveInitialEmailTemplate(launch, launch.key, null, [template])).toBe(template);
    expect(resolveInitialEmailTemplate(launch, launch.key, launch.key, [template])).toBeNull();
    expect(resolveInitialEmailTemplate(
      { key: "launch-2", initialTemplateId: "missing" },
      "launch-2",
      null,
      [template],
    )).toBeNull();
  });

  it("updates an existing template with its id and expected revision", () => {
    const fields = createEmailComposerFields("mailbox-1", {
      subject: "Updated subject",
      text: "Updated body",
    });
    const view = createComposerViewState();
    expect(buildSaveEmailTemplateInput(fields, view, " Renamed ", template)).toEqual({
      id: "template-1",
      name: "Renamed",
      subject: "Updated subject",
      text: "Updated body",
      html: null,
      expected_revision: 3,
    });
    expect(buildSaveEmailTemplateInput(fields, view, "New", null)).toMatchObject({
      id: null,
      expected_revision: null,
    });
  });

  it("stores html on a template only when it is untouched template html", () => {
    const markdown = withDerivedBodyHtml(
      createEmailComposerFields("mailbox-1", { text: "Hi {{name}}" }),
      createComposerViewState(),
    );
    expect(markdown.html).toContain("{{name}}");
    expect(templateHtmlForSave(markdown, createComposerViewState())).toBeNull();
    expect(buildSaveEmailTemplateInput(markdown, createComposerViewState(), "T", null).html).toBeNull();
    // Stale flag on derived html still does not freeze the body into the template.
    expect(templateHtmlForSave(markdown, createComposerViewState({ templateHtmlIntact: true }))).toBeNull();

    const fromTemplate = createEmailComposerFields("mailbox-1", { text: "Hi", html: "<p>Hi</p>" });
    expect(templateHtmlForSave(fromTemplate, createComposerViewState({ templateHtmlIntact: true })))
      .toBe("<p>Hi</p>");
    expect(templateHtmlForSave(fromTemplate, createComposerViewState())).toBeNull();
  });

  it("distinguishes saveable partial drafts from sendable messages", () => {
    const empty = createEmailComposerFields("mailbox-1");
    const partial = { ...empty, subject: "An idea" };
    expect(emailComposerCanSave(empty)).toBe(false);
    expect(emailComposerCanSave(partial)).toBe(true);
    expect(emailComposerCanSend(partial)).toBe(false);
    expect(emailComposerCanSend({ ...partial, to: "a@b.com", text: "Hello" })).toBe(true);
    expect(emailComposerCanSend({
      ...partial,
      text: "Reply body",
      replyToEmailId: "inbound-email-1",
    })).toBe(true);
    expect(buildSaveEmailDraftInput({
      ...partial,
      text: "Reply body",
      replyToEmailId: "inbound-email-1",
    })).toMatchObject({
      to: "",
      creator_id: null,
      contact_route_id: null,
      reply_to_email_id: "inbound-email-1",
    });
  });

  it("normalizes reply subjects once", () => {
    expect(replySubject("Hello")).toBe("Re: Hello");
    expect(replySubject("RE: Hello")).toBe("RE: Hello");
  });

  it("recognizes when composer content still matches the persisted revision", () => {
    const fields = emailComposerFieldsFromDraft(persistedDraft);
    expect(emailComposerMatchesDraft(fields, persistedDraft)).toBe(true);
    expect(emailComposerMatchesDraft({ ...fields, text: "Changed" }, persistedDraft)).toBe(false);
  });

  it("guards only unsaved composer exits and treats a persisted send reservation as safe to close", () => {
    const initial = createEmailComposerFields("mailbox-1");
    expect(emailComposerHasUnsavedChanges(initial, initial, null)).toBe(false);
    expect(emailComposerExitDisposition({
      fields: initial,
      initialFields: initial,
      persistedDraft: null,
      sending: false,
    })).toBe("close");

    const edited = { ...initial, subject: "Unsaved subject" };
    expect(emailComposerHasUnsavedChanges(edited, initial, null)).toBe(true);
    expect(emailComposerExitDisposition({
      fields: edited,
      initialFields: initial,
      persistedDraft: null,
      sending: false,
    })).toBe("confirm");

    const savedFields = emailComposerFieldsFromDraft(persistedDraft);
    expect(emailComposerExitDisposition({
      fields: savedFields,
      initialFields: initial,
      persistedDraft,
      sending: false,
    })).toBe("close");
    expect(emailComposerExitDisposition({
      fields: { ...savedFields, text: "Unsaved edit" },
      initialFields: initial,
      persistedDraft,
      sending: false,
    })).toBe("confirm");
    expect(emailComposerExitDisposition({
      fields: { ...savedFields, text: "Unsaved edit" },
      initialFields: initial,
      persistedDraft: { ...persistedDraft, sending: true, pending_send_idempotency_key: "send-key" },
      sending: true,
    })).toBe("close");
  });

  it("reuses one idempotency key for retries and rotates only for content or revision changes", () => {
    let generated = 0;
    const createKey = () => `key-${++generated}`;
    const first = ensureEmailSendIntent(new Map(), persistedDraft, createKey);
    const retry = ensureEmailSendIntent(first.registry, persistedDraft, createKey);
    expect(retry.intent.idempotencyKey).toBe("key-1");
    expect(generated).toBe(1);

    const reservedByCore = {
      ...persistedDraft,
      sending: true,
      pending_send_idempotency_key: "key-1",
    };
    const recovery = ensureEmailSendIntent(retry.registry, reservedByCore, createKey);
    expect(recovery.intent.idempotencyKey).toBe("key-1");
    expect(findEmailSendIntent(recovery.registry, reservedByCore)?.idempotencyKey).toBe("key-1");
    expect(generated).toBe(1);

    const changedContent = ensureEmailSendIntent(retry.registry, {
      ...persistedDraft,
      text: "Changed body",
    }, createKey);
    expect(changedContent.intent.idempotencyKey).toBe("key-2");

    const changedRevision = ensureEmailSendIntent(changedContent.registry, {
      ...persistedDraft,
      revision: persistedDraft.revision + 1,
    }, createKey);
    expect(changedRevision.intent.idempotencyKey).toBe("key-3");
  });

  it("recovers the runtime send key after reopening and clears it for an ordinary draft", () => {
    const reloadedSendingDraft = {
      ...persistedDraft,
      sending: true,
      pending_send_idempotency_key: "runtime-reservation-key",
    };
    const reopened = seedEmailSendIntentFromDraft(new Map(), reloadedSendingDraft);
    expect(reopened.intent).toMatchObject({
      draftId: persistedDraft.id,
      revision: persistedDraft.revision,
      idempotencyKey: "runtime-reservation-key",
    });
    expect(findEmailSendIntent(reopened.registry, reloadedSendingDraft)?.idempotencyKey)
      .toBe("runtime-reservation-key");

    const ordinary = seedEmailSendIntentFromDraft(reopened.registry, persistedDraft);
    expect(ordinary.intent).toBeNull();
    expect(findEmailSendIntent(ordinary.registry, persistedDraft)).toBeNull();

    const fresh = ensureEmailSendIntent(ordinary.registry, persistedDraft, () => "fresh-key");
    expect(fresh.intent.idempotencyKey).toBe("fresh-key");
  });

  it("accepts only the latest template render and invalidates pending work on cancel", () => {
    const first = beginTemplateRender(createTemplateRenderGate());
    const second = beginTemplateRender(first.gate);

    const stale = settleTemplateRender(second.gate, first.generation);
    expect(stale.accepted).toBe(false);
    expect(stale.gate.pendingGeneration).toBe(second.generation);

    const latest = settleTemplateRender(stale.gate, second.generation);
    expect(latest.accepted).toBe(true);
    expect(latest.gate.pendingGeneration).toBeNull();

    const third = beginTemplateRender(latest.gate);
    const cancelled = cancelTemplateRender(third.gate);
    expect(settleTemplateRender(cancelled, third.generation).accepted).toBe(false);
  });
});

describe("composer view state", () => {
  const shortcut = {
    key: "Enter",
    metaKey: false,
    ctrlKey: true,
    shiftKey: true,
    altKey: false,
    isComposing: false,
  };

  it("starts with the format bar expanded on the edit tab and collapses it for plain text", () => {
    const initial = createComposerViewState();
    expect(initial).toEqual({
      formatExpanded: true,
      plainText: false,
      bodyStyle: DEFAULT_EMAIL_BODY_STYLE,
      templateHtmlIntact: false,
      tab: "edit",
    });
    expect(createComposerViewState({ tab: "preview" }).tab).toBe("preview");

    const enabled = composerViewStateForPlainText(
      { ...initial, templateHtmlIntact: true, tab: "preview" },
      true,
    );
    expect(enabled).toMatchObject({
      plainText: true,
      formatExpanded: false,
      templateHtmlIntact: false,
      tab: "preview",
    });
    const disabled = composerViewStateForPlainText(enabled, false);
    expect(disabled).toMatchObject({ plainText: false, formatExpanded: false, tab: "preview" });
  });

  it("derives html from markdown unless plain text or an untouched template html is in play", () => {
    const fields = createEmailComposerFields("mailbox-1", {
      text: "**Hi** Ada",
      html: "<p>template</p>",
    });
    const style = { fontFamily: "SimHei, sans-serif", fontSize: 14 };

    expect(withDerivedBodyHtml(fields, createComposerViewState({ plainText: true })).html).toBeNull();
    expect(withDerivedBodyHtml(fields, createComposerViewState({ templateHtmlIntact: true })))
      .toBe(fields);
    const derived = withDerivedBodyHtml(fields, createComposerViewState({ bodyStyle: style }));
    expect(derived.html).toBe(markdownToEmailHtml("**Hi** Ada", style));
    expect(derived.html).toContain("<strong>Hi</strong>");
    expect(derived.html).toContain("font-size: 14px");
    expect(withDerivedBodyHtml({ ...fields, text: "  " }, createComposerViewState()).html).toBeNull();
  });

  it("converts fields to plain text and asks first only when formatting would be lost", () => {
    const view = createComposerViewState();
    const rich = createEmailComposerFields("mailbox-1", {
      text: "**Hi** [docs](https://example.com)",
      html: "<p>x</p>",
    });
    expect(plainTextNeedsConfirmation(rich, view)).toBe(true);
    expect(plainTextNeedsConfirmation({ ...rich, html: null }, view)).toBe(true);
    expect(plainTextNeedsConfirmation({ ...rich, html: null, text: "plain" }, view)).toBe(false);
    // Derived html exists for any non-empty body; on its own it is not lost formatting.
    const derived = withDerivedBodyHtml({ ...rich, text: "plain" }, view);
    expect(derived.html).not.toBeNull();
    expect(plainTextNeedsConfirmation(derived, view)).toBe(false);
    expect(plainTextNeedsConfirmation({ ...rich, text: "plain" }, createComposerViewState({ templateHtmlIntact: true })))
      .toBe(true);
    expect(plainTextNeedsConfirmation(
      derived,
      createComposerViewState({ bodyStyle: { fontFamily: null, fontSize: 16 } }),
    )).toBe(true);
    expect(plainTextNeedsConfirmation(
      derived,
      createComposerViewState({ bodyStyle: { fontFamily: "SimHei, sans-serif", fontSize: null } }),
    )).toBe(true);

    expect(composerFieldsForPlainText(rich)).toMatchObject({
      html: null,
      text: "Hi docs (https://example.com)",
    });
  });

  it("marks a persisted draft dirty after switching to plain text only when it carried html", () => {
    const withHtml: EmailDraft = { ...persistedDraft, html: "<p>Body</p>" };
    const fromHtml = composerFieldsForPlainText(emailComposerFieldsFromDraft(withHtml));
    expect(emailComposerMatchesDraft(fromHtml, withHtml)).toBe(false);

    const fromPlain = composerFieldsForPlainText(emailComposerFieldsFromDraft(persistedDraft));
    expect(emailComposerMatchesDraft(fromPlain, persistedDraft)).toBe(true);
  });

  it("restores the view state that produced each kind of saved draft", () => {
    const legacyTemplate = viewStateFromDraft({
      ...persistedDraft,
      text: "Hi Ada",
      html: "<p>Hi Ada</p>",
    });
    expect(legacyTemplate).toMatchObject({
      templateHtmlIntact: true,
      plainText: false,
      bodyStyle: DEFAULT_EMAIL_BODY_STYLE,
    });

    const style = { fontFamily: "KaiTi, serif", fontSize: 16 };
    const markdownDraft = viewStateFromDraft({
      ...persistedDraft,
      text: "Hello\nworld",
      html: markdownToEmailHtml("Hello\nworld", style),
    });
    expect(markdownDraft).toMatchObject({
      templateHtmlIntact: false,
      plainText: false,
      bodyStyle: style,
    });

    // The wrapper marker decides, not byte equality: html derived from an older
    // body (or an older converter) is still a Markdown draft with its style.
    const driftedDraft = viewStateFromDraft({
      ...persistedDraft,
      text: "Hello\nworld (edited elsewhere)",
      html: markdownToEmailHtml("Hello\nworld", style),
    });
    expect(driftedDraft).toMatchObject({ templateHtmlIntact: false, bodyStyle: style });

    const crlfDraft = viewStateFromDraft({
      ...persistedDraft,
      text: "a\r\nb",
      html: markdownToEmailHtml("a\r\nb"),
    });
    expect(crlfDraft.templateHtmlIntact).toBe(false);

    const plainDraft = viewStateFromDraft({ ...persistedDraft, text: "just text", html: null });
    expect(plainDraft).toMatchObject({ plainText: true, formatExpanded: false });

    const emptyDraft = viewStateFromDraft({ ...persistedDraft, text: "   ", html: null });
    expect(emptyDraft).toEqual(createComposerViewState());
  });

  it("keeps tab and format bar across runtime draft round-trips", () => {
    const current = createComposerViewState({ tab: "preview", formatExpanded: false, templateHtmlIntact: true });
    expect(viewStateAfterRuntimeDraft(current, { ...persistedDraft, text: " ", html: null }))
      .toEqual({ ...current, templateHtmlIntact: false });

    const plain = viewStateAfterRuntimeDraft(
      { ...current, formatExpanded: true },
      { ...persistedDraft, text: "plain", html: null },
    );
    expect(plain).toMatchObject({ plainText: true, formatExpanded: false, tab: "preview" });

    const style = { fontFamily: "KaiTi, serif", fontSize: 16 };
    const markdown = viewStateAfterRuntimeDraft(
      { ...current, formatExpanded: true },
      { ...persistedDraft, text: "Hi", html: markdownToEmailHtml("Hi", style) },
    );
    expect(markdown).toMatchObject({
      plainText: false,
      formatExpanded: true,
      tab: "preview",
      templateHtmlIntact: false,
      bodyStyle: style,
    });

    const templateHtml = viewStateAfterRuntimeDraft(current, { ...persistedDraft, text: "Hi", html: "<p>Hi</p>" });
    expect(templateHtml).toMatchObject({ templateHtmlIntact: true, tab: "preview" });
  });

  it("treats a rendered template as intact html only when the html is not markdown-derived", () => {
    const view = createComposerViewState();
    const rendered = { template_id: "template-1", revision: 3, subject: "S", text: "Hi", unresolved: [] };
    expect(viewStateForRenderedTemplate(view, { ...rendered, html: null }).templateHtmlIntact).toBe(false);
    expect(viewStateForRenderedTemplate(view, { ...rendered, html: "<p>Hi</p>" }).templateHtmlIntact).toBe(true);
    expect(viewStateForRenderedTemplate(
      createComposerViewState({ plainText: true }),
      { ...rendered, html: "<p>Hi</p>" },
    ).templateHtmlIntact).toBe(false);

    const style = { fontFamily: "SimHei, sans-serif", fontSize: 14 };
    const legacyDerived = viewStateForRenderedTemplate(view, { ...rendered, html: markdownToEmailHtml("Hi", style) });
    expect(legacyDerived).toMatchObject({ templateHtmlIntact: false, bodyStyle: style });
    expect(withDerivedBodyHtml(
      createEmailComposerFields("mailbox-1", { text: "Changed", html: markdownToEmailHtml("Hi", style) }),
      legacyDerived,
    ).html).toBe(markdownToEmailHtml("Changed", style));
  });

  it("re-derives html from markdown when a template is detached", () => {
    const fields = createEmailComposerFields("mailbox-1", {
      text: "Hi **Ada**",
      html: "<p>template html</p>",
      templateId: "template-1",
      templateVariables: { name: "Ada" },
    });
    const detached = detachTemplateFields(fields, createComposerViewState());
    expect(detached).toMatchObject({ templateId: null, templateVariables: {} });
    expect(detached.html).toBe(markdownToEmailHtml("Hi **Ada**"));
    expect(detachTemplateFields(fields, createComposerViewState({ plainText: true })).html).toBeNull();
  });

  it("confirms template application whenever it would overwrite typed content", () => {
    const empty = createEmailComposerFields("mailbox-1");
    expect(templateApplyNeedsConfirmation(empty)).toBe(false);
    expect(templateApplyNeedsConfirmation({ ...empty, subject: "Typed" })).toBe(true);
    expect(templateApplyNeedsConfirmation({ ...empty, text: "Typed" })).toBe(true);
    expect(templateApplyNeedsConfirmation({ ...empty, to: "a@b.com" })).toBe(false);
    // Re-picking the active template re-renders and overwrites edits just the same.
    expect(templateApplyNeedsConfirmation({ ...empty, subject: "Typed", templateId: "template-1" })).toBe(true);
  });

  it("recognises the send shortcut and ignores it while an IME composition is active", () => {
    expect(isComposerSendShortcut(shortcut)).toBe(true);
    expect(isComposerSendShortcut({ ...shortcut, ctrlKey: false, metaKey: true })).toBe(true);
    expect(isComposerSendShortcut({ ...shortcut, shiftKey: false })).toBe(false);
    expect(isComposerSendShortcut({ ...shortcut, ctrlKey: false })).toBe(false);
    expect(isComposerSendShortcut({ ...shortcut, key: "a" })).toBe(false);
    expect(isComposerSendShortcut({ ...shortcut, altKey: true })).toBe(false);
    expect(isComposerSendShortcut({ ...shortcut, isComposing: true })).toBe(false);
  });

  it("labels the send shortcut per platform", () => {
    expect(composerSendShortcutLabel("macos")).toBe("⌘+Shift+Enter");
    expect(composerSendShortcutLabel("windows")).toBe("Ctrl+Shift+Enter");
    expect(composerSendShortcutLabel("linux")).toBe("Ctrl+Shift+Enter");
    expect(composerSendShortcutLabel("unknown")).toBe("Ctrl+Shift+Enter");
  });

  it("derives the template menu state from the query", () => {
    const runtimeError = new Error("boom");
    const opaqueError = {};
    expect(templateMenuState({ isLoading: true, error: null, data: undefined }))
      .toEqual({ kind: "loading" });
    expect(templateMenuState({ isLoading: false, error: runtimeError, data: undefined }))
      .toEqual({ kind: "error", error: runtimeError });
    expect(templateMenuState({ isLoading: false, error: opaqueError, data: undefined }))
      .toEqual({ kind: "error", error: opaqueError });
    expect(templateMenuState({ isLoading: false, error: null, data: undefined }))
      .toEqual({ kind: "empty" });
    expect(templateMenuState({ isLoading: false, error: null, data: [] }))
      .toEqual({ kind: "empty" });
    expect(templateMenuState({ isLoading: false, error: null, data: [template] }))
      .toEqual({ kind: "ready", templates: [template] });
  });

  it("focuses the first field that still needs input", () => {
    const empty = createEmailComposerFields("mailbox-1");
    expect(composerInitialFocusId("new", empty)).toBe("mail-compose-to");
    expect(composerInitialFocusId("draft", { ...empty, to: "a@b.com" })).toBe("mail-compose-subject");
    expect(composerInitialFocusId("draft", { ...empty, to: "a@b.com", subject: "Hi" }))
      .toBe("mail-compose-body");
    expect(composerInitialFocusId("reply", empty)).toBe("mail-compose-body");
  });

  it("locks the recipient row for replies and for drafts that were created as replies", () => {
    const empty = createEmailComposerFields("mailbox-1");
    expect(recipientLocked("reply", empty)).toBe(true);
    expect(recipientLocked("draft", { ...empty, replyToEmailId: "email-1" })).toBe(true);
    expect(recipientLocked("draft", empty)).toBe(false);
    expect(recipientLocked("new", empty)).toBe(false);
    expect(recipientLocked(null, empty)).toBe(false);
  });

  it("skips the recipient input once a creator is linked, because the row is a chip", () => {
    const empty = createEmailComposerFields("mailbox-1");
    expect(composerInitialFocusId("new", { ...empty, creatorId: "creator-1", contactRouteId: "route-1", to: "a@b.com" }))
      .toBe("mail-compose-subject");
    expect(composerInitialFocusId("new", { ...empty, creatorId: "creator-1", contactRouteId: "route-1" }))
      .toBe("mail-compose-subject");
    expect(composerInitialFocusId("new", { ...empty, creatorId: "creator-1", contactRouteId: "route-1", to: "a@b.com", subject: "Hi" }))
      .toBe("mail-compose-body");
  });
});
