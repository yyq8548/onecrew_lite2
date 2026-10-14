import { z } from "zod";
export const targets = ["aarch64-apple-darwin", "x86_64-apple-darwin", "x86_64-pc-windows-msvc"] as const;
export const versionSchema = z.string().regex(/^\d+\.\d+\.\d+$/);
export const manifestSchema = z.object({
  schema_version: z.literal(1), product: z.literal("onecrew-cli"), version: versionSchema,
  assets: z.array(z.object({ target: z.enum(targets), key: z.string(), size: z.number().int().positive().safe(), sha256: z.string().regex(/^[a-f0-9]{64}$/) }).strict()).length(3),
}).strict().superRefine((value, context) => {
  if (new Set(value.assets.map(a => a.target)).size !== 3 || value.assets.some(a => a.key !== `releases/${value.version}/onecrew-${value.version}-${a.target}${a.target.endsWith("windows-msvc") ? ".exe" : ""}`)) context.addIssue({ code: "custom", message: "invalid_cli_assets" });
});
export type CliManifest = z.infer<typeof manifestSchema>;
export const legacyTicketPattern = /^[A-Za-z0-9_-]{43}$/;
export const credentialPattern = /^v1\.[A-Za-z0-9_-]{60,1536}$/;
export function cliMethods(path: string): readonly string[] | null {
  if (path === "/api/cli/update") return ["GET"];
  if (/^\/api\/cli\/update\/\d+\.\d+\.\d+\/files\/(?:aarch64-apple-darwin|x86_64-apple-darwin|x86_64-pc-windows-msvc)$/.test(path)) return ["GET", "HEAD"];
  if (path === "/api/cli/install-links") return ["GET", "POST"];
  if (/^\/cli\/install\/v1\.[A-Za-z0-9_-]{60,1536}$/.test(path)) return ["GET", "HEAD"];
  if (/^\/cli\/download\/v1\.[A-Za-z0-9_-]{60,1536}(?:\/install\.(?:sh|ps1)|\/files\/(?:aarch64-apple-darwin|x86_64-apple-darwin|x86_64-pc-windows-msvc))$/.test(path)) return ["GET", "HEAD"];
  if (/^\/cli\/install\/[A-Za-z0-9_-]{43}(?:\/install\.(?:sh|ps1)|\/files\/(?:aarch64-apple-darwin|x86_64-apple-darwin|x86_64-pc-windows-msvc))?$/.test(path)) return ["GET", "HEAD"];
  return null;
}
