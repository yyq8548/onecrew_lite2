/**
 * 写邮件正文格式栏的 Markdown 编辑动作（纯 TS，无 React/DOM 依赖）。
 *
 * 组件层拿 textarea 的 selectionStart/selectionEnd 调 `applyMarkdownCommand`，
 * 再用 setSelectionRange + document.execCommand("insertText") 只替换
 * `[start, end)` 这一段（最小替换），以保住浏览器原生撤销栈；不支持
 * execCommand 时直接用 `text` 整体回写。
 */

export type TextSelection = { text: string; start: number; end: number };

export type MarkdownCommand =
  | "bold"
  | "italic"
  | "underline"
  | "strike"
  | "ol"
  | "ul"
  | "quote"
  | "link"
  | "image"
  | "table";

export type MarkdownEdit = {
  /** 需要被替换的区间（在原 text 上） */
  start: number;
  end: number;
  /** 写入该区间的内容 */
  insert: string;
  /** 应用后的光标/选区（在新 text 上） */
  selectionStart: number;
  selectionEnd: number;
  /** 应用后的完整文本（给不支持 execCommand 的回退路径与测试用） */
  text: string;
};

export type MarkdownInsertCopy = {
  bold: string;
  italic: string;
  underline: string;
  strike: string;
  link: string;
  image: string;
  tableHeader1: string;
  tableHeader2: string;
};

type WrapCommand = "bold" | "italic" | "underline" | "strike";
type LinePrefixCommand = "ol" | "ul" | "quote";

const WRAP_MARKERS: Record<WrapCommand, { open: string; close: string }> = {
  bold: { open: "**", close: "**" },
  italic: { open: "*", close: "*" },
  underline: { open: "<u>", close: "</u>" },
  strike: { open: "~~", close: "~~" },
};

const LINK_URL_PLACEHOLDER = "https://";

const UL_PREFIX = /^[-*+] /;
const OL_PREFIX = /^\d+\. /;
const QUOTE_PREFIX = /^> /;

export function applyMarkdownCommand(
  selection: TextSelection,
  command: MarkdownCommand,
  copy: MarkdownInsertCopy,
): MarkdownEdit {
  const normalized = normalizeSelection(selection);
  switch (command) {
    case "bold":
    case "italic":
    case "underline":
    case "strike":
      return applyWrap(normalized, command, copy[command]);
    case "ol":
    case "ul":
    case "quote":
      return applyLinePrefix(normalized, command);
    case "link":
      return applyLink(normalized, copy.link);
    case "image":
      return applyImage(normalized, copy.image);
    case "table":
      return applyTable(normalized, copy.tableHeader1, copy.tableHeader2);
  }
}

/** 把 start/end 钳到文本范围内，并保证 start <= end。 */
function normalizeSelection(selection: TextSelection): TextSelection {
  const text = typeof selection.text === "string" ? selection.text : "";
  const clamp = (value: number) => {
    if (!Number.isFinite(value)) return 0;
    return Math.min(Math.max(Math.trunc(value), 0), text.length);
  };
  const a = clamp(selection.start);
  const b = clamp(selection.end);
  return { text, start: Math.min(a, b), end: Math.max(a, b) };
}

function buildEdit(
  text: string,
  start: number,
  end: number,
  insert: string,
  selectionStart: number,
  selectionEnd: number,
): MarkdownEdit {
  return {
    start,
    end,
    insert,
    selectionStart,
    selectionEnd,
    text: text.slice(0, start) + insert + text.slice(end),
  };
}

/** 文本里已经用了 CRLF 时，新插入的换行也跟着用 CRLF，避免混用。 */
function eolOf(text: string): string {
  return text.includes("\r\n") ? "\r\n" : "\n";
}

// ---------------------------------------------------------------------------
// 包裹类：bold / italic / underline / strike
// ---------------------------------------------------------------------------

function countEdge(value: string, char: string, side: "leading" | "trailing"): number {
  let count = 0;
  if (side === "leading") {
    while (count < value.length && value[count] === char) count += 1;
  } else {
    while (count < value.length && value[value.length - 1 - count] === char) count += 1;
  }
  return count;
}

/**
 * 判断 selected 是否已经被该命令的标记包裹（标记在选区内部）。
 * "*" 与 "**" 互为前缀，所以 bold/italic 按星号个数判断：
 * 连续 >=2 个是 bold，奇数个（1 或 3）才含 italic，避免 "**粗体**" 被当成斜体。
 */
