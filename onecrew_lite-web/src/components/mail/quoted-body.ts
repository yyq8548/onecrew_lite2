/**
 * Split a plain-text email body into what this message actually says and the
 * history it quotes.
 *
 * A reply carries the whole conversation again. Rendering that as one block of
 * text makes the new sentences indistinguishable from the ones already read,
 * which is exactly what every mail client avoids by folding the quote away.
 *
 * Detection is deliberately conservative: the quote is only recognised when it
 * runs to the end of the body, so an inline `>` in the middle of a sentence
 * never truncates a message. Nothing is discarded — the quote is returned in
 * full and the caller decides how to reveal it.
 */
export type QuotedBody = {
  /** What this message adds, with the attribution line removed. */
  body: string;
  /** The line introducing the quote ("On … wrote:"), when the body has one. */
  attribution: string | null;
  /** The quoted history, verbatim, still carrying its `>` markers. */
  quote: string | null;
};

const QUOTE_MARKER = /^\s*>/;
/** Separators mail clients write instead of, or above, a `>` quote. */
const SEPARATOR = /^\s*(-{2,}\s*Original Message\s*-{2,}|_{5,}|-{5,}|发件人[:：]|原始邮件)\s*$/i;
/** "On <date>, <person> wrote:" and its localised forms. */
const ATTRIBUTION = /(wrote:|写道[:：]|於.*寫道[:：]|schrieb:|a écrit\s*:)\s*$/i;

function isQuoteLine(line: string) {
  return QUOTE_MARKER.test(line) || SEPARATOR.test(line);
}

/**
 * Find the first line from which the rest of the body is quoted history.
 * Blank lines inside the run are allowed; anything else ends the candidate.
 */
function quoteStart(lines: string[]): number | null {
  let candidate: number | null = null;
  for (let index = lines.length - 1; index >= 0; index -= 1) {
    const line = lines[index];
    if (isQuoteLine(line)) {
      candidate = index;
      continue;
    }
    if (candidate !== null && line.trim() === "") continue;
    break;
  }
  return candidate;
}

export function splitQuotedBody(text: string | null | undefined): QuotedBody {
  const source = text ?? "";
  const lines = source.split("\n");
  const start = quoteStart(lines);
  if (start === null) return { body: source.trimEnd(), attribution: null, quote: null };

  let bodyEnd = start;
  let attribution: string | null = null;
  // An attribution line belongs to the quote it introduces, not to the reply.
  for (let index = start - 1; index >= 0; index -= 1) {
    const line = lines[index];
    if (line.trim() === "") {
      bodyEnd = index;
      continue;
    }
    if (ATTRIBUTION.test(line)) {
      attribution = line.trim();
      bodyEnd = index;
    }
    break;
  }

  const quote = lines.slice(start).join("\n").trimEnd();
  return {
    body: lines.slice(0, bodyEnd).join("\n").trimEnd(),
    attribution,
    quote: quote === "" ? null : quote,
  };
}

/**
 * Strip one level of `>` markers so the quote can be shown as an indented
 * block rather than as punctuation. Deeper levels keep their remaining markers,
 * which is what preserves nesting.
 */
export function unquote(quote: string): string {
  return quote
    .split("\n")
    .map((line) => line.replace(/^\s?>\s?/, ""))
    .join("\n")
    .trim();
}

/** A one-line summary for the collapsed quote's trigger. */
export function quoteSummary(quoted: QuotedBody, fallback: string): string {
  if (quoted.attribution) return quoted.attribution;
  const first = unquote(quoted.quote ?? "")
    .split("\n")
    .map((line) => line.trim())
    .find((line) => line !== "");
  return first ?? fallback;
}

/** 历史里的一轮:抬头(谁在什么时候写的)加上这一轮说的话。 */
export type ChainTurn = {
  /** 客户端写的 From/Date/Subject/To 抬头块,解析成字段。 */
  header: Record<string, string> | null;
  /** "On … wrote:" 那种单行抬头,没有抬头块时用它。 */
  attribution: string | null;
  /** 这一轮自己说的话,不含它再往下引用的部分。 */
  body: string;
};

/** 抬头块的字段名,中英文都认——不同客户端各写各的。 */
const HEADER_FIELD = /^\s*(From|Date|Subject|To|Cc|Sent|发件人|日期|主题|收件人|抄送)\s*[:：]\s*(.*)$/i;

/** 一层最多认这么多行抬头,免得把正文吃进去。 */
const MAX_HEADER_LINES = 8;
/** 递归上限:一条真实线程不会有这么多轮,超过说明解析跑偏了。 */
const MAX_TURNS = 24;

function takeHeaderBlock(text: string): { header: Record<string, string> | null; body: string } {
  const lines = text.split("\n");
  const header: Record<string, string> = {};
  let index = 0;
  while (index < lines.length && index < MAX_HEADER_LINES) {
    const match = HEADER_FIELD.exec(lines[index]);
    if (!match) break;
    header[match[1]] = match[2].trim();
    index += 1;
  }
  if (index === 0) return { header: null, body: text };
  return { header, body: lines.slice(index).join("\n").trimStart() };
}

/**
 * 把一封回复拆成一轮轮:最新的在前,越往后越旧。
 *
 * 邮件回复是累积的——每一轮把上一轮整段带下来,再加一层 `>`。原样把
 * 这段文本倒出来,读者看到的是一堵 `>` 组成的墙;真正的客户端把每一轮
 * 还原成「谁在什么时候说了什么」。这个函数做的就是还原,渲染只管画。
 */
export function parseEmailChain(text: string | null | undefined): ChainTurn[] {
  const turns: ChainTurn[] = [];
  let current = (text ?? "").trim();
  let attribution: string | null = null;
  for (let depth = 0; depth < MAX_TURNS; depth += 1) {
    const split = splitQuotedBody(current);
    // 第一轮是这封信自己说的话,没有抬头;更深的每一轮都带着上一层
    // 留下的属性行或自己的抬头块。
    const { header, body } = depth === 0
      ? { header: null, body: split.body }
      : takeHeaderBlock(split.body);
    if (header || attribution || body.trim() || depth === 0) {
      turns.push({ header, attribution, body: body.trim() });
    }
    if (!split.quote) break;
    attribution = split.attribution;
    current = unquote(split.quote);
  }
  return turns;
}
