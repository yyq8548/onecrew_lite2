import { describe, expect, it } from "vitest";

import {
  applyMarkdownCommand,
  IMAGE_WIDTH_PRESETS,
  imageMarkup,
  insertImageAtSelection,
  insertTextAtSelection,
  isHttpUrl,
  isImageWidthPercentInRange,
  normalizeImageWidthPercent,
  type MarkdownInsertCopy,
  type MarkdownCommand,
  type MarkdownEdit,
  type TextSelection,
} from "@/components/mail/markdown-edit";

const INSERT_COPY: MarkdownInsertCopy = {
  bold: "粗体文字",
  italic: "斜体文字",
  underline: "下划线文字",
  strike: "删除线文字",
  link: "链接文字",
  image: "图片描述",
  tableHeader1: "列 1",
  tableHeader2: "列 2",
};

function run(text: string, start: number, end: number, command: MarkdownCommand): MarkdownEdit {
  return applyMarkdownCommand({ text, start, end }, command, INSERT_COPY);
}

/** 应用后的选区在新文本上实际选中的内容。 */
function selected(edit: MarkdownEdit): string {
  return edit.text.slice(edit.selectionStart, edit.selectionEnd);
}

/** 最小替换必须与完整结果一致（组件层只会用 start/end/insert 做 execCommand）。 */
function expectConsistent(selection: TextSelection, edit: MarkdownEdit) {
  const { text } = selection;
  expect(edit.start).toBeGreaterThanOrEqual(0);
  expect(edit.end).toBeGreaterThanOrEqual(edit.start);
  expect(edit.end).toBeLessThanOrEqual(text.length);
  expect(text.slice(0, edit.start) + edit.insert + text.slice(edit.end)).toBe(edit.text);
  expect(edit.selectionStart).toBeGreaterThanOrEqual(0);
  expect(edit.selectionEnd).toBeGreaterThanOrEqual(edit.selectionStart);
  expect(edit.selectionEnd).toBeLessThanOrEqual(edit.text.length);
}

