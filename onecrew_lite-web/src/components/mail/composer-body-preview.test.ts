import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import i18next, { type i18n } from "i18next";
import { I18nextProvider } from "react-i18next";
import { beforeAll, describe, expect, it } from "vitest";

import { buildPreviewDocument, ComposerBodyPreview } from "@/components/mail/composer-body-preview";
import zhCNMail from "@/i18n/locales/zh-CN/mail";

let mailI18n: i18n;

beforeAll(async () => {
  mailI18n = i18next.createInstance();
  await mailI18n.init({ lng: "zh-CN", resources: { "zh-CN": { mail: zhCNMail } } });
});

function renderPreview(props: Parameters<typeof ComposerBodyPreview>[0]): string {
  return renderToStaticMarkup(
    createElement(
      I18nextProvider,
      { i18n: mailI18n },
      createElement(ComposerBodyPreview, props),
    ),
  );
}

describe("buildPreviewDocument", () => {
  it("starts with an HTML5 doctype and declares utf-8", () => {
    const doc = buildPreviewDocument("<p>你好</p>");
    expect(doc.startsWith("<!doctype html>")).toBe(true);
    expect(doc).toContain('<meta charset="utf-8">');
  });

  it("embeds the body html inside <body>", () => {
    const html = '<div style="font-family:SimSun, serif"><p>正文</p></div>';
    const doc = buildPreviewDocument(html);
    const bodyStart = doc.indexOf("<body>");
    const bodyEnd = doc.indexOf("</body>");
    expect(bodyStart).toBeGreaterThan(-1);
    expect(doc.indexOf(html)).toBeGreaterThan(bodyStart);
    expect(doc.indexOf(html)).toBeLessThan(bodyEnd);
  });

  it("inlines recipient-client styles instead of theme tokens", () => {
    const doc = buildPreviewDocument("");
    expect(doc).toContain("background:#fff");
    expect(doc).toContain("img{max-width:100%}");
    expect(doc).toContain("blockquote{border-left:3px solid #ccc");
    expect(doc).not.toContain("<link");
  });

  it("declares a CSP that only allows inline styles and images", () => {
    const doc = buildPreviewDocument("");
    const meta = doc.match(/<meta http-equiv="Content-Security-Policy" content="([^"]*)">/);
    expect(meta).not.toBeNull();
    expect(meta![1]).toContain("default-src 'none'");
    expect(meta![1]).toContain("img-src data: http: https:");
    expect(meta![1]).toContain("style-src 'unsafe-inline'");
    expect(meta![1]).not.toContain("script-src");
    expect(doc.indexOf(meta![0])).toBeLessThan(doc.indexOf("<style>"));
  });

  it("does not sanitize scripts; neutralising them is the iframe sandbox's job", () => {
    const doc = buildPreviewDocument("<script>alert(1)</script><p>x</p>");
    // The document is passed through verbatim — the component relies on
    // sandbox="" so this script can never run in the preview frame.
    expect(doc).toContain("<script>alert(1)</script>");
  });
});

describe("ComposerBodyPreview", () => {
  it("renders rich html in a fully sandboxed iframe with no allow-* permissions", () => {
    const markup = renderPreview({ html: "<p>x</p>", text: "", plainText: false });
    expect(markup).toContain("<iframe");
    expect(markup).toContain('sandbox=""');
    expect(markup).not.toContain("allow=");
    expect(markup).not.toContain("allow-");
    expect(markup).not.toContain("bg-white");
  });

  it("renders plain text without an iframe", () => {
    const markup = renderPreview({ html: "<p>x</p>", text: "hello", plainText: true });
    expect(markup).not.toContain("<iframe");
    expect(markup).toContain("hello");
  });
});
