import { describe, expect, it } from "vitest";

import {
  DEFAULT_EMAIL_BODY_STYLE,
  EMAIL_FONT_OPTIONS,
  EMAIL_FONT_SIZES,
  emailFontOptionForFamily,
  hasRichMarkdown,
  isDerivedEmailHtml,
  markdownToEmailHtml,
  readEmailBodyStyle,
  stripMarkdownForPlainText,
} from "@/components/mail/markdown-email";

describe("markdownToEmailHtml", () => {
  it("returns null for an empty body and wraps everything else in the body div", () => {
    expect(markdownToEmailHtml("")).toBeNull();
    expect(markdownToEmailHtml("  \n\t\n")).toBeNull();
    const html = markdownToEmailHtml("Hello");
    expect(html).toMatch(/^<div data-mail-body><p>Hello<\/p>\n?<\/div>$/);
  });

  it("turns a single Enter into a hard break outside fenced code", () => {
    const html = markdownToEmailHtml("line one\nline two")!;
    expect(html).toContain("<p>line one<br />\nline two</p>");

    const fenced = markdownToEmailHtml("```js\nconst a = 1;\nconst b = 2;\n```")!;
    expect(fenced).toContain("<pre><code class=\"language-js\">const a = 1;\nconst b = 2;\n</code></pre>");
    expect(fenced).not.toContain("<br />");

    const tilde = markdownToEmailHtml("~~~\nraw\n~~~\nafter\nmore")!;
    expect(tilde).toContain("<pre><code>raw\n</code></pre>");
    expect(tilde).toContain("<p>after<br />\nmore</p>");
  });

  it("does not mistake an inline code span written with three backticks for a fence", () => {
    const html = markdownToEmailHtml("```js```\nnext\nline")!;
    expect(html).toContain("<code>js</code>");
    expect(html).toContain("next<br />\nline");
    expect(stripMarkdownForPlainText("```js```\nnext")).toBe("js\nnext");
  });

  it("does not add breaks to block ends such as headings, list items and table rows", () => {
    const html = markdownToEmailHtml("# Title\n- a\n- b\n\n| h | k |\n|---|---|\n| 1 | 2 |")!;
    expect(html).toContain("<h1>Title</h1>");
    expect(html).toContain("<li>a</li>");
    expect(html).toContain("<li>b</li>");
    expect(html).toContain("<th>h</th>");
    expect(html).toContain("<td>2</td>");
    expect(html).not.toContain("<br />");
  });

  it("renders GFM strikethrough and autolinks", () => {
    const html = markdownToEmailHtml("~~gone~~ see https://example.com")!;
    expect(html).toContain("<del>gone</del>");
    expect(html).toContain("<a href=\"https://example.com\">https://example.com</a>");
  });

  it("lets inline <u> through and survives script tags without throwing", () => {
    expect(markdownToEmailHtml("<u>under</u>")).toContain("<u>under</u>");
    // Script content is send-side content; the preview renders in a sandboxed
    // iframe, so here we only assert the converter does not crash on it.
    expect(() => markdownToEmailHtml("<script>alert(1)</script>\nnext")).not.toThrow();
    expect(markdownToEmailHtml("<script>alert(1)</script>\nnext")).toContain("alert(1)");
  });

  it("neutralises javascript: links", () => {
    const html = markdownToEmailHtml("[x](javascript:alert(1))")!;
    expect(html).not.toContain("javascript:");
    expect(html).toContain("<a href=\"\">x</a>");
  });

  it("scrubs script protocols from raw html href/src attributes", () => {
    const anchor = markdownToEmailHtml("<a href=\"javascript:alert(1)\">x</a>")!;
    expect(anchor).not.toContain("javascript:");
    expect(anchor).toContain("<a href=\"alert(1)\">x</a>");
    const image = markdownToEmailHtml("<img src='JavaScript:alert(1)' alt=\"i\">")!;
    expect(image).not.toMatch(/javascript:/i);
    expect(markdownToEmailHtml("<a href=\"https://example.com\">ok</a>")).toContain("href=\"https://example.com\"");
    expect(markdownToEmailHtml("say javascript: is fine in prose")).toContain("javascript: is fine");
  });

  it("recognises html it derived itself by the wrapper marker", () => {
    expect(isDerivedEmailHtml(null)).toBe(false);
    expect(isDerivedEmailHtml("<p>template</p>")).toBe(false);
    expect(isDerivedEmailHtml(markdownToEmailHtml("Hi"))).toBe(true);
    expect(isDerivedEmailHtml(markdownToEmailHtml("Hi", { fontFamily: "SimSun, serif", fontSize: 14 }))).toBe(true);
    expect(isDerivedEmailHtml("<div data-mail-body><p>x</p></div>\n")).toBe(true);
  });

  it("writes font-family and font-size on the wrapper and reads them back", () => {
    const style = { fontFamily: "Microsoft YaHei, sans-serif", fontSize: 16 };
    const html = markdownToEmailHtml("Hi", style)!;
    expect(html.startsWith("<div data-mail-body style=\"font-family: Microsoft YaHei, sans-serif; font-size: 16px\">")).toBe(true);
    expect(readEmailBodyStyle(html)).toEqual(style);

    const familyOnly = { fontFamily: "SimSun, serif", fontSize: null };
    expect(readEmailBodyStyle(markdownToEmailHtml("Hi", familyOnly))).toEqual(familyOnly);

    const sizeOnly = { fontFamily: null, fontSize: 13 };
    const sizeHtml = markdownToEmailHtml("Hi", sizeOnly)!;
    expect(sizeHtml).toContain("style=\"font-size: 13px\"");
    expect(sizeHtml).not.toContain("font-family");
    expect(readEmailBodyStyle(sizeHtml)).toEqual(sizeOnly);

    const none = markdownToEmailHtml("Hi", DEFAULT_EMAIL_BODY_STYLE)!;
    expect(none).not.toContain("style=");
    expect(readEmailBodyStyle(none)).toEqual(DEFAULT_EMAIL_BODY_STYLE);
  });

  it("escapes quotes inside the style attribute and restores them on read", () => {
    const style = { fontFamily: "\"Noto Sans SC\", sans-serif", fontSize: 14 };
    const html = markdownToEmailHtml("Hi", style)!;
    expect(html).toContain("font-family: &quot;Noto Sans SC&quot;, sans-serif");
    expect(readEmailBodyStyle(html)).toEqual(style);
  });

  it("derives the same html for the trimmed text a saved draft stores", () => {
    const style = { fontFamily: "SimHei, sans-serif", fontSize: 15 };
    const text = "\n\nHello\nworld  \n\n";
    expect(markdownToEmailHtml(text, style)).toBe(markdownToEmailHtml(text.trim(), style));
  });
});

