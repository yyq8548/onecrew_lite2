import { isDerivedEmailHtml } from "@/components/mail/markdown-email";
import type { EmailTemplateSummary } from "@/lib/creator";

export function filterEmailTemplates(
  templates: EmailTemplateSummary[],
  query: string,
): EmailTemplateSummary[] {
  const normalized = query.trim().toLocaleLowerCase();
  if (!normalized) return templates;
  return templates.filter((template) => (
    [template.name, template.subject, ...template.variables]
      .join(" ")
      .toLocaleLowerCase()
      .includes(normalized)
  ));
}

/** 编辑器的保存条件：名称、主题必填，正文 text 或 html 至少一个非空。 */
export function templateEditorCanSave(fields: {
  name: string;
  subject: string;
  text: string;
  html: string | null;
}): boolean {
  return Boolean(
    fields.name.trim()
    && fields.subject.trim()
    && (fields.text.trim() || fields.html?.trim()),
  );
}

/** 复制模板时的新名字：先试「name 副本」，冲突则「name 副本 2」「name 副本 3」…。 */
export function duplicateEmailTemplateName(
  name: string,
  existingNames: readonly string[],
  formatName: (name: string, index: number) => string,
): string {
  const taken = new Set(existingNames);
  const base = formatName(name, 1);
  if (!taken.has(base)) return base;
  for (let index = 2; ; index += 1) {
    const candidate = formatName(name, index);
    if (!taken.has(candidate)) return candidate;
  }
}

/**
 * 工具行是否显示「新建模板」：列表为空且无搜索词时空状态里已放主按钮，
 * 再显示一个同义主操作会重复（UI-11）；其余情况都显示。
 */
export function templateLibraryShowsToolbarCreate(input: { total: number; query: string }): boolean {
  return !(input.total === 0 && !input.query.trim());
}

/**
 * 模板编辑器打开时要原样保留的 html：只有 MCP 写的 HTML 模板（非 Markdown
 * 派生）才保留；Markdown 派生的 html 是正文快照，随正文改动会分叉，不随模板
 * 保存（应用时按 text 重新派生）。返回 null 表示按 Markdown 处理。
 */
export function templateEditorIntactHtml(html: string | null): string | null {
  return html !== null && !isDerivedEmailHtml(html) ? html : null;
}

// ---------------------------------------------------------------------------
// 模板变量
// ---------------------------------------------------------------------------

export type EmailTemplateBuiltinVariableName = "name" | "first_name" | "handle" | "email";

/**
 * runtime 渲染模板时会自动填充的内置变量：只有邮件关联了 Creator
 * （fields.creatorId 非空）时才取到值；其他变量只能在写信时手填或由 Agent 传值。
 */
export const EMAIL_TEMPLATE_BUILTIN_VARIABLES: ReadonlyArray<{
  name: EmailTemplateBuiltinVariableName;
}> = [
  { name: "name" },
  { name: "first_name" },
  { name: "handle" },
  { name: "email" },
];

export function isBuiltinTemplateVariable(name: string): boolean {
  return EMAIL_TEMPLATE_BUILTIN_VARIABLES.some((variable) => variable.name === name);
}

/**
 * 关联 / 解除 Creator 是否会改变模板的渲染结果：只有模板用到内置变量时 creator_id
 * 才参与渲染。无变量或只有自定义变量的模板，换 Creator 不必重新应用，也不该锁发送。
 */
export function templateRenderDependsOnCreator(variables: readonly string[]): boolean {
  return variables.some(isBuiltinTemplateVariable);
}

export type TemplateVariableSlot = {
  name: string;
  builtin: boolean;
  unresolved: boolean;
  source: "linked" | "unlinked" | "missing" | null;
};

/**
 * 模板横幅里每个变量输入框的状态：占位文案告诉用户该变量从哪来——自定义变量
 * 只显示变量名；内置变量按是否关联 Creator 提示「自动填充」或「关联 Creator 后自动填充」；
 * 已关联 Creator 但 runtime 仍报未解析（Creator 记录缺该字段）时提示手填，不再说「自动填充」。
 */
export function templateVariableSlots(input: {
  variables: readonly string[];
  unresolved: readonly string[];
  hasCreator: boolean;
}): TemplateVariableSlot[] {
  const unresolved = new Set(input.unresolved);
  return input.variables.map((name) => {
    const builtin = EMAIL_TEMPLATE_BUILTIN_VARIABLES.find((variable) => variable.name === name);
    const isUnresolved = unresolved.has(name);
    const source = builtin ? builtinVariableSource(input.hasCreator, isUnresolved) : null;
    return { name, builtin: Boolean(builtin), unresolved: isUnresolved, source };
  });
}

function builtinVariableSource(
  hasCreator: boolean,
  unresolved: boolean,
): "linked" | "unlinked" | "missing" {
  if (!hasCreator) return "unlinked";
  return unresolved ? "missing" : "linked";
}

export type TemplateBannerStatus =
  | { kind: "unresolved"; count: number }
  | { kind: "creator-linked" }
  | { kind: "dirty" };

/**
 * 模板横幅第一行的状态小字：未解析优先；其次是「上次渲染之后才关联了 Creator」
 * （重新应用才会自动填内置变量，比泛泛的「已修改」更能说明该做什么）；再是「已修改」；
 * 都没有时不显示。
 */
export function templateBannerStatus(input: {
  unresolvedCount: number;
  dirty: boolean;
  /** 上次模板渲染之后是否（重新）关联了 Creator */
  creatorLinked: boolean;
}): TemplateBannerStatus | null {
  if (input.unresolvedCount > 0) {
    return { kind: "unresolved", count: input.unresolvedCount };
  }
  if (input.creatorLinked) {
    return { kind: "creator-linked" };
  }
  if (input.dirty) return { kind: "dirty" };
  return null;
}
