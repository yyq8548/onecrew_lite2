import { createHash } from "node:crypto";
import { lstat, readFile, realpath } from "node:fs/promises";
import { join, resolve, toNamespacedPath } from "node:path";
import { runtimeAssetPath } from "../packaging/runtime-context.ts";

export const mcpAppMime = "text/html;profile=mcp-app";
const defaultDirectory = runtimeAssetPath("mcp", new URL("../../../onecrew_lite-rust/crates/mcp-adapter/assets/apps/", import.meta.url));
const samePath = (left: string, right: string) => process.platform === "win32" ? toNamespacedPath(resolve(left)).toLowerCase() === toNamespacedPath(resolve(right)).toLowerCase() : left === right;

export async function loadMcpAppResources(directory = defaultDirectory) {
  const canonical = await realpath(directory);
  if (!samePath(canonical, resolve(directory))) throw new Error("mcp_app_directory_alias");
  async function load(name: string) {
    const path = join(canonical, name), before = await lstat(path, { bigint: true });
    if (!before.isFile() || before.isSymbolicLink() || before.nlink !== 1n || before.size < 1n || before.size > 2n * 1024n * 1024n) throw new Error("mcp_app_invalid_file");
    if (!samePath(await realpath(path), path)) throw new Error("mcp_app_file_alias");
    const bytes = await readFile(path), after = await lstat(path, { bigint: true });
    if (before.dev !== after.dev || before.ino !== after.ino || before.size !== after.size || before.mtimeNs !== after.mtimeNs || before.ctimeNs !== after.ctimeNs) throw new Error("mcp_app_changed_during_read");
    return { text: new TextDecoder("utf-8", { fatal: true }).decode(bytes), bytes };
  }
  const current = await load("operation-inspector.html"), retired = await load("retired-operation-inspector.html");
  const hash = createHash("sha256").update(current.bytes).digest("hex").slice(0, 16), uri = `ui://onecrew/operation-inspector/${hash}.html`;
  return Object.freeze({
    uri,
    list: () => ({ resources: [{ uri, name: "operation-inspector", title: "onecrew_lite action card", mimeType: mcpAppMime, size: current.bytes.length }] }),
    read(requested: string) {
      if (!/^ui:\/\/onecrew\/operation-inspector\/[0-9a-f]{16}\.html$/u.test(requested)) throw new Error("mcp_app_resource_not_found");
      return { contents: [{ uri: requested, mimeType: mcpAppMime, text: requested === uri ? current.text : retired.text }] };
    },
  });
}