function innerWrapped(selected: string, command: WrapCommand): boolean {
  const { open, close } = WRAP_MARKERS[command];
  if (selected.length < open.length + close.length) return false;
  if (command === "bold" || command === "italic") {
    const leading = countEdge(selected, "*", "leading");
    const trailing = countEdge(selected, "*", "trailing");
    const stars = Math.min(leading, trailing);
    // "**a** **b**" 是两段独立的加粗，不是一段被包裹的文字：去掉首尾星号后
    // 中间不能再出现标记，否则会被剥成 "a** **b"。
    if (selected.slice(leading, selected.length - trailing).includes(open)) return false;
    return command === "bold" ? stars >= 2 : stars % 2 === 1;
  }
  if (!selected.startsWith(open) || !selected.endsWith(close)) return false;
  return !selected.slice(open.length, selected.length - close.length).includes(open);
}

/** 判断选区两侧紧邻的文本是否正好是该命令的标记。 */
function outerWrapped(before: string, after: string, command: WrapCommand): boolean {
  const { open, close } = WRAP_MARKERS[command];
  if (command === "bold" || command === "italic") {
    const stars = Math.min(countEdge(before, "*", "trailing"), countEdge(after, "*", "leading"));
    return command === "bold" ? stars >= 2 : stars % 2 === 1;
  }
  return before.endsWith(open) && after.startsWith(close);
}

/** 把选区收缩到非空白范围；选区全是空白时折叠到原选区起点。 */
function trimSelection({ text, start, end }: TextSelection): { start: number; end: number } {
  const selected = text.slice(start, end);
  const lead = selected.match(/^\s*/)?.[0].length ?? 0;
  if (lead === selected.length) return { start, end: start };
  const trail = selected.match(/\s*$/)?.[0].length ?? 0;
  return { start: start + lead, end: end - trail };
}

function applyWrap(selection: TextSelection, command: WrapCommand, placeholder: string): MarkdownEdit {
  const { text } = selection;
  const { open, close } = WRAP_MARKERS[command];
  // 选区首尾的空白留在标记外面：双击选词常带上尾随空格，"**world **" 按
  // CommonMark 侧翼规则不会渲染成加粗。全是空白时视为光标（走占位符分支）。
  const { start, end } = trimSelection(selection);
  const selected = text.slice(start, end);

  if (selected.length > 0 && innerWrapped(selected, command)) {
    // toggle：标记在选区内 → 去掉首尾标记，仍选中中间文字
    const inner = selected.slice(open.length, selected.length - close.length);
    return buildEdit(text, start, end, inner, start, start + inner.length);
  }

  if (selected.length > 0 && outerWrapped(text.slice(0, start), text.slice(end), command)) {
    // toggle：标记紧邻选区两侧 → 连同两侧标记一起替换成原文字
    const outerStart = start - open.length;
    const outerEnd = end + close.length;
    return buildEdit(text, outerStart, outerEnd, selected, outerStart, outerStart + selected.length);
  }

  if (selected.length > 0) {
    const insert = open + selected + close;
    return buildEdit(text, start, end, insert, start + open.length, start + open.length + selected.length);
  }

  const insert = open + placeholder + close;
  return buildEdit(text, start, end, insert, start + open.length, start + open.length + placeholder.length);
}

// ---------------------------------------------------------------------------
// 行前缀类：ol / ul / quote
// ---------------------------------------------------------------------------

type LineParts = { indent: string; body: string; cr: string };

function splitLine(line: string): LineParts {
  const cr = line.endsWith("\r") ? "\r" : "";
  const withoutCr = cr ? line.slice(0, -1) : line;
  const indent = withoutCr.match(/^[ \t]*/)?.[0] ?? "";
  return { indent, body: withoutCr.slice(indent.length), cr };
}

function joinLine(parts: LineParts, body: string): string {
  return parts.indent + body + parts.cr;
}

function prefixPattern(command: LinePrefixCommand): RegExp {
  switch (command) {
    case "ol":
      return OL_PREFIX;
    case "ul":
      return UL_PREFIX;
    case "quote":
      return QUOTE_PREFIX;
  }
}

/** 选区触及的整行区间：[lineStart, lineEnd)，不含行尾换行。 */
function touchedLineRange(text: string, start: number, end: number): { lineStart: number; lineEnd: number } {
  const lineStart = text.lastIndexOf("\n", start - 1) + 1;
  // 选区以换行结束时（例如三击选中整行会把 "\n" 带上），不把下一行算进来
  let searchFrom = end;
  if (end > start && text[end - 1] === "\n") {
    searchFrom = end - 1;
    if (searchFrom > start && text[searchFrom - 1] === "\r") searchFrom -= 1;
  }
  let lineEnd = text.indexOf("\n", searchFrom);
  if (lineEnd === -1) lineEnd = text.length;
  else if (lineEnd > lineStart && text[lineEnd - 1] === "\r") lineEnd -= 1;
  return { lineStart, lineEnd: Math.max(lineEnd, lineStart) };
}

