import { readFile } from "node:fs/promises";

import {
  computeSiteCspManifest,
  verifySiteCspManifest,
} from "./site-csp.mjs";

const root = new URL("../", import.meta.url);
const pages = JSON.parse(await readFile(new URL("site/marketing-pages.json", root), "utf8"));
const manifest = await computeSiteCspManifest({
  clientDirectory: new URL("site/build/client/", root),
  pages,
});

const [actualJson, actualTypescript] = await Promise.all([
  readFile(new URL("site/build/client/site-csp-manifest.json", root), "utf8"),
  readFile(new URL("site/src/site-csp-manifest.generated.ts", root), "utf8"),
]);

verifySiteCspManifest(manifest, actualJson, actualTypescript);

console.log("Verified 46 per-page Site CSP hash manifests against prerendered HTML");
