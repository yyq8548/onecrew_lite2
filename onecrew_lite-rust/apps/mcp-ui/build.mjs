// Build every widget under src/widgets/<name>/ into one self-contained HTML
// file at crates/mcp-adapter/assets/apps/<name>.html. The adapter embeds
// these with include_str! and serves them as ui:// resources, so the output
// must carry zero external references.
import { readdirSync, mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { dirname, extname, join, relative, resolve, sep } from "node:path";
import { fileURLToPath } from "node:url";
import { brotliCompressSync, constants, gzipSync } from "node:zlib";
import ts from "typescript";
import { build } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";
import { viteSingleFile } from "vite-plugin-singlefile";

const appRoot = dirname(fileURLToPath(import.meta.url));
const repoRoot = resolve(appRoot, "../..");
const sourceRoot = join(appRoot, "src");
const packageJson = JSON.parse(readFileSync(join(appRoot, "package.json"), "utf8"));
const widgetContracts = {
  "operation-inspector": {
    budgets: {
      // SearchPlan adds complete typed snapshot/continuation validators. Measured
      // Inlined dist measurement, 2026-09-08: HTML 928637 raw/207365 gzip;
      // JS 898711 raw/201231 gzip. Canonical platform glyphs + safe diagnostics
      // measured 210459 HTML gzip on 2026-09-08; retain the Brotli/CSS limits.
      // CreatorSave typed review + Checkbox + complete text continuations and
      // global long-text bounds: measured HTML 989990/220694/180325 and
      // JS 958614/214331/175211 (raw/gzip/Brotli), 2026-09-08.
      // Creator batch list + full typed per-row outcomes, 2026-09-08:
      // HTML 1070330/230571/187698; JS 1035846/223606/182096.
      // New branded UI, route review, scope editor and score display (2026-09-09);
      // preview dependencies and demo scoring are excluded from widget imports.
      html: { raw: 1_120_000, gzip: 249_000, brotli: 202_000 },
      js: { raw: 1_078_000, gzip: 240_000, brotli: 195_000 },
      css: { raw: 50_000, gzip: 10_000, brotli: 9_000 },
    },
  },
  "retired-operation-inspector": {
    budgets: {
      html: { raw: 450_000, gzip: 140_000, brotli: 120_000 },
      js: { raw: 440_000, gzip: 135_000, brotli: 115_000 },
      css: { raw: 10_000, gzip: 3_000, brotli: 3_000 },
    },
  },
};

function sourceFiles(root) {
  return readdirSync(root, { withFileTypes: true }).flatMap((entry) => {
    const path = join(root, entry.name);
    if (entry.isDirectory()) return sourceFiles(path);
    return [path];
  });
}

function moduleImports(path) {
  if (![".ts", ".tsx"].includes(extname(path))) return [];
  const source = readFileSync(path, "utf8");
  const scriptKind = extname(path) === ".tsx" ? ts.ScriptKind.TSX : ts.ScriptKind.TS;
  const sourceFile = ts.createSourceFile(path, source, ts.ScriptTarget.Latest, false, scriptKind);
  const imports = [];
  const staticSpecifier = (node) =>
    ts.isStringLiteral(node) || ts.isNoSubstitutionTemplateLiteral(node) ? node.text : null;
  function visit(node) {
    if ((ts.isImportDeclaration(node) || ts.isExportDeclaration(node)) && node.moduleSpecifier) {
      const specifier = staticSpecifier(node.moduleSpecifier);
      if (specifier != null) imports.push(specifier);
    } else if (ts.isCallExpression(node) && node.expression.kind === ts.SyntaxKind.ImportKeyword &&
               node.arguments.length === 1) {
      const argument = node.arguments[0];
      const specifier = staticSpecifier(argument);
      if (specifier != null) {
        imports.push(specifier);
      } else {
        throw new Error(`${path} has a non-static dynamic import that the bundle source audit cannot resolve`);
      }
    }
    ts.forEachChild(node, visit);
  }
  visit(sourceFile);
  return imports;
}

function assertSourceBoundary(widget) {
  if (!widgetContracts[widget]) throw new Error(`${widget} is missing an explicit build contract`);
  const violations = [];
  for (const path of sourceFiles(sourceRoot)) {
    for (const specifier of moduleImports(path)) {
      if (specifier.startsWith("@/") || specifier.startsWith("~/") || specifier.includes("desktop")) {
        violations.push(`${relative(appRoot, path)} -> ${specifier}`);
        continue;
      }
      if (specifier.startsWith(".")) {
        const target = resolve(dirname(path), specifier);
        if (target !== sourceRoot && !target.startsWith(`${sourceRoot}${sep}`)) {
          violations.push(`${relative(appRoot, path)} -> ${specifier}`);
        }
      }
    }
  }
  if (violations.length > 0) {
    throw new Error(`${widget} crosses the MCP UI source boundary\n${violations.join("\n")}`);
  }
}

function sizes(value) {
  const bytes = Buffer.from(value);
  return {
    raw: bytes.length,
    gzip: gzipSync(bytes, { level: 9 }).length,
    brotli: brotliCompressSync(bytes, {
      params: { [constants.BROTLI_PARAM_QUALITY]: 11 },
    }).length,
  };
}

function inlineContent(html, tag) {
  return [...html.matchAll(new RegExp(`<${tag}[^>]*>([\\s\\S]*?)<\\/${tag}>`, "g"))]
    .map((match) => match[1])
    .join("");
}

function assertSelfContained(widget, html) {
  const scripts = [...html.matchAll(/<script\b([^>]*)>([\s\S]*?)<\/script>/gi)];
  const styles = [...html.matchAll(/<style\b[^>]*>([\s\S]*?)<\/style>/gi)];
  if (scripts.length === 0 || scripts.some((match) => /\bsrc\s*=/i.test(match[1])) ||
      scripts.every((match) => match[2].trim().length === 0)) {
    throw new Error(`${widget} must contain nonempty inline JavaScript and no script src`);
  }
  if (styles.length === 0 || styles.every((match) => match[1].trim().length === 0)) {
    throw new Error(`${widget} must contain nonempty inline CSS`);
  }
  const markup = html
    .replace(/<script\b[^>]*>[\s\S]*?<\/script>/gi, "")
    .replace(/<style\b[^>]*>[\s\S]*?<\/style>/gi, "");
  if (/<link\b[^>]*\bhref\s*=/i.test(markup) ||
      /\b(?:src|href)\s*=\s*["'](?:https?:|\/\/)/i.test(markup)) {
    throw new Error(`${widget} must not reference external documents or media`);
  }
  const css = inlineContent(html, "style");
  if (/@import\b/i.test(css)) {
    throw new Error(`${widget} compiled CSS must not retain @import rules`);
  }
  for (const match of css.matchAll(/url\(\s*(?:(["'])(.*?)\1|([^)]*?))\s*\)/gi)) {
    const value = (match[2] ?? match[3] ?? "").trim();
    if (!value.startsWith("data:") && !value.startsWith("#")) {
      throw new Error(`${widget} CSS must not reference external asset ${value}`);
    }
  }
}

function assertBudgets(widget, html) {
  const budget = widgetContracts[widget]?.budgets;
  if (!budget) throw new Error(`${widget} is missing an explicit self-contained bundle budget`);
  const measured = {
    html: sizes(html),
    js: sizes(inlineContent(html, "script")),
    css: sizes(inlineContent(html, "style")),
  };
  for (const [group, groupSizes] of Object.entries(measured)) {
    for (const [encoding, actual] of Object.entries(groupSizes)) {
      const limit = budget[group]?.[encoding];
      if (!Number.isFinite(limit) || limit < 0) {
        throw new Error(`${widget} has an invalid ${group}.${encoding} budget`);
      }
      if (actual > limit) {
        throw new Error(`${widget} ${group}.${encoding} is ${actual} bytes; budget is ${limit} bytes`);
      }
    }
  }
  const report = Object.entries(measured)
    .map(([group, groupSizes]) => `${group} raw=${groupSizes.raw} gzip=${groupSizes.gzip} br=${groupSizes.brotli}`)
    .join("; ");
  console.log(`${widget} bundle: ${report}`);
}

function assertOperationInspectorFeatureCss(widget, html) {
  if (widget !== "operation-inspector") return;
  const css = inlineContent(html, "style");
  const requiredSelectors = [
    ".grid-cols-\\[auto_1fr_auto\\]",
    ".rounded-xl",
    ".channel-mark",
    ".text-success-foreground",
    ".bg-destructive\\/10",
  ];
  const missing = requiredSelectors.filter((selector) => !css.includes(selector));
  if (missing.length > 0) {
    throw new Error(`${widget} compiled CSS is missing feature selectors: ${missing.join(", ")}`);
  }
  if (!css.includes(":root[data-theme=dark]") || !css.includes("[data-theme=dark]")) {
    throw new Error(`${widget} compiled CSS must bind dark tokens and variants to Host data-theme`);
  }
  if (/prefers-color-scheme:\s*dark/i.test(css)) {
    throw new Error(`${widget} compiled CSS must not infer dark mode from the iframe media query`);
  }
}

const widgets = readdirSync(join(sourceRoot, "widgets"), { withFileTypes: true })
  .filter((entry) => entry.isDirectory())
  .map((entry) => entry.name)
  .sort();
const outRoot = join(repoRoot, "crates/mcp-adapter/assets/apps");
mkdirSync(outRoot, { recursive: true });
// The vertical component of a minified rule's overflow, whether it was written
// as the `overflow-y` longhand or collapsed into the one- or two-value
// shorthand (in `overflow:<x> <y>` the second value is the vertical one).
const verticalOverflow = (rule) => {
  const shorthand = rule.match(/[;{]\s*overflow:\s*([a-z]+)(?:\s+([a-z]+))?/);
  return shorthand ? shorthand[2] ?? shorthand[1] : rule.match(/[;{]\s*overflow-y:\s*([a-z]+)/)?.[1];
};
for (const widget of widgets) {
  assertSourceBoundary(widget);
  if (widget === "retired-operation-inspector") {
    const source = sourceFiles(join(sourceRoot, "widgets", widget))
      .map((path) => readFileSync(path, "utf8"))
      .join("\n");
    for (const forbidden of ["callServerTool", "crm_interaction_state", "crm_operation_execute", "onecrew_interaction_state", "onecrew_operation_execute"]) {
      if (source.includes(forbidden)) throw new Error(`${widget} must not contain ${forbidden}`);
    }
  }
  const outDir = join(appRoot, "dist", widget);
  await build({
    configFile: false,
    root: join(sourceRoot, "widgets", widget),
    plugins: [react(), tailwindcss(), viteSingleFile()],
    resolve: {
      alias: { "@mcp-ui": sourceRoot },
      dedupe: ["react", "react-dom"],
    },
    define: {
      __MCP_UI_VERSION__: JSON.stringify(packageJson.version),
    },
    build: {
      outDir,
      emptyOutDir: true,
      modulePreload: false,
      reportCompressedSize: false,
      sourcemap: false,
    },
    logLevel: "warn",
  });
  // Strip trailing whitespace: the file is committed and repository
  // whitespace checks apply to it like to any source file.
  const html = readFileSync(join(outDir, "index.html"), "utf8")
    // Windows template inputs can retain CRCRLF through inlining. Canonical
    // output uses LF before applying the committed-asset whitespace check.
    .replace(/\r+\n/g, "\n")
    .replace(/[ \t]+$/gm, "");
  // No page-level scrollbar is visible inside the iframe — one beside the
  // Host's own would be noise — but the viewport itself must stay scrollable.
  // This Host keeps its own frame height whatever height the App reports, so
  // what the frame cannot show is reachable only by scrolling; `overflow` on
  // the root propagates to the viewport, which is why suppressing it there
  // turned clipped content into lost content. Vertical scroll lives on `body`.
  const rootRule = html.match(/html,\s*body,\s*#root\s*\{[^}]+\}/)?.[0] ?? "";
  // A widget whose stylesheet stayed unminified starts its `body` rule on its
  // own indented line, so allow whitespace ahead of it — but not a name ending
  // in `body`, which is a different selector.
  const bodyRules = [...html.matchAll(/(?:^|[,{}\s])(body\s*\{[^}]*\})/g)].map(([, rule]) => rule);
  if (!/scrollbar-width:\s*none/.test(rootRule) || !/body::-webkit-scrollbar[^{}]*\{[^}]*display:\s*none/.test(html)) {
    throw new Error(`${widget} must keep the page scrollbar invisible beside the Host's own`);
  }
  if (["hidden", "clip"].includes(verticalOverflow(rootRule) ?? "")) {
    throw new Error(`${widget} must not suppress the iframe viewport's vertical scroll on the root`);
  }
  if (!bodyRules.some((rule) => ["auto", "scroll"].includes(verticalOverflow(rule) ?? ""))) {
    throw new Error(`${widget} must leave content the Host's frame cannot show reachable by scrolling`);
  }
  // An inline card fits its content and adds no scrollport of its own: an inner
  // scroll region hid the message the card existed to show, and overflow is now
  // reached by scrolling the document. Fullscreen is the one mode where the card
  // owns a scrollport, so a scrolling content region stays scoped to that mode.
  if (widget === "operation-inspector") {
    for (const [, selector] of html.matchAll(/([^{}]*\.result-shell-content)\{[^}]*overflow-y:auto/g)) {
      if (!selector.includes("[data-display-mode=fullscreen]")) {
        throw new Error(`${widget} must fit its content inline instead of scrolling inside the card`);
      }
    }
  }
  const outputs = readdirSync(outDir).sort();
  if (outputs.length !== 1 || outputs[0] !== "index.html") {
    throw new Error(`${widget} must emit only one self-contained index.html; got ${outputs.join(", ")}`);
  }
  assertSelfContained(widget, html);
  assertOperationInspectorFeatureCss(widget, html);
  assertBudgets(widget, html);
  writeFileSync(join(outRoot, `${widget}.html`), html);
}
rmSync(join(appRoot, "dist"), { recursive: true, force: true });
console.log(`built ${widgets.length} widget(s): ${widgets.join(", ")}`);
