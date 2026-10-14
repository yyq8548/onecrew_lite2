import { createHash } from "node:crypto";
import assert from "node:assert/strict";
import { describe, it } from "node:test";

import {
  analyzeSiteHtml,
  localizedSitePaths,
  manifestJson,
  manifestTypescript,
  verifySiteCspManifest,
} from "./site-csp.mjs";

describe("Site CSP manifest generation", () => {
  it("derives default English and explicitly localized Chinese canonical paths", () => {
    assert.deepEqual(localizedSitePaths([{ path: "/" }, { path: "/product" }]), [
      "/",
      "/product",
      "/zh",
      "/zh/product",
    ]);
  });

  it("hashes exact executable inline UTF-8 text without trimming", () => {
    const source = "\n  globalThis.message = '你好';\r\n";
    const expected = `'sha256-${createHash("sha256").update(source, "utf8").digest("base64")}'`;
    const html = `<script>${source}</script><script type="module">module()</script><script type="text/javascript; charset=utf-8">classic()</script>`;
    const hashes = analyzeSiteHtml(html);

    assert.ok(hashes.includes(expected));
    assert.equal(hashes.length, 3);
  });

  it("excludes JSON-LD and external JavaScript", () => {
    const html = [
      '<script type="application/ld+json">{"name":"OneCrew"}</script>',
      '<script type="application/json">{"data":true}</script>',
      '<script src="/assets/site.js"></script>',
      "<script>boot()</script>",
    ].join("");
    assert.deepEqual(analyzeSiteHtml(html), [
      `'sha256-${createHash("sha256").update("boot()", "utf8").digest("base64")}'`,
    ]);
  });

  for (const html of [
    '<script type="">boot()</script>',
    '<script type="not a mime">boot()</script>',
    '<script></script>',
    '<main style="color:red"></main>',
    '<style>body { color: red; }</style>',
  ]) {
    it(`fails closed for ambiguous or forbidden markup: ${html}`, () => {
      assert.throws(() => analyzeSiteHtml(html, "fixture.html"), /fixture\.html/);
    });
  }

  it("makes a changed inline script visibly stale in both stable manifests", () => {
    const before = { "/": analyzeSiteHtml("<script>before()</script>") };
    const after = { "/": analyzeSiteHtml("<script>after()</script>") };

    assert.notEqual(manifestJson(after), manifestJson(before));
    assert.notEqual(manifestTypescript(after), manifestTypescript(before));
    assert.deepEqual(Object.keys(JSON.parse(manifestJson(before))), ["/"]);
    assert.throws(
      () => verifySiteCspManifest(after, manifestJson(before), manifestTypescript(before)),
      /stale or malformed/,
    );
    assert.doesNotThrow(
      () => verifySiteCspManifest(after, manifestJson(after), manifestTypescript(after)),
    );
  });
});