function applyLinePrefix(selection: TextSelection, command: LinePrefixCommand): MarkdownEdit {
  const { text, start, end } = selection;
  const { lineStart, lineEnd } = touchedLineRange(text, start, end);
  const block = text.slice(lineStart, lineEnd);
  const lines = block.split("\n").map(splitLine);
  const pattern = prefixPattern(command);

  // 空行不参与「是否全部已有前缀」的判断，也不会被加上前缀（避免产生空的列表项）；
  // 但若整段只有空行（例如空文本上直接点按钮），就给第一行加上，让用户能直接开始输入。
  const contentLines = lines.filter((line) => line.body.trim() !== "");
  const targets = contentLines.length > 0 ? contentLines : lines.slice(0, 1);
  const allPrefixed = contentLines.length > 0 && contentLines.every((line) => pattern.test(line.body));

  let counter = 0;
  const result = lines
    .map((line) => {
      if (!targets.includes(line)) return joinLine(line, line.body);
      if (allPrefixed) return joinLine(line, line.body.replace(pattern, ""));
      switch (command) {
        case "ol": {
          // 有序/无序互斥：给已是无序列表的行编号时先去掉 "- "，并从 1 重新编号
          counter += 1;
          const body = line.body.replace(OL_PREFIX, "").replace(UL_PREFIX, "");
          return joinLine(line, `${counter}. ${body}`);
        }
        case "ul": {
          if (UL_PREFIX.test(line.body)) return joinLine(line, line.body);
          return joinLine(line, `- ${line.body.replace(OL_PREFIX, "")}`);
        }
        case "quote": {
          if (QUOTE_PREFIX.test(line.body)) return joinLine(line, line.body);
          return joinLine(line, `> ${line.body}`);
        }
      }
    })
    .join("\n");

  return buildEdit(text, lineStart, lineEnd, result, lineStart, lineStart + result.length);
}

// ---------------------------------------------------------------------------
// link / image / table
// ---------------------------------------------------------------------------

function applyLink(selection: TextSelection, placeholder: string): MarkdownEdit {
  const { text, start, end } = selection;
  const selected = text.slice(start, end);
  if (selected.length > 0) {
    const insert = `[${selected}](${LINK_URL_PLACEHOLDER})`;
    const urlStart = start + 1 + selected.length + 2;
    return buildEdit(text, start, end, insert, urlStart, urlStart + LINK_URL_PLACEHOLDER.length);
  }
  const insert = `[${placeholder}](${LINK_URL_PLACEHOLDER})`;
  return buildEdit(text, start, end, insert, start + 1, start + 1 + placeholder.length);
}

function applyImage(selection: TextSelection, placeholder: string): MarkdownEdit {
  const { text, start, end } = selection;
  // 与 link 一致：选中的文字作为图片描述，而不是被丢掉。
  const alt = text.slice(start, end).trim() || placeholder;
  const atLineStart = start === 0 || text[start - 1] === "\n";
  const prefix = atLineStart ? "" : eolOf(text);
  const insert = `${prefix}![${alt}](${LINK_URL_PLACEHOLDER})`;
  const urlStart = start + prefix.length + 2 + alt.length + 2;
  return buildEdit(text, start, end, insert, urlStart, urlStart + LINK_URL_PLACEHOLDER.length);
}

/** 需要补多少个换行才能让 before 之后出现一个空行（before 为空时视为文首，不补）。 */
function newlinesNeededBefore(before: string): number {
  if (before.length === 0) return 0;
  const normalized = before.replace(/\r\n/g, "\n");
  if (normalized.endsWith("\n\n")) return 0;
  if (normalized.endsWith("\n")) return 1;
  return 2;
}

/** 需要补多少个换行才能让 after 之前出现一个空行（after 为空时只补一个，让光标能移到表格下方）。 */
function newlinesNeededAfter(after: string): number {
  if (after.length === 0) return 1;
  const normalized = after.replace(/\r\n/g, "\n");
  if (normalized.startsWith("\n\n")) return 0;
  if (normalized.startsWith("\n")) return 1;
  return 2;
}

function applyTable(
  selection: TextSelection,
  firstHeader: string,
  secondHeader: string,
): MarkdownEdit {
  const { text, start, end } = selection;
  const eol = eolOf(text);
  const prefix = eol.repeat(newlinesNeededBefore(text.slice(0, start)));
  const suffix = eol.repeat(newlinesNeededAfter(text.slice(end)));
  const rows = [`| ${firstHeader} | ${secondHeader} |`, "| --- | --- |", "|  |  |"];
  const insert = prefix + rows.join(eol) + suffix;
  const headerStart = start + prefix.length + 2;
  return buildEdit(text, start, end, insert, headerStart, headerStart + firstHeader.length);
}

