import { readFile, writeFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";

import {
  computeSiteCspManifest,
  manifestJson,
  manifestTypescript,
} from "./site-csp.mjs";

const root = new URL("../", import.meta.url);
const clientDirectory = new URL("site/build/client/", root);
const pages = JSON.parse(await readFile(new URL("site/marketing-pages.json", root), "utf8"));
const manifest = await computeSiteCspManifest({ clientDirectory, pages });

await Promise.all([
  writeFile(new URL("site/build/client/site-csp-manifest.json", root), manifestJson(manifest), "utf8"),
  writeFile(new URL("site/src/site-csp-manifest.generated.ts", root), manifestTypescript(manifest), "utf8"),
]);

console.log(`Generated per-page CSP hashes for ${Object.keys(manifest).length} Site paths in ${fileURLToPath(clientDirectory)}`);
