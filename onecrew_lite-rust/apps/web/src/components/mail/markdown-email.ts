import { micromark } from "micromark";
import { gfm, gfmHtml } from "micromark-extension-gfm";

/**
 * The single Markdown → email HTML conversion point. Preview and draft saving
 * both go through `markdownToEmailHtml`, so a saved draft's `html` is exactly
 * what the preview showed and what the send path delivers.
 */

export type EmailBodyStyle = {
  fontFamily: string | null;
  fontSize: number | null;
};

export const DEFAULT_EMAIL_BODY_STYLE: EmailBodyStyle = {
  fontFamily: null,
  fontSize: null,
};

export const EMAIL_FONT_OPTIONS: ReadonlyArray<{
  value: string;
  family: string | null;
}> = [
  { value: "system", family: null },
  { value: "heiti", family: "SimHei, sans-serif" },
  { value: "songti", family: "SimSun, serif" },
  { value: "nsimsun", family: "NSimSun, serif" },
  { value: "kaiti", family: "KaiTi, serif" },
  { value: "lishu", family: "LiSu, serif" },
  { value: "youyuan", family: "YouYuan, sans-serif" },
  { value: "yahei", family: "Microsoft YaHei, sans-serif" },
  { value: "arial", family: "Arial, sans-serif" },
];

export const EMAIL_FONT_SIZES: readonly number[] = [12, 13, 14, 15, 16, 18, 20, 24];

export function emailFontOptionForFamily(
  family: string | null,
): (typeof EMAIL_FONT_OPTIONS)[number] {
  const normalized = family?.trim() ?? null;
  return EMAIL_FONT_OPTIONS.find((option) => option.family === normalized)
    ?? EMAIL_FONT_OPTIONS[0];
}

const BODY_WRAPPER_ATTR = "data-mail-body";
/**
 * CommonMark: a backtick fence's info string may not contain backticks, so a
 * line like "```js```" is an inline code span, not a fence. Tilde fences allow
 * anything in the info string.
 */
const FENCE_OPEN = /^ {0,3}(?:(`{3,})(?!.*`)|(~{3,}))/;

function normalizeLineEndings(source: string): string {
  return source.replace(/\r\n?/g, "\n");
}

/**
 * Walks lines while tracking fenced code blocks so callers can leave code
 * content untouched. `onFenceLine` receives the opening/closing fence lines.
 */
