import { createHash } from "node:crypto";
import { posix } from "node:path";
import { valid } from "semver";
import { z } from "zod";
import { parseApplicationJson } from "../contracts/application-json.ts";

export const payloadDigest = (bytes: Uint8Array | string) => createHash("sha256").update(bytes).digest("hex");
const digest = z.string().regex(/^[a-f0-9]{64}$/u);
export const payloadPathSchema = z.string().min(1).max(1024).refine(value => !value.includes("\\") && !value.includes(":") && !value.includes("\0") &&
  !value.startsWith("/") && value !== "." && posix.normalize(value) === value && value.split("/").every(part => part !== ".." && part !== "" && !/[. ]$/u.test(part) && !/[<>"|?*\u0000-\u001f]/u.test(part) && !/^(?:CON|PRN|AUX|NUL|COM[1-9]|LPT[1-9])(?:\.|$)/iu.test(part)), "Invalid payload path");
const productVersion = z.string().max(32).refine(value => valid(value) === value, "Invalid product version");
export const payloadFileSchema = z.object({ path: payloadPathSchema, size: z.number().int().min(0).max(512 * 1024 * 1024), sha256: digest, executable: z.boolean() }).strict();
export const payloadBaseSchema = z.object({
  format: z.literal(1), product: z.literal("onecrew_lite"), state: z.literal("candidate"), productVersion,
  builtAt: z.number().int().positive().safe().optional(),
  target: z.enum(["win32-x64", "darwin-arm64", "darwin-x64"]), nodeVersion: z.literal("v26.7.0"),
  source: z.object({ baselineRepository: z.literal("https://github.com/yyq8548/onecrew_lite.git"), baselineCommit: z.literal("efc3c6eb69507b9809bb30279d9af26156d075e3"),
    checkoutCommit: z.string().regex(/^[a-f0-9]{40}$/u), sourceTreeSha256: digest, dirty: z.boolean() }).strict(),
  lockfileSha256: digest,
  components: z.array(z.object({ name: z.string().min(1), version: z.string().min(1), license: z.string().min(1), licenseFiles: z.array(payloadPathSchema).min(1), licenseEvidence: z.enum(["full_text", "declared_only"]).default("full_text"),
    licenseProvenance: z.array(z.object({ file: payloadPathSchema, source: z.url().refine(value => new URL(value).protocol === "https:"), sha256: digest }).strict()).min(1).optional() }).strict()).min(1),
  files: z.array(payloadFileSchema).min(1).max(50_000),
}).strict();
export const payloadManifestSchema = payloadBaseSchema.extend({ payloadSha256: digest }).strict();
export type PayloadManifest = z.infer<typeof payloadManifestSchema>;

function validateFiles(manifest: z.infer<typeof payloadBaseSchema>): void {
  const names = manifest.files.map(file => file.path), unique = new Set(names);
  if (unique.size !== names.length || names.some((name, index) => index > 0 && Buffer.compare(Buffer.from(names[index - 1]!), Buffer.from(name)) >= 0)) throw new Error("package.payload_manifest_order_invalid");
  const folded = new Set(names.map(name => name.toLowerCase()));
  if (folded.size !== names.length || folded.has(".complete") || folded.has("runtime-manifest.json")) throw new Error("package.payload_path_collision");
  for (const name of names) { const parts = name.toLowerCase().split("/"); parts.pop(); while (parts.length) { if (folded.has(parts.join("/"))) throw new Error("package.payload_path_collision"); parts.pop(); } }
  for (const required of ["app/cli.mjs", "resources/web/index.html", "resources/library-web/index.html", "resources/storage/private-backup.ps1", "build/pnpm-lock.yaml"]) {
    if (!unique.has(required)) throw new Error("package.payload_required_resource_missing");
  }
  for (const component of manifest.components) for (const license of component.licenseFiles) if (!unique.has(license)) throw new Error("package.payload_license_missing");
  for (const component of manifest.components) for (const evidence of component.licenseProvenance ?? []) {
    if (component.licenseEvidence !== "full_text" || !component.licenseFiles.includes(evidence.file)
      || manifest.files.find(file => file.path === evidence.file)?.sha256 !== evidence.sha256) throw new Error("package.payload_license_provenance_invalid");
  }
}
export function createPayloadManifest(value: unknown): PayloadManifest {
  const base = payloadBaseSchema.parse(value); validateFiles(base);
  return { ...base, payloadSha256: payloadDigest(JSON.stringify(base)) };
}
export function parsePayloadManifest(bytes: Uint8Array): PayloadManifest {
  const manifest = payloadManifestSchema.parse(parseApplicationJson(bytes));
  const { payloadSha256, ...base } = manifest; validateFiles(base);
  if (payloadDigest(JSON.stringify(base)) !== payloadSha256) throw new Error("package.payload_manifest_hash_invalid");
  return manifest;
}
