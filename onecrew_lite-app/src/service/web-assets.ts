import { lstat, readFile, readdir, realpath } from "node:fs/promises";
import { extname, join } from "node:path";
import { getMimeType } from "hono/utils/mime";
import { createHash } from "node:crypto";

export class WebAssets {
  readonly #assets: ReadonlyMap<string, Readonly<{ bytes: Uint8Array; type: string }>>;
  private constructor(assets: Map<string, { bytes: Uint8Array; type: string }>) { this.#assets = assets; }

  static async load(directory: string): Promise<WebAssets> {
    const root = await realpath(directory);
    const rootInfo = await lstat(directory);
    if (!rootInfo.isDirectory() || rootInfo.isSymbolicLink()) throw new Error("Web assets require a regular build directory");
    const assets = new Map<string, { bytes: Uint8Array; type: string }>();
    let total = 0;
    async function visit(path: string, prefix: string): Promise<void> {
      for (const entry of await readdir(path, { withFileTypes: true })) {
        const child = join(path, entry.name), url = `${prefix}/${entry.name}`;
        const info = await lstat(child);
        if (info.isSymbolicLink()) throw new Error("Web assets contain a symbolic link");
        if (info.isDirectory()) { await visit(child, url); continue; }
        if (!info.isFile()) throw new Error("Web assets contain a non-regular entry");
        if (url.startsWith("/api/") || url.startsWith("/_local/") || url === "/api" || url === "/_local") throw new Error("Web assets overlap a service route");
        if (extname(child) === ".map") continue;
        const type = getMimeType(child);
        if (!type) throw new Error("Web asset has an unsupported media type");
        total += info.size;
        if (info.size > 16 * 1024 * 1024 || total > 64 * 1024 * 1024) throw new Error("Web assets exceed the build resource budget");
        assets.set(url, { bytes: new Uint8Array(await readFile(child)), type });
      }
    }
    await visit(root, "");
    const index = assets.get("/index.html");
    if (!index || index.type.split(";")[0] !== "text/html") throw new Error("Web build has no index document");
    assets.set("/", index);
    return new WebAssets(assets);
  }
  has(path: string): boolean { return this.#assets.has(path); }
  fingerprint(): string {
    const hash = createHash("sha256");
    for (const [path, asset] of [...this.#assets].sort(([a], [b]) => Buffer.compare(Buffer.from(a), Buffer.from(b)))) {
      for (const bytes of [Buffer.from(path), Buffer.from(asset.type), asset.bytes]) {
        const length = Buffer.alloc(8); length.writeBigUInt64BE(BigInt(bytes.byteLength));
        hash.update(length).update(bytes);
      }
    }
    return hash.digest("hex");
  }
  response(path: string, method: string): Response {
    const asset = this.#assets.get(path);
    if (!asset || !["GET", "HEAD"].includes(method)) throw new Error("Web asset is unavailable");
    return new Response(method === "HEAD" ? null : new Uint8Array(asset.bytes), { headers: {
      "Content-Type": asset.type,
      "Cache-Control": "no-store", "X-Content-Type-Options": "nosniff", "Referrer-Policy": "no-referrer",
    } });
  }
}