function mapLinesOutsideFences(
  source: string,
  onLine: (line: string) => string | null,
  onFenceLine: (line: string) => string | null,
  onCodeLine: (line: string) => string | null = (line) => line,
): string {
  const out: string[] = [];
  let fence: { char: string; length: number } | null = null;
  for (const line of normalizeLineEndings(source).split("\n")) {
    let next: string | null;
    if (fence) {
      const close = line.match(/^ {0,3}(`{3,}|~{3,})\s*$/);
      if (close && close[1][0] === fence.char && close[1].length >= fence.length) {
        fence = null;
        next = onFenceLine(line);
      } else {
        next = onCodeLine(line);
      }
    } else {
      const open = line.match(FENCE_OPEN);
      if (open) {
        const marker = open[1] ?? open[2];
        fence = { char: marker[0], length: marker.length };
        next = onFenceLine(line);
      } else {
        next = onLine(line);
      }
    }
    if (next !== null) out.push(next);
  }
  return out.join("\n");
}

/**
 * CommonMark only turns a line ending into `<br />` when the line ends with two
 * spaces (or a backslash). Mail authors expect a single Enter to be a line
 * break, so every non-empty line outside fenced code gets a trailing two-space
 * hard break. A hard break at the end of a block is ignored by the parser, so
 * headings, list items and table rows are unaffected.
 *
 * Only fenced code is protected: lines inside indented code blocks and HTML
 * blocks also receive the two trailing spaces (visible as trailing whitespace
 * in the rendered `<pre>` / raw html). Mail authors write fences, and telling
 * an indented code block apart from a list item's continuation paragraph
 * needs a full block parser, so this is documented rather than handled.
 */
function withHardLineBreaks(markdown: string): string {
  return mapLinesOutsideFences(
    markdown,
    (line) => {
      if (!line.trim()) return line;
      if (/( {2,}|\\)$/.test(line)) return line;
      return `${line}  `;
    },
    (line) => line,
  );
}

function escapeAttribute(value: string): string {
  return value
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;");
}

function unescapeAttribute(value: string): string {
  return value
    .replace(/&quot;/g, "\"")
    .replace(/&#39;/g, "'")
    .replace(/&lt;/g, "<")
    .replace(/&gt;/g, ">")
    .replace(/&amp;/g, "&");
}

function bodyStyleAttribute(style: EmailBodyStyle): string {
  const declarations: string[] = [];
  if (style.fontFamily && style.fontFamily.trim()) {
    declarations.push(`font-family: ${style.fontFamily.trim()}`);
  }
  if (style.fontSize !== null && Number.isFinite(style.fontSize) && style.fontSize > 0) {
    declarations.push(`font-size: ${style.fontSize}px`);
  }
  if (declarations.length === 0) return "";
  return ` style="${escapeAttribute(declarations.join("; "))}"`;
}

/**
 * Inline HTML passes through micromark untouched, so `<a href="javascript:…">`
 * would survive. Best-effort scrub of script protocols on href/src attributes
 * of raw tags (markdown links/images/autolinks are already neutralised by
 * micromark). Not a sanitizer: the preview relies on the iframe sandbox.
 */
const SCRIPT_PROTOCOL_ATTR = /(<[a-z][^>]*\s(?:href|src)\s*=\s*["']?)\s*(?:javascript|vbscript):/gi;

function stripScriptProtocols(html: string): string {
  return html.replace(SCRIPT_PROTOCOL_ATTR, "$1");
}

/**
 * Returns `null` for an empty body. Inline HTML such as `<u>` is allowed
 * through (the format bar emits it). Markdown link/image/autolink targets with
 * dangerous protocols are neutralised by micromark (`allowDangerousProtocol`
 * stays off); `javascript:`/`vbscript:` on raw-html href/src attributes are
 * scrubbed by `stripScriptProtocols`. The result is meant for sending and for
 * the sandboxed preview iframe, never for `innerHTML` on the app document.
 */
export function markdownToEmailHtml(
  markdown: string,
  style: EmailBodyStyle = DEFAULT_EMAIL_BODY_STYLE,
): string | null {
  // Trim so the derived html matches what a saved draft (which stores trimmed
  // text) derives again on reopen; only a leading indented-code line is lost.
  const source = markdown.trim();
  if (!source) return null;
  const body = micromark(withHardLineBreaks(source), {
    extensions: [gfm()],
    htmlExtensions: [gfmHtml()],
    allowDangerousHtml: true,
  });
  return `<div ${BODY_WRAPPER_ATTR}${bodyStyleAttribute(style)}>${stripScriptProtocols(body)}</div>`;
}

const BODY_WRAPPER_PATTERN = /^\s*<div data-mail-body(?:\s+style="([^"]*)")?\s*>/;

/**
 * True when `html` was produced by `markdownToEmailHtml` (the `data-mail-body`
 * wrapper is the marker). Used to tell a Markdown-derived body apart from
 * template-authored html without re-deriving and comparing byte-for-byte,
 * which would break on any converter change.
 */
export function isDerivedEmailHtml(html: string | null): boolean {
  return html !== null && BODY_WRAPPER_PATTERN.test(html) && /<\/div>\s*$/.test(html);
}

export function readEmailBodyStyle(html: string | null): EmailBodyStyle {
  if (!html) return { ...DEFAULT_EMAIL_BODY_STYLE };
  const wrapper = html.match(BODY_WRAPPER_PATTERN);
  if (!wrapper) return { ...DEFAULT_EMAIL_BODY_STYLE };
  const style: EmailBodyStyle = { ...DEFAULT_EMAIL_BODY_STYLE };
  const declarations = unescapeAttribute(wrapper[1] ?? "");
  for (const declaration of declarations.split(";")) {
    const separator = declaration.indexOf(":");
    if (separator === -1) continue;
    const property = declaration.slice(0, separator).trim().toLowerCase();
    const value = declaration.slice(separator + 1).trim();
    if (!value) continue;
    if (property === "font-family") {
      style.fontFamily = value;
    } else if (property === "font-size") {
      const size = value.match(/^(\d+(?:\.\d+)?)px$/);
      if (size) style.fontSize = Number(size[1]);
    }
  }
  return style;
}

const RICH_MARKDOWN_PATTERNS: readonly RegExp[] = [
  // Emphasis markers must hug the text on the inside (CommonMark flanking):
  // "5 * 3 * 2" is arithmetic, not italics. Written without lookbehind on
  // purpose: WKWebView on older macOS lacks it and a regex literal that fails
  // to parse would take the whole composer module down at load time.
  /\*\*(?:[^\s*\n]|[^\s*\n][^*\n]*?[^\s*\n])\*\*/, // bold
  /(?:^|[^*\n])\*(?:[^\s*\n]|[^\s*\n][^*\n]*?[^\s*\n])\*(?!\*)/, // italic
  /~~(?:[^\s~\n]|[^\s~\n][^~\n]*?[^\s~\n])~~/, // strikethrough
  /<u>|<\/u>/i, // underline (inline html emitted by the format bar)
  /!\[/, // image
  /<img\b/i, // image emitted by the composer's image form (inline html with a width)
  /\[[^\]\n]*\]\(/, // link
  /^ {0,3}[-*]\s/m, // bullet list
  /^ {0,3}\d+\.\s/m, // ordered list
  /^ {0,3}>/m, // blockquote
  /^ {0,3}#{1,6}(?:\s|$)/m, // heading
  /\|\s*:?-{3,}:?\s*\|/, // table delimiter row
  /`[^`\n]+`/, // inline code
  /^ {0,3}(?:`{3,}|~{3,})/m, // fenced code
];

/** True when converting the body to plain text would lose visible formatting. */
export function hasRichMarkdown(markdown: string): boolean {
  const source = normalizeLineEndings(markdown);
  return RICH_MARKDOWN_PATTERNS.some((pattern) => pattern.test(source));
}

const LIST_PREFIX = /^(\s*(?:[-*+]|\d+\.)\s+)/;

/**
 * `<img … src=URL …>` in any attribute order, quoted with " or ' or bare,
 * self-closing or not. The whole tag degrades to its (unescaped) url.
 */
const HTML_IMG_TAG = /<img\b[^>]*?\ssrc\s*=\s*(?:"([^"]*)"|'([^']*)'|([^\s>]+))[^>]*>/gi;

function stripInlineMarkdown(text: string): string {
  return text
    .replace(HTML_IMG_TAG, (_match, doubleQuoted?: string, singleQuoted?: string, bare?: string) =>
      unescapeAttribute(doubleQuoted ?? singleQuoted ?? bare ?? ""))
    .replace(/!\[[^\]]*\]\(([^)\s]+)(?:\s+"[^"]*")?\)/g, "$1")
    .replace(/\[([^\]]+)\]\(([^)\s]+)(?:\s+"[^"]*")?\)/g, "$1 ($2)")
    .replace(/<\/?u>/gi, "")
    // Same flanking rule as hasRichMarkdown: literal " * " stays as written,
    // and a single "*" never pairs across a "**" run ("** not bold **").
    .replace(/\*\*(\S|\S.*?\S)\*\*/g, "$1")
    .replace(/\*([^\s*]|[^\s*].*?[^\s*])\*/g, "$1")
    .replace(/~~(\S|\S.*?\S)~~/g, "$1")
    // Code spans may use any run of backticks ("```js```" is inline code).
    .replace(/(`+)(.+?)\1/g, "$2");
}

/**
 * Best-effort Markdown → plain text for the plain-text mode switch. List
 * prefixes and table rows stay as written; fenced code keeps its content and
 * loses only the fence lines.
 */
export function stripMarkdownForPlainText(markdown: string): string {
  return mapLinesOutsideFences(
    markdown,
    (line) => {
      let rest = line.replace(/^ {0,3}(?:>\s?)+/, "");
      rest = rest.replace(/^ {0,3}#{1,6}\s+/, "");
      const prefix = rest.match(LIST_PREFIX);
      if (prefix) {
        return prefix[1] + stripInlineMarkdown(rest.slice(prefix[1].length));
      }
      return stripInlineMarkdown(rest);
    },
    () => null,
  );
}
