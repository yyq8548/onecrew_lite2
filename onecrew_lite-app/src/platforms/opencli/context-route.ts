import { lstat, readFile } from "node:fs/promises";
import { join } from "node:path";
import { z } from "zod";
import { parseApplicationJson } from "../../contracts/application-json.ts";
import { verifyPrivateFile } from "../../storage/private-backup.ts";
import { OpenCliError } from "./errors.ts";

export async function requireExactOpenCliContextRoute(configurationDirectory: string, contextId: string): Promise<void> {
  const path = join(configurationDirectory, "browser-profiles.json");
  const info = await lstat(path).then(value => value, error => { if (error.code === "ENOENT") return undefined; throw error; });
  if (!info) return;
  if (!info.isFile() || info.isSymbolicLink() || info.nlink !== 1 || info.size > 256 * 1024) throw new OpenCliError("opencli_profile_unavailable");
  await verifyPrivateFile(path);
  const config = z.object({ aliases: z.record(z.string(), z.string()).optional() }).passthrough().parse(parseApplicationJson(new TextDecoder("utf-8", { fatal: true }).decode(await readFile(path))));
  if (config.aliases && Object.hasOwn(config.aliases, contextId) && config.aliases[contextId] !== contextId) throw new OpenCliError("opencli_profile_unavailable");
  const after = await lstat(path);
  if (after.dev !== info.dev || after.ino !== info.ino || after.size !== info.size || after.mtimeMs !== info.mtimeMs) throw new OpenCliError("opencli_profile_unavailable");
}