describe("readEmailBodyStyle", () => {
  it("returns defaults without a wrapper or without a style", () => {
    expect(readEmailBodyStyle(null)).toEqual(DEFAULT_EMAIL_BODY_STYLE);
    expect(readEmailBodyStyle("<p>legacy template</p>")).toEqual(DEFAULT_EMAIL_BODY_STYLE);
    expect(readEmailBodyStyle("<div data-mail-body><p>x</p></div>")).toEqual(DEFAULT_EMAIL_BODY_STYLE);
    expect(readEmailBodyStyle("<div data-mail-body style=\"color: red\"><p>x</p></div>"))
      .toEqual(DEFAULT_EMAIL_BODY_STYLE);
  });
});

describe("font options", () => {
  it("resolves a family back to its option and falls back to the system default", () => {
    expect(emailFontOptionForFamily(null).value).toBe("system");
    expect(emailFontOptionForFamily("KaiTi, serif").value).toBe("kaiti");
    expect(emailFontOptionForFamily("Comic Sans MS").value).toBe("system");
    expect(EMAIL_FONT_OPTIONS.map((option) => option.value)).toEqual([
      "system", "heiti", "songti", "nsimsun", "kaiti", "lishu", "youyuan", "yahei", "arial",
    ]);
    expect(EMAIL_FONT_SIZES).toEqual([12, 13, 14, 15, 16, 18, 20, 24]);
  });
});