describe("wrap commands", () => {
  it("bold wraps a selection and keeps the original words selected", () => {
    const edit = run("hello world", 6, 11, "bold");
    expect(edit.text).toBe("hello **world**");
    expect(edit).toMatchObject({ start: 6, end: 11, insert: "**world**" });
    expect(selected(edit)).toBe("world");
    expectConsistent({ text: "hello world", start: 6, end: 11 }, edit);
  });

  it("bold inserts a placeholder when nothing is selected", () => {
    const edit = run("hello ", 6, 6, "bold");
    expect(edit.text).toBe("hello **粗体文字**");
    expect(selected(edit)).toBe("粗体文字");
  });

  it("bold removes markers contained in the selection (toggle)", () => {
    const text = "a **b** c";
    const edit = run(text, 2, 7, "bold");
    expect(edit.text).toBe("a b c");
    expect(edit).toMatchObject({ start: 2, end: 7, insert: "b" });
    expect(selected(edit)).toBe("b");
    expectConsistent({ text, start: 2, end: 7 }, edit);
  });

  it("bold removes markers adjacent to the selection (toggle)", () => {
    const text = "a **b** c";
    const edit = run(text, 4, 5, "bold");
    expect(edit.text).toBe("a b c");
    expect(edit).toMatchObject({ start: 2, end: 7, insert: "b" });
    expect(selected(edit)).toBe("b");
    expectConsistent({ text, start: 4, end: 5 }, edit);
  });

  it("italic wraps with a single asterisk and uses its own placeholder", () => {
    expect(run("x y", 2, 3, "italic").text).toBe("x *y*");
    const empty = run("", 0, 0, "italic");
    expect(empty.text).toBe("*斜体文字*");
    expect(selected(empty)).toBe("斜体文字");
  });

  it("italic does not mistake bold markers for italic ones", () => {
    const bolded = run("**b**", 0, 5, "italic");
    expect(bolded.text).toBe("***b***");
    expect(selected(bolded)).toBe("**b**");
    const inside = run("**b**", 2, 3, "italic");
    expect(inside.text).toBe("***b***");
    const both = run("***b***", 0, 7, "italic");
    expect(both.text).toBe("**b**");
    expect(selected(both)).toBe("**b**");
  });

  it("bold toggles off from bold+italic and leaves italic in place", () => {
    const edit = run("***b***", 0, 7, "bold");
    expect(edit.text).toBe("*b*");
    expect(selected(edit)).toBe("*b*");
  });

  it("strike wraps, toggles off, and inserts a placeholder", () => {
    expect(run("a b", 2, 3, "strike").text).toBe("a ~~b~~");
    expect(run("a ~~b~~", 2, 7, "strike").text).toBe("a b");
    expect(run("a ~~b~~", 4, 5, "strike").text).toBe("a b");
    const empty = run("a ", 2, 2, "strike");
    expect(empty.text).toBe("a ~~删除线文字~~");
    expect(selected(empty)).toBe("删除线文字");
  });

  it("underline wraps with <u> tags, toggles off, and inserts a placeholder", () => {
    const wrapped = run("a b", 2, 3, "underline");
    expect(wrapped.text).toBe("a <u>b</u>");
    expect(selected(wrapped)).toBe("b");
    expect(run("a <u>b</u>", 2, 10, "underline").text).toBe("a b");
    const adjacent = run("a <u>b</u>", 5, 6, "underline");
    expect(adjacent.text).toBe("a b");
    expect(adjacent).toMatchObject({ start: 2, end: 10 });
    const empty = run("", 0, 0, "underline");
    expect(empty.text).toBe("<u>下划线文字</u>");
    expect(selected(empty)).toBe("下划线文字");
  });

  it("leaves selection-edge whitespace outside the markers", () => {
    const text = "hello world ";
    const edit = run(text, 6, 12, "bold");
    expect(edit.text).toBe("hello **world** ");
    expect(selected(edit)).toBe("world");
    expectConsistent({ text, start: 6, end: 12 }, edit);
    // A whitespace-only selection collapses to its start and inserts the placeholder.
    expect(run("a  b", 1, 3, "italic").text).toBe("a*斜体文字*  b");
    expect(run("a **b** c", 1, 8, "bold").text).toBe("a b c");
  });

  it("does not strip two independent bold runs into broken markers", () => {
    const text = "**a** **b**";
    const edit = run(text, 0, text.length, "bold");
    expect(edit.text).not.toBe("a** **b");
    expect(edit.text).toContain("**a** **b**");
    expectConsistent({ text, start: 0, end: text.length }, edit);
    expect(run("<u>a</u> <u>b</u>", 0, 17, "underline").text).not.toBe("a</u> <u>b");
  });

  it("wraps a multi-line selection as a whole", () => {
    const edit = run("one\ntwo\nthree", 0, 7, "bold");
    expect(edit.text).toBe("**one\ntwo**\nthree");
    expect(selected(edit)).toBe("one\ntwo");
  });

  it("tolerates reversed and out-of-range selections", () => {
    const reversed = run("hello world", 11, 6, "bold");
    expect(reversed.text).toBe("hello **world**");
    const outOfRange = run("hi", -5, 99, "bold");
    expect(outOfRange.text).toBe("**hi**");
    expect(outOfRange).toMatchObject({ start: 0, end: 2 });
    expectConsistent({ text: "hi", start: 0, end: 2 }, outOfRange);
    const nan = run("hi", Number.NaN, 1, "italic");
    expect(nan.text).toBe("*h*i");
  });
});