// ---------------------------------------------------------------------------
// 图片插入表单（动作条「图片」）
// ---------------------------------------------------------------------------

export const IMAGE_WIDTH_PRESETS: readonly number[] = [25, 50, 75, 100];

export type ImageInsertOptions = {
  url: string;
  alt: string;
  /** 相对正文宽度的百分比；null 表示按图片原始尺寸 */
  widthPercent: number | null;
};

/** 表单只接受 http(s) 链接：data:/file: 一类的地址收件人客户端大多不显示。 */
export function isHttpUrl(url: string): boolean {
  return /^https?:\/\//i.test(url.trim());
}

function clampWidthPercent(value: number): number {
  return Math.min(Math.max(Math.round(value), 1), 100);
}

/**
 * 「自定义 %」输入框的值是否可以直接采用：空（未启用自定义）算合法；否则必须是
 * 1..100 之间的有限数。表单据此提示并禁用「插入」，而不是让 normalizeImageWidthPercent
 * 静默把 150 夹成 100。
 */
export function isImageWidthPercentInRange(input: string): boolean {
  const trimmed = input.trim();
  if (!trimmed) return true;
  const value = Number(trimmed);
  return Number.isFinite(value) && value >= 1 && value <= 100;
}

/** 「自定义 %」输入框的值：空/非数字 → null（视为原始尺寸）；否则取整并夹到 1..100（兜底，表单已先按 isImageWidthPercentInRange 拦住越界值）。 */
export function normalizeImageWidthPercent(input: string): number | null {
  const trimmed = input.trim();
  if (!trimmed) return null;
  const value = Number(trimmed);
  if (!Number.isFinite(value)) return null;
  return clampWidthPercent(value);
}

function escapeHtmlAttribute(value: string): string {
  return value
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;");
}

/**
 * 图片始终输出内联 HTML 而不是 `![]()`：Markdown 图片语法没有尺寸，而收件人
 * 客户端不会带我们预览里的 `img{max-width:100%}`，大图会撑爆邮件；`width="N%"`
 * 是最多邮件客户端认的尺寸写法，`max-width:100%;height:auto` 兜住超宽与比例。
 * micromark 开着 allowDangerousHtml，这段标记原样进入派生 html。
 */
export function imageMarkup({ url, alt, widthPercent }: ImageInsertOptions): string {
  const width = widthPercent === null ? "" : ` width="${clampWidthPercent(widthPercent)}%"`;
  return `<img src="${escapeHtmlAttribute(url.trim())}" alt="${escapeHtmlAttribute(alt.trim())}"${width} style="max-width:100%;height:auto">`;
}

/**
 * 图片标记独占一行：光标不在行首先补换行；标记之后保证有一个空行——单独成行的
 * `<img>` 在 CommonMark 里是 HTML 块（类型 7），会把后面直到空行为止的所有行
 * 原样吞掉，下一行的 `**加粗**` 就不再渲染。替换选区，新光标落在插入内容之后。
 */
export function insertImageAtSelection(selection: TextSelection, options: ImageInsertOptions): MarkdownEdit {
  const { text, start, end } = normalizeSelection(selection);
  const eol = eolOf(text);
  const atLineStart = start === 0 || text[start - 1] === "\n";
  const prefix = atLineStart ? "" : eol;
  const suffix = eol.repeat(blankLineNeededAfter(text.slice(end)));
  const insert = prefix + imageMarkup(options) + suffix;
  const caret = start + insert.length;
  return buildEdit(text, start, end, insert, caret, caret);
}

/** 需要补多少个换行才能让 after 之前出现一个空行（after 为空时也补足，让接着输入的内容不落进 HTML 块）。 */
function blankLineNeededAfter(after: string): number {
  const normalized = after.replace(/\r\n/g, "\n");
  if (normalized.startsWith("\n\n")) return 0;
  if (normalized.startsWith("\n")) return 1;
  return 2;
}

// ---------------------------------------------------------------------------
// 纯插入（模板编辑器「插入变量」）
// ---------------------------------------------------------------------------

/** 把选区替换为 text，光标落在插入文本之后。 */
export function insertTextAtSelection(selection: TextSelection, text: string): MarkdownEdit {
  const { text: source, start, end } = normalizeSelection(selection);
  const caret = start + text.length;
  return buildEdit(source, start, end, text, caret, caret);
}