describe("hasRichMarkdown", () => {
  it("detects every formatting construct the composer can emit", () => {
    for (const sample of [
      "some **bold** text",
      "some *italic* text",
      "some ~~struck~~ text",
      "some <u>underlined</u> text",
      "![alt](https://example.com/a.png)",
      "<img src=\"https://example.com/a.png\" alt=\"\" width=\"50%\" style=\"max-width:100%;height:auto\">",
      "<IMG SRC='https://example.com/a.png'>",
      "[link](https://example.com)",
      "- bullet",
      "* bullet",
      "1. numbered",
      "> quoted",
      "# heading",
      "intro\n## heading",
      "| a | b |\n|---|---|\n| 1 | 2 |",
      "run `npm test` now",
      "```\ncode\n```",
    ]) {
      expect(hasRichMarkdown(sample), sample).toBe(true);
    }
  });

  it("stays false for plain prose", () => {
    for (const sample of [
      "",
      "Hello, world.",
      "Two lines\nof plain text",
      "Price is 5 * 3 = 15",
      "a * b * c",
      "5 * 3 * 2 = 30",
      "** not bold **",
      "issue #12 is fixed",
      "a | b | c",
      "email me at a@b.com",
    ]) {
      expect(hasRichMarkdown(sample), sample).toBe(false);
    }
  });
});

describe("stripMarkdownForPlainText", () => {
  it("rewrites images and links to their urls", () => {
    expect(stripMarkdownForPlainText("![logo](https://example.com/logo.png)"))
      .toBe("https://example.com/logo.png");
    expect(stripMarkdownForPlainText("see [docs](https://example.com/docs) now"))
      .toBe("see docs (https://example.com/docs) now");
  });

  it("degrades html img tags to their url whatever the attribute order or quoting", () => {
    expect(stripMarkdownForPlainText(
      "<img src=\"https://example.com/a.png\" alt=\"图\" width=\"50%\" style=\"max-width:100%;height:auto\">",
    )).toBe("https://example.com/a.png");
    expect(stripMarkdownForPlainText("<img alt='x' width='25%' src='https://example.com/b.png' />"))
      .toBe("https://example.com/b.png");
    expect(stripMarkdownForPlainText("<IMG style=\"max-width:100%\" SRC=https://example.com/c.png>"))
      .toBe("https://example.com/c.png");
    expect(stripMarkdownForPlainText("<img src=\"https://example.com/?a=1&amp;b=2\" alt=\"\">"))
      .toBe("https://example.com/?a=1&b=2");
    expect(stripMarkdownForPlainText("before <img src=\"https://example.com/a.png\" alt=\"\"> after"))
      .toBe("before https://example.com/a.png after");
    expect(stripMarkdownForPlainText("<img alt=\"no source\">")).toBe("<img alt=\"no source\">");
  });

  it("removes emphasis, underline, inline code, blockquote and heading markers", () => {
    expect(stripMarkdownForPlainText("**bold** *it* ~~gone~~ <u>u</u> `code`"))
      .toBe("bold it gone u code");
    expect(stripMarkdownForPlainText("> quoted line")).toBe("quoted line");
    expect(stripMarkdownForPlainText("## Heading")).toBe("Heading");
    expect(stripMarkdownForPlainText("> # nested")).toBe("nested");
  });

  it("keeps literal asterisks that are not emphasis", () => {
    for (const sample of ["a * b * c", "5 * 3 * 2 = 30", "** not bold **"]) {
      expect(stripMarkdownForPlainText(sample), sample).toBe(sample);
    }
  });

  it("keeps list prefixes and table rows", () => {
    expect(stripMarkdownForPlainText("- **item**\n* other\n1. *third*"))
      .toBe("- item\n* other\n1. third");
    const table = "| a | b |\n|---|---|\n| 1 | 2 |";
    expect(stripMarkdownForPlainText(table)).toBe(table);
  });

  it("drops fence lines and keeps code content untouched", () => {
    expect(stripMarkdownForPlainText("before\n```ts\nconst x = **not bold**;\n```\nafter"))
      .toBe("before\nconst x = **not bold**;\nafter");
  });
});
