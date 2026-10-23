import { readFileSync } from "node:fs";
import { runtimeAssetPath } from "../packaging/runtime-context.ts";

export function readStorageSql(file: string): string {
  if (!/^[a-z0-9-]+\.sql$/u.test(file)) throw new Error("storage.invalid_sql_resource");
  return readFileSync(runtimeAssetPath(`storage/${file}`, new URL(`./${file}`, import.meta.url)), "utf8");
}