describe("line prefix commands", () => {
  it("ul prefixes the line under a collapsed cursor in the middle of the line", () => {
    const text = "first\nsecond\nthird";
    const edit = run(text, 8, 8, "ul");
    expect(edit.text).toBe("first\n- second\nthird");
    expect(edit).toMatchObject({ start: 6, end: 12, insert: "- second" });
    expect(selected(edit)).toBe("- second");
    expectConsistent({ text, start: 8, end: 8 }, edit);
  });

  it("ul prefixes every line touched by a multi-line selection", () => {
    const text = "a\nb\nc";
    const edit = run(text, 1, 3, "ul");
    expect(edit.text).toBe("- a\n- b\nc");
    expect(selected(edit)).toBe("- a\n- b");
  });

  it("ul removes the prefix when all touched lines already have it (toggle)", () => {
    const text = "- a\n- b\nc";
    const edit = run(text, 0, 5, "ul");
    expect(edit.text).toBe("a\nb\nc");
    expect(selected(edit)).toBe("a\nb");
  });

  it("ul only fills in the missing prefixes when the lines are mixed", () => {
    const edit = run("- a\nb", 0, 5, "ul");
    expect(edit.text).toBe("- a\n- b");
  });

  it("ul works on empty text", () => {
    const edit = run("", 0, 0, "ul");
    expect(edit.text).toBe("- ");
    expect(edit).toMatchObject({ start: 0, end: 0, selectionStart: 0, selectionEnd: 2 });
  });

  it("ul does not spread onto the next line when the selection ends after a newline", () => {
    const edit = run("a\nb\n", 0, 2, "ul");
    expect(edit.text).toBe("- a\nb\n");
  });

  it("ol numbers the touched lines from 1 and skips blank lines", () => {
    const text = "a\n\nb\nc";
    const edit = run(text, 0, text.length, "ol");
    expect(edit.text).toBe("1. a\n\n2. b\n3. c");
    expect(selected(edit)).toBe("1. a\n\n2. b\n3. c");
  });

  it("ol renumbers from 1 when some lines already carry a number", () => {
    const edit = run("7. a\nb\n9. c", 0, 11, "ol");
    expect(edit.text).toBe("1. a\n2. b\n3. c");
  });

  it("ol removes numbering when every line already has it (toggle)", () => {
    const edit = run("1. a\n2. b", 0, 9, "ol");
    expect(edit.text).toBe("a\nb");
  });

  it("ol and ul replace each other instead of stacking", () => {
    expect(run("- a\n- b", 0, 7, "ol").text).toBe("1. a\n2. b");
    expect(run("1. a\n2. b", 0, 9, "ul").text).toBe("- a\n- b");
  });

  it("quote prefixes, toggles, and preserves indentation", () => {
    expect(run("a\nb", 0, 3, "quote").text).toBe("> a\n> b");
    expect(run("> a\n> b", 0, 7, "quote").text).toBe("a\nb");
    expect(run("  a", 3, 3, "quote").text).toBe("  > a");
  });

  it("keeps CRLF line endings intact", () => {
    const text = "a\r\nb\r\nc";
    const edit = run(text, 0, 4, "ul");
    expect(edit.text).toBe("- a\r\n- b\r\nc");
    expect(edit).toMatchObject({ start: 0, end: 4, insert: "- a\r\n- b" });
    expectConsistent({ text, start: 0, end: 4 }, edit);
    expect(run("- a\r\n- b", 0, 8, "ul").text).toBe("a\r\nb");
  });

  it("tolerates out-of-range selections", () => {
    const edit = run("a\nb", -3, 50, "ol");
    expect(edit.text).toBe("1. a\n2. b");
    expect(edit).toMatchObject({ start: 0, end: 3 });
  });
});

describe("link", () => {
  it("wraps a selection and selects the url placeholder", () => {
    const text = "see docs now";
    const edit = run(text, 4, 8, "link");
    expect(edit.text).toBe("see [docs](https://) now");
    expect(selected(edit)).toBe("https://");
    expectConsistent({ text, start: 4, end: 8 }, edit);
  });

  it("inserts a placeholder and selects the link text when nothing is selected", () => {
    const edit = run("see ", 4, 4, "link");
    expect(edit.text).toBe("see [链接文字](https://)");
    expect(selected(edit)).toBe("链接文字");
  });

  it("clamps an out-of-range cursor to the end of the text", () => {
    const edit = run("ab", 10, 10, "link");
    expect(edit.text).toBe("ab[链接文字](https://)");
  });
});

