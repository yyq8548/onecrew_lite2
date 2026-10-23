import assert from "node:assert/strict";
import { test } from "node:test";
import type { PublicOpenCliWebRead } from "../src/platforms/opencli/web.ts";
import { discoveryWebAuthorResolution, validateDiscoveryWebAuthorResolution } from "../src/discovery/web-author-material.ts";
import { discoveryWebSourceTarget } from "../src/discovery/web-source-contracts.ts";

// 本轮调用记录 events seq74 的原始连续片段；这是已保存的作者介绍材料，未声称文章署名流程已经验证。
const page: PublicOpenCliWebRead = {
  requested_url: "https://bio.site/dcscrypto", document_url: "https://bio.site/dcscrypto", title: "Ryan \"Coop D'Ville\" Cooper", truncated: true,
  markdown_excerpt: "[![Ryan Cooper - LinkedIn Profile thumbnail](https://media.bio.site/sites/36b6d162-aecd-4410-ba6c-c753c58fbb0f/7cHwD8V22Xxky68rQ2Bp7c.jpg)\n\nRyan Cooper - LinkedIn Profile](https://linkedin.com/in/dcscrypto) [![Coop D'Ville (Web 3 Music Artist) thumbnail](https://media.bio.site/sites/36b6d162-aecd-4410-ba6c-c753c58fbb0f/bb5fze8McBokqg3XrHscJe.jpg)\n\nCoop D'Ville (Web 3 Music Artist)](https://bio.site/coopdville)",
};
const resolution = { author_name: "Ryan Cooper", attribution_quote: "[![Ryan Cooper - LinkedIn Profile thumbnail](https://media.bio.site/sites/36b6d162-aecd-4410-ba6c-c753c58fbb0f/7cHwD8V22Xxky68rQ2Bp7c.jpg)\n\nRyan Cooper - LinkedIn Profile](https://linkedin.com/in/dcscrypto)", author_page_url: null, native_source_url: "https://linkedin.com/in/dcscrypto", reason: "已保存介绍材料明确列出姓名与 LinkedIn 个人主页链接。" };

test("真实作者介绍材料保留姓名、原文和实际原生主页链接", () => {
  assert.deepEqual(validateDiscoveryWebAuthorResolution(page, resolution), resolution);
  assert.deepEqual(validateDiscoveryWebAuthorResolution(page, resolution, "Ryan Cooper"), resolution);
});

test("无归属结果明确保留原因，严格限制字段和长度", () => {
  const unresolved = { author_name: "", attribution_quote: "", author_page_url: null, native_source_url: null, reason: "当前材料尚不足以确认作者归属。" };
  assert.deepEqual(validateDiscoveryWebAuthorResolution(page, unresolved), unresolved);
  assert.throws(() => discoveryWebAuthorResolution.parse({ ...resolution, guessed_url: page.document_url }));
  for (const [field, maximum] of [["author_name", 512], ["attribution_quote", 2000], ["reason", 1000]] as const) assert.equal(discoveryWebAuthorResolution.safeParse({ ...resolution, [field]: "x".repeat(maximum + 1) }).success, false);
  assert.throws(() => validateDiscoveryWebAuthorResolution(page, { ...unresolved, reason: " " }));
});

test("真实文本的无效变异拒绝无姓名、改写引文和作者漂移", () => {
  assert.throws(() => validateDiscoveryWebAuthorResolution(page, { ...resolution, author_name: "" }), /web_author_quote_invalid/u);
  assert.throws(() => validateDiscoveryWebAuthorResolution(page, { ...resolution, attribution_quote: "Ryan Cooper - verified LinkedIn Profile" }), /web_author_quote_invalid/u);
  assert.throws(() => validateDiscoveryWebAuthorResolution(page, { ...resolution, attribution_quote: "LinkedIn Profile" }), /web_author_quote_invalid/u);
  assert.throws(() => validateDiscoveryWebAuthorResolution(page, resolution, "Rony Roy"), /web_author_identity_conflict/u);
});

test("真实链接不能替换为猜测地址、图片资源或代码中的地址", () => {
  assert.throws(() => validateDiscoveryWebAuthorResolution(page, { ...resolution, attribution_quote: "Ryan Cooper - LinkedIn Profile" }), /web_author_native_attribution_invalid/u);
  const unrelatedFooter = { ...page, markdown_excerpt: `${page.markdown_excerpt}\n\n[X](https://x.com/BitcoinMagazine)` };
  assert.throws(() => validateDiscoveryWebAuthorResolution(unrelatedFooter, { ...resolution, native_source_url: "https://x.com/BitcoinMagazine" }), /web_author_native_attribution_invalid/u);
  const differentProfileQuote = resolution.attribution_quote.replace("https://linkedin.com/in/dcscrypto)", "https://linkedin.com/in/dcscryptohub)");
  const prefixFooter = { ...page, markdown_excerpt: `${page.markdown_excerpt.replace(resolution.attribution_quote, differentProfileQuote)}\n\n[LinkedIn](https://linkedin.com/in/dcscrypto)` };
  assert.throws(() => validateDiscoveryWebAuthorResolution(prefixFooter, { ...resolution, attribution_quote: differentProfileQuote }), /web_author_native_attribution_invalid/u);
  assert.throws(() => validateDiscoveryWebAuthorResolution(page, { ...resolution, native_source_url: "https://linkedin.com/in/ronyroycrypto" }), /web_author_link_unobserved/u);
  assert.throws(() => validateDiscoveryWebAuthorResolution(page, { ...resolution, native_source_url: "https://media.bio.site/sites/36b6d162-aecd-4410-ba6c-c753c58fbb0f/7cHwD8V22Xxky68rQ2Bp7c.jpg" }), /web_author_link_unobserved/u);
  assert.throws(() => validateDiscoveryWebAuthorResolution({ ...page, markdown_excerpt: `\`\`\`markdown\n${page.markdown_excerpt}\n\`\`\`` }, resolution), /web_author_link_unobserved/u);
  assert.throws(() => validateDiscoveryWebAuthorResolution({ ...page, markdown_excerpt: `<script>${page.markdown_excerpt}</script>` }, resolution), /web_author_link_unobserved/u);
});

test("同域的其他人物链接与外域链接均不能当作当前作者介绍页", () => {
  assert.throws(() => validateDiscoveryWebAuthorResolution(page, { ...resolution, author_page_url: "https://bio.site/coopdville" }), /web_author_page_invalid/u);
  assert.throws(() => validateDiscoveryWebAuthorResolution(page, { ...resolution, author_page_url: resolution.native_source_url }), /web_author_page_invalid/u);
  for (const url of ["http://bio.site/dcscrypto", "https://127.0.0.1/author", "https://user:password@bio.site/dcscrypto"]) assert.throws(() => discoveryWebAuthorResolution.parse({ ...resolution, author_page_url: url }));
});

test("本轮真实未读普通网页URL保持非原生账号线索", () => {
  for (const url of ["https://en.wikipedia.org/wiki/Blockchain", "https://www.cloudflare.com/learning/security/what-is-blockchain/", "https://www.blockchain-council.org/blockchain/what-is-blockchain-technology-and-how-does-it-work/"]) assert.equal(discoveryWebSourceTarget(url), null);
});
