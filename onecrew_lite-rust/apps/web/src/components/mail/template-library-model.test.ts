import { describe, expect, it } from "vitest";

import {
  duplicateEmailTemplateName,
  EMAIL_TEMPLATE_BUILTIN_VARIABLES,
  filterEmailTemplates,
  isBuiltinTemplateVariable,
  templateRenderDependsOnCreator,
  templateBannerStatus,
  templateEditorCanSave,
  templateEditorIntactHtml,
  templateLibraryShowsToolbarCreate,
  templateVariableSlots,
} from "@/components/mail/template-library-model";
import { markdownToEmailHtml } from "@/components/mail/markdown-email";
import type { EmailTemplateSummary } from "@/lib/creator";

const partnership: EmailTemplateSummary = {
  id: "partnership",
  name: "Partnership intro",
  subject: "Hello {{first_name}}",
  variables: ["first_name", "handle"],
  revision: 1,
  created_at: 1,
  updated_at: 1,
};

const followUp: EmailTemplateSummary = {
  ...partnership,
  id: "follow-up",
  name: "Follow up",
  subject: "Checking in",
  variables: [],
};

const formatCopyName = (name: string, index: number) => (
  index === 1 ? `${name} 副本` : `${name} 副本 ${index}`
);

describe("mail template library model", () => {
  it("searches the loaded template summaries by name, subject, and variable", () => {
    const templates = [partnership, followUp];

    expect(filterEmailTemplates(templates, "follow")).toEqual([followUp]);
    expect(filterEmailTemplates(templates, "checking")).toEqual([followUp]);
    expect(filterEmailTemplates(templates, "HANDLE")).toEqual([partnership]);
    expect(filterEmailTemplates(templates, "  ")).toEqual(templates);
  });

  it("requires a name, a subject, and either a text or an html body before saving", () => {
    const base = { name: "Intro", subject: "Hi", text: "", html: null };
    expect(templateEditorCanSave(base)).toBe(false);
    expect(templateEditorCanSave({ ...base, text: "Body" })).toBe(true);
    expect(templateEditorCanSave({ ...base, html: "<p>Body</p>" })).toBe(true);
    expect(templateEditorCanSave({ ...base, html: "   " })).toBe(false);
    expect(templateEditorCanSave({ ...base, text: "Body", name: "  " })).toBe(false);
    expect(templateEditorCanSave({ ...base, text: "Body", subject: "  " })).toBe(false);
  });

  it("derives a duplicate name that skips names already taken", () => {
    expect(duplicateEmailTemplateName("Intro", ["Intro"], formatCopyName)).toBe("Intro 副本");
    expect(duplicateEmailTemplateName("Intro", ["Intro", "Intro 副本"], formatCopyName)).toBe("Intro 副本 2");
    expect(duplicateEmailTemplateName("Intro", ["Intro", "Intro 副本", "Intro 副本 2"], formatCopyName)).toBe("Intro 副本 3");
    expect(duplicateEmailTemplateName("Intro", ["Intro", "Intro 副本 2"], formatCopyName)).toBe("Intro 副本");
  });

  it("hides the toolbar create button only when the empty state already carries it", () => {
    expect(templateLibraryShowsToolbarCreate({ total: 0, query: "" })).toBe(false);
    expect(templateLibraryShowsToolbarCreate({ total: 0, query: "  " })).toBe(false);
    expect(templateLibraryShowsToolbarCreate({ total: 0, query: "intro" })).toBe(true);
    expect(templateLibraryShowsToolbarCreate({ total: 3, query: "" })).toBe(true);
    expect(templateLibraryShowsToolbarCreate({ total: 3, query: "intro" })).toBe(true);
  });

  it("keeps only MCP-authored template html intact; derived html is re-derived from text", () => {
    expect(templateEditorIntactHtml(null)).toBeNull();
    expect(templateEditorIntactHtml(markdownToEmailHtml("**Body**"))).toBeNull();
    expect(templateEditorIntactHtml("<table><tr><td>Hi</td></tr></table>")).toBe("<table><tr><td>Hi</td></tr></table>");
  });

  it("knows the four runtime-filled builtin variables", () => {
    expect(EMAIL_TEMPLATE_BUILTIN_VARIABLES.map((variable) => variable.name)).toEqual([
      "name",
      "first_name",
      "handle",
      "email",
    ]);
    expect(isBuiltinTemplateVariable("first_name")).toBe(true);
    expect(isBuiltinTemplateVariable("creator_name")).toBe(false);
  });

  it("only treats a creator change as invalidating the render when builtin variables are used", () => {
    expect(templateRenderDependsOnCreator([])).toBe(false);
    expect(templateRenderDependsOnCreator(["topic"])).toBe(false);
    expect(templateRenderDependsOnCreator(["name"])).toBe(true);
    expect(templateRenderDependsOnCreator(["topic", "first_name"])).toBe(true);
  });

  describe("templateVariableSlots", () => {
    it("labels a custom variable by its name and flags it when unresolved", () => {
      expect(templateVariableSlots({ variables: ["creator_name"], unresolved: ["creator_name"], hasCreator: false })).toEqual([
        { name: "creator_name", builtin: false, unresolved: true, source: null },
      ]);
    });

    it("keeps a resolved custom variable unflagged", () => {
      expect(templateVariableSlots({ variables: ["creator_name"], unresolved: [], hasCreator: true })).toEqual([
        { name: "creator_name", builtin: false, unresolved: false, source: null },
      ]);
    });

    it("tells the user a builtin variable auto-fills when the mail has a creator", () => {
      expect(templateVariableSlots({ variables: ["first_name"], unresolved: [], hasCreator: true })).toEqual([
        { name: "first_name", builtin: true, unresolved: false, source: "linked" },
      ]);
    });

    it("tells the user a builtin variable needs a creator when the mail has none", () => {
      expect(templateVariableSlots({ variables: ["email"], unresolved: ["email"], hasCreator: false })).toEqual([
        { name: "email", builtin: true, unresolved: true, source: "unlinked" },
      ]);
    });

    it("asks the user to fill a builtin variable the linked creator could not provide", () => {
      expect(templateVariableSlots({ variables: ["email"], unresolved: ["email"], hasCreator: true })).toEqual([
        { name: "email", builtin: true, unresolved: true, source: "missing" },
      ]);
    });

    it("preserves the template's variable order", () => {
      const slots = templateVariableSlots({ variables: ["handle", "custom", "name"], unresolved: ["custom"], hasCreator: true });
      expect(slots.map((slot) => slot.name)).toEqual(["handle", "custom", "name"]);
      expect(slots.map((slot) => slot.builtin)).toEqual([true, false, true]);
    });
  });

  describe("templateBannerStatus", () => {
    it("reports the unresolved count ahead of the dirty and creator-linked states", () => {
      expect(templateBannerStatus({ unresolvedCount: 2, dirty: true, creatorLinked: false })).toEqual({ kind: "unresolved", count: 2 });
      expect(templateBannerStatus({ unresolvedCount: 1, dirty: false, creatorLinked: false })).toEqual({ kind: "unresolved", count: 1 });
      expect(templateBannerStatus({ unresolvedCount: 1, dirty: true, creatorLinked: true })).toEqual({ kind: "unresolved", count: 1 });
    });

    it("tells the user a re-apply will auto-fill once a creator was linked after the last render", () => {
      expect(templateBannerStatus({ unresolvedCount: 0, dirty: true, creatorLinked: true }))
        .toEqual({ kind: "creator-linked" });
      expect(templateBannerStatus({ unresolvedCount: 0, dirty: false, creatorLinked: true }))
        .toEqual({ kind: "creator-linked" });
    });

    it("asks for a re-apply when variables changed without unresolved ones", () => {
      expect(templateBannerStatus({ unresolvedCount: 0, dirty: true, creatorLinked: false })).toEqual({ kind: "dirty" });
    });

    it("shows nothing when the template is applied and clean", () => {
      expect(templateBannerStatus({ unresolvedCount: 0, dirty: false, creatorLinked: false })).toBeNull();
    });
  });

});