describe("image", () => {
  it("inserts inline at the start of a line", () => {
    const edit = run("a\n", 2, 2, "image");
    expect(edit.text).toBe("a\n![图片描述](https://)");
    expect(selected(edit)).toBe("https://");
  });

  it("inserts at the start of empty text without a leading newline", () => {
    const edit = run("", 0, 0, "image");
    expect(edit.text).toBe("![图片描述](https://)");
    expect(selected(edit)).toBe("https://");
  });

  it("adds a newline first when the cursor is mid-line", () => {
    const text = "abc";
    const edit = run(text, 3, 3, "image");
    expect(edit.text).toBe("abc\n![图片描述](https://)");
    expect(selected(edit)).toBe("https://");
    expectConsistent({ text, start: 3, end: 3 }, edit);
  });

  it("uses the selection as the alt text and CRLF when the text does", () => {
    const edit = run("ab\r\ncd", 5, 6, "image");
    expect(edit.text).toBe("ab\r\nc\r\n![d](https://)");
    expect(selected(edit)).toBe("https://");
    const text = "see the chart";
    const chart = run(text, 8, 13, "image");
    expect(chart.text).toBe("see the \n![chart](https://)");
    expect(selected(chart)).toBe("https://");
    expectConsistent({ text, start: 8, end: 13 }, chart);
  });
});

describe("table", () => {
  const skeleton = "| 列 1 | 列 2 |\n| --- | --- |\n|  |  |";

  it("inserts a GFM skeleton into empty text and selects the first header cell", () => {
    const edit = run("", 0, 0, "table");
    expect(edit.text).toBe(`${skeleton}\n`);
    expect(selected(edit)).toBe("列 1");
  });

  it("adds blank lines around the table when inserted mid-text", () => {
    const text = "before after";
    const edit = run(text, 6, 6, "table");
    expect(edit.text).toBe(`before\n\n${skeleton}\n\n after`);
    expect(selected(edit)).toBe("列 1");
    expectConsistent({ text, start: 6, end: 6 }, edit);
  });

  it("reuses existing blank lines instead of adding more", () => {
    expect(run("a\n\n\n\nb", 3, 3, "table").text).toBe(`a\n\n${skeleton}\n\nb`);
    expect(run("a\n\nb", 2, 2, "table").text).toBe(`a\n\n${skeleton}\n\nb`);
  });

  it("adds only the missing newline when the cursor sits on its own line", () => {
    expect(run("a\nb", 2, 2, "table").text).toBe(`a\n\n${skeleton}\n\nb`);
  });

  it("replaces the selection and keeps CRLF endings", () => {
    const edit = run("a\r\nxx\r\nb", 3, 5, "table");
    expect(edit.text).toBe(`a\r\n\r\n${skeleton.replace(/\n/g, "\r\n")}\r\n\r\nb`);
    expect(selected(edit)).toBe("列 1");
  });
});

