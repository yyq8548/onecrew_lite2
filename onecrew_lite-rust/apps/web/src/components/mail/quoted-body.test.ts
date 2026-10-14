import { describe, expect, it } from "vitest";

import { parseEmailChain, quoteSummary, splitQuotedBody, unquote } from "./quoted-body";

describe("splitQuotedBody", () => {
  it("keeps a message with no quoted history whole", () => {
    const result = splitQuotedBody("Hi River,\n\nThanks for reaching out.\n");
    expect(result).toEqual({
      body: "Hi River,\n\nThanks for reaching out.",
      attribution: null,
      quote: null,
    });
  });

  it("splits the reply from the header block a client quotes back", () => {
    const result = splitQuotedBody(
      [
        "Hi GoAI Partnerships,",
        "Thanks for reaching out.",
        "Best, River",
        '> From: "GoAI Partnerships"<partnership@mail.goai.xin>',
        "> Date:  Fri, Aug 7, 2026, 16:49",
        ">",
        "> Hi River,",
      ].join("\n"),
    );
    expect(result.body).toBe("Hi GoAI Partnerships,\nThanks for reaching out.\nBest, River");
    expect(result.attribution).toBeNull();
    expect(result.quote).toBe(
      '> From: "GoAI Partnerships"<partnership@mail.goai.xin>\n> Date:  Fri, Aug 7, 2026, 16:49\n>\n> Hi River,',
    );
  });

  it("moves the attribution line into the quote it introduces", () => {
    const result = splitQuotedBody(
      ["Sounds good.", "", "On Fri, Aug 7, 2026 at 16:49, GoAI wrote:", "> Original text"].join("\n"),
    );
    expect(result.body).toBe("Sounds good.");
    expect(result.attribution).toBe("On Fri, Aug 7, 2026 at 16:49, GoAI wrote:");
    expect(result.quote).toBe("> Original text");
  });

  it("recognises a separator-style quote with no markers", () => {
    const result = splitQuotedBody(["Reply text", "-----Original Message-----"].join("\n"));
    expect(result.body).toBe("Reply text");
    expect(result.quote).toBe("-----Original Message-----");
  });

  it("never truncates a message on a mid-body marker", () => {
    const source = ["> quoted at the top", "", "My actual reply."].join("\n");
    expect(splitQuotedBody(source)).toEqual({
      body: source,
      attribution: null,
      quote: null,
    });
  });

  it("treats an empty body as empty", () => {
    expect(splitQuotedBody(null)).toEqual({ body: "", attribution: null, quote: null });
  });
});

describe("unquote", () => {
  it("removes one level and preserves nesting", () => {
    expect(unquote("> outer\n>> inner\n> outer again")).toBe("outer\n> inner\nouter again");
  });
});

describe("quoteSummary", () => {
  it("prefers the attribution line", () => {
    expect(quoteSummary({ body: "", attribution: "On … wrote:", quote: "> x" }, "引用的历史邮件")).toBe("On … wrote:");
  });

  it("falls back to the quote's first real line", () => {
    expect(quoteSummary({ body: "", attribution: null, quote: ">\n> From: a@b.c" }, "引用的历史邮件")).toBe(
      "From: a@b.c",
    );
  });
});

describe("parseEmailChain", () => {
  it("restores one turn per round instead of a wall of markers", () => {
    // 飞书式:抬头块 + 正文,每深一层多一个 `>`。原样倒出来读者只会
    // 看到一堵 `>`;客户端要还原成「谁在什么时候说了什么」。
    const text = [
      "Hi GoAI Partnerships,",
      "Thanks for reaching out.",
      "",
      "> From: \"GoAI Partnerships\"<partnership@mail.goai.xin>",
      "> Date:  Thu, Aug 20, 2026, 11:47",
      "> Subject:  Re: Potential crypto industry partnership",
      "> To: <river_fan@digifinex.org>",
      ">",
      "> Thans a lot",
    ].join("\n");

    const turns = parseEmailChain(text);
    expect(turns).toHaveLength(2);
    expect(turns[0].header).toBeNull();
    expect(turns[0].body).toContain("Thanks for reaching out.");
    expect(turns[1].header).toEqual({
      From: '"GoAI Partnerships"<partnership@mail.goai.xin>',
      Date: "Thu, Aug 20, 2026, 11:47",
      Subject: "Re: Potential crypto industry partnership",
      To: "<river_fan@digifinex.org>",
    });
    expect(turns[1].body).toBe("Thans a lot");
  });

  it("carries the attribution line down to the turn it introduces", () => {
    // Gmail/我们自己产出的形态:一行 "On … wrote:" 而不是抬头块。
    const text = ["好的", "", "On Wed, Aug 19, 2026 at 10:00, Alice <a@e.com> wrote:", "> 能寄样品吗？"].join("\n");
    const turns = parseEmailChain(text);
    expect(turns).toHaveLength(2);
    expect(turns[0].body).toBe("好的");
    expect(turns[1].attribution).toContain("Alice <a@e.com> wrote:");
    expect(turns[1].body).toBe("能寄样品吗？");
  });

  it("keeps unwinding a chain that accumulated over several rounds", () => {
    // 第三轮里第一轮必须还在:每层剥掉一个 `>`。
    const text = [
      "第三轮",
      "> From: B",
      "> 第二轮",
      "> > From: A",
      "> > 第一轮",
    ].join("\n");
    const turns = parseEmailChain(text);
    expect(turns.map((turn) => turn.body)).toEqual(["第三轮", "第二轮", "第一轮"]);
    expect(turns[1].header).toEqual({ From: "B" });
    expect(turns[2].header).toEqual({ From: "A" });
  });

  it("returns the message itself when nothing is quoted", () => {
    const turns = parseEmailChain("首次触达");
    expect(turns).toEqual([{ header: null, attribution: null, body: "首次触达" }]);
  });

  it("survives an empty message without inventing a turn's worth of content", () => {
    expect(parseEmailChain("")).toEqual([{ header: null, attribution: null, body: "" }]);
    expect(parseEmailChain(null)).toEqual([{ header: null, attribution: null, body: "" }]);
  });
});
