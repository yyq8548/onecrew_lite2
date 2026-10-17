import { findPackageJSON } from "node:module";
import { lstat, readFile } from "node:fs/promises";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { z } from "zod";
import { parseApplicationJson } from "../contracts/application-json.ts";
import { payloadDigest } from "./payload-manifest.ts";

const metadataSchema = z.object({ name: z.string(), version: z.string() });
const relatedMetadataSchema = metadataSchema.extend({ license: z.literal("MIT"), repository: z.object({ url: z.string() }), optionalDependencies: z.record(z.string(), z.string()).optional() });
export interface SupplementalLicense { path: string; source: string; sha256: string }

async function verified(path: string, source: string, sha256: string): Promise<SupplementalLicense> {
  const info = await lstat(path);
  if (!info.isFile() || info.isSymbolicLink() || payloadDigest(await readFile(path)) !== sha256) throw new Error("package.supplemental_license_changed");
  return { path, source, sha256 };
}

// 仅使用已核对到具体发布版本的全文；更换版本后必须重新核对来源和哈希。
export async function supplementalLicense(packageRoot: string): Promise<SupplementalLicense | null> {
  const raw = parseApplicationJson(await readFile(join(packageRoot, "package.json"))), metadata = metadataSchema.parse(raw);
  if (metadata.name === "isarray" && metadata.version === "1.0.0") {
    return verified(join(packageRoot, "README.md"), "https://registry.npmjs.org/isarray/-/isarray-1.0.0.tgz#README.md", "ff138e683771b187f3629c383db72ee7d632009010a36d08e18e8d2a34222ec7");
  }
  if (metadata.name === "saxes" && metadata.version === "5.0.1") {
    return verified(fileURLToPath(new URL("./licenses/saxes-5.0.1-LICENSE.txt", import.meta.url)), "https://raw.githubusercontent.com/lddubeau/saxes/6ef6a275b20f19cb67808daf0d8f64e06aaf5b0e/LICENSE", "0fac2374380621b22e6b50451057721a9c52935b02d16d106a9f04897f061d0e");
  }
  if (/^@wreq-js\/binding-(?:darwin-(?:arm64|x64)|win32-x64-msvc)$/u.test(metadata.name) && metadata.version === "3.2.0") {
    const parentPath = findPackageJSON("wreq-js", import.meta.url);
    if (!parentPath) throw new Error("package.license_parent_missing");
    const binding = relatedMetadataSchema.parse(raw), parent = relatedMetadataSchema.parse(parseApplicationJson(await readFile(parentPath)));
    if (parent.name !== "wreq-js" || parent.version !== metadata.version || parent.optionalDependencies?.[metadata.name] !== metadata.version
      || parent.repository.url !== binding.repository.url) throw new Error("package.license_parent_mismatch");
    return verified(join(dirname(parentPath), "LICENSE"), "https://registry.npmjs.org/wreq-js/-/wreq-js-3.2.0.tgz#LICENSE", "f5e211eaa1c732f23cae866f00c7a0d9f458cbb6e37051170a3f7bb45c2e5d8e");
  }
  return null;
}