describe("image insert form helpers", () => {
  const TAG = "<img src=\"https://example.com/a.png\" alt=\"图\" style=\"max-width:100%;height:auto\">";

  it("offers the four percentage presets", () => {
    expect(IMAGE_WIDTH_PRESETS).toEqual([25, 50, 75, 100]);
  });

  it("accepts only http(s) urls", () => {
    expect(isHttpUrl("https://example.com/a.png")).toBe(true);
    expect(isHttpUrl("  HTTP://example.com ")).toBe(true);
    for (const bad of ["", "   ", "example.com/a.png", "ftp://x", "data:image/png;base64,AAA", "javascript:alert(1)", "https:/x"]) {
      expect(isHttpUrl(bad), bad).toBe(false);
    }
  });

  it("normalizes the custom width: empty or non-numeric is null, otherwise rounded and clamped to 1..100", () => {
    expect(normalizeImageWidthPercent("")).toBeNull();
    expect(normalizeImageWidthPercent("   ")).toBeNull();
    expect(normalizeImageWidthPercent("abc")).toBeNull();
    expect(normalizeImageWidthPercent("50%")).toBeNull();
    expect(normalizeImageWidthPercent("50")).toBe(50);
    expect(normalizeImageWidthPercent(" 33.4 ")).toBe(33);
    expect(normalizeImageWidthPercent("33.6")).toBe(34);
    expect(normalizeImageWidthPercent("0")).toBe(1);
    expect(normalizeImageWidthPercent("-20")).toBe(1);
    expect(normalizeImageWidthPercent("150")).toBe(100);
    expect(normalizeImageWidthPercent("1")).toBe(1);
    expect(normalizeImageWidthPercent("100")).toBe(100);
  });

  it("isImageWidthPercentInRange accepts empty (custom inactive) and 1..100, rejects the rest", () => {
    expect(isImageWidthPercentInRange("")).toBe(true);
    expect(isImageWidthPercentInRange("   ")).toBe(true);
    expect(isImageWidthPercentInRange("1")).toBe(true);
    expect(isImageWidthPercentInRange(" 50 ")).toBe(true);
    expect(isImageWidthPercentInRange("33.4")).toBe(true);
    expect(isImageWidthPercentInRange("100")).toBe(true);
    for (const bad of ["0", "-20", "101", "150", "abc", "50%", "Infinity", "NaN"]) {
      expect(isImageWidthPercentInRange(bad), bad).toBe(false);
    }
  });

  it("emits an inline img with max-width and no width attribute for the original size", () => {
    expect(imageMarkup({ url: "https://example.com/a.png", alt: "图", widthPercent: null })).toBe(TAG);
  });

  it("adds width=\"N%\" for presets and custom widths", () => {
    for (const preset of IMAGE_WIDTH_PRESETS) {
      expect(imageMarkup({ url: "https://example.com/a.png", alt: "", widthPercent: preset }))
        .toBe(`<img src="https://example.com/a.png" alt="" width="${preset}%" style="max-width:100%;height:auto">`);
    }
    expect(imageMarkup({ url: "https://example.com/a.png", alt: "", widthPercent: 37 })).toContain(" width=\"37%\" ");
    // A width that slipped past the form still lands inside 1..100.
    expect(imageMarkup({ url: "https://example.com/a.png", alt: "", widthPercent: 250 })).toContain(" width=\"100%\" ");
    expect(imageMarkup({ url: "https://example.com/a.png", alt: "", widthPercent: 0.2 })).toContain(" width=\"1%\" ");
  });

  it("escapes src and alt as attributes and trims both", () => {
    const markup = imageMarkup({
      url: "  https://example.com/a.png?a=1&b=<2> ",
      alt: " say \"hi\" & <bye> ",
      widthPercent: null,
    });
    expect(markup).toBe(
      "<img src=\"https://example.com/a.png?a=1&amp;b=&lt;2&gt;\" alt=\"say &quot;hi&quot; &amp; &lt;bye&gt;\" style=\"max-width:100%;height:auto\">",
    );
    expect(markup).not.toContain("\"hi\"");
  });

  const options = { url: "https://example.com/a.png", alt: "图", widthPercent: null };

  it("inserts on its own line at the start of empty text and leaves a blank line after it", () => {
    const edit = insertImageAtSelection({ text: "", start: 0, end: 0 }, options);
    expect(edit.text).toBe(`${TAG}\n\n`);
    expect(edit.selectionStart).toBe(edit.text.length);
    expect(edit.selectionEnd).toBe(edit.text.length);
    expectConsistent({ text: "", start: 0, end: 0 }, edit);
  });

  it("does not add a leading newline when the cursor is already at a line start", () => {
    const text = "a\nb";
    const edit = insertImageAtSelection({ text, start: 2, end: 2 }, options);
    expect(edit.text).toBe(`a\n${TAG}\n\nb`);
    expect(edit).toMatchObject({ start: 2, end: 2, insert: `${TAG}\n\n` });
    expect(edit.selectionStart).toBe(2 + TAG.length + 2);
    expect(edit.selectionEnd).toBe(edit.selectionStart);
    expectConsistent({ text, start: 2, end: 2 }, edit);
  });

  it("breaks the line first when the cursor is mid-line", () => {
    const text = "abc";
    const edit = insertImageAtSelection({ text, start: 3, end: 3 }, options);
    expect(edit.text).toBe(`abc\n${TAG}\n\n`);
    expect(edit.selectionStart).toBe(edit.text.length);
    expectConsistent({ text, start: 3, end: 3 }, edit);
    const mid = insertImageAtSelection({ text: "ab cd", start: 2, end: 2 }, options);
    expect(mid.text).toBe(`ab\n${TAG}\n\n cd`);
  });

  it("reuses the newlines that already follow the cursor", () => {
    expect(insertImageAtSelection({ text: "a\n\n\nb", start: 2, end: 2 }, options).text).toBe(`a\n${TAG}\n\nb`);
    expect(insertImageAtSelection({ text: "a\n\nb", start: 2, end: 2 }, options).text).toBe(`a\n${TAG}\n\nb`);
  });

  it("replaces the selection instead of keeping it as the alt text", () => {
    const text = "see the chart here";
    const edit = insertImageAtSelection({ text, start: 8, end: 13 }, { ...options, alt: "chart", widthPercent: 50 });
    const tag = "<img src=\"https://example.com/a.png\" alt=\"chart\" width=\"50%\" style=\"max-width:100%;height:auto\">";
    expect(edit.text).toBe(`see the \n${tag}\n\n here`);
    expect(edit).toMatchObject({ start: 8, end: 13 });
    expect(edit.text.slice(edit.selectionStart, edit.selectionEnd)).toBe("");
    expect(edit.selectionStart).toBe(8 + 1 + tag.length + 2);
    expectConsistent({ text, start: 8, end: 13 }, edit);
  });

  it("uses CRLF when the text does", () => {
    const text = "ab\r\ncd";
    const edit = insertImageAtSelection({ text, start: 5, end: 6 }, options);
    expect(edit.text).toBe(`ab\r\nc\r\n${TAG}\r\n\r\n`);
    expect(edit.text.slice(edit.selectionStart)).toBe("");
    expectConsistent({ text, start: 5, end: 6 }, edit);
    expect(insertImageAtSelection({ text: "a\r\n\r\nb", start: 3, end: 3 }, options).text).toBe(`a\r\n${TAG}\r\n\r\nb`);
  });

  it("normalizes a reversed or out-of-range selection", () => {
    const edit = insertImageAtSelection({ text: "abc", start: 99, end: 1 }, options);
    expect(edit.text).toBe(`a\n${TAG}\n\n`);
    expect(edit).toMatchObject({ start: 1, end: 3 });
    expectConsistent({ text: "abc", start: 1, end: 3 }, edit);
  });

  it("keeps the legacy markdown image command untouched", () => {
    expect(run("", 0, 0, "image").text).toBe("![图片描述](https://)");
  });
});

describe("insertTextAtSelection", () => {
  it("inserts at the caret and leaves the caret after the inserted text", () => {
    const text = "Hi , welcome";
    const edit = insertTextAtSelection({ text, start: 3, end: 3 }, "{{first_name}}");
    expect(edit.text).toBe("Hi {{first_name}}, welcome");
    expect(edit).toMatchObject({ start: 3, end: 3, insert: "{{first_name}}" });
    expect(edit.selectionStart).toBe(3 + "{{first_name}}".length);
    expect(edit.selectionEnd).toBe(edit.selectionStart);
    expectConsistent({ text, start: 3, end: 3 }, edit);
  });

  it("replaces the selection with the inserted text", () => {
    const text = "Hi NAME, welcome";
    const edit = insertTextAtSelection({ text, start: 3, end: 7 }, "{{name}}");
    expect(edit.text).toBe("Hi {{name}}, welcome");
    expect(edit).toMatchObject({ start: 3, end: 7, insert: "{{name}}" });
    expect(selected(edit)).toBe("");
    expect(edit.selectionStart).toBe(3 + "{{name}}".length);
    expectConsistent({ text, start: 3, end: 7 }, edit);
  });

  it("normalizes a reversed or out-of-range selection", () => {
    const edit = insertTextAtSelection({ text: "abc", start: 99, end: 1 }, "X");
    expect(edit.text).toBe("aX");
    expect(edit).toMatchObject({ start: 1, end: 3, selectionStart: 2, selectionEnd: 2 });
  });
});
