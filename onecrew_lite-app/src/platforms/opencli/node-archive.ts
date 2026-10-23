import { createWriteStream } from "node:fs";
import { mkdir } from "node:fs/promises";
import { dirname, join } from "node:path";
import { Transform } from "node:stream";
import { pipeline } from "node:stream/promises";
import yauzl from "yauzl";

// 由 yauzl 解析归档；仅把固定 Node 根目录中的普通文件发布到新建的暂存目录。
export async function extractNodeZip(archive: string, destination: string, expectedRoot: string): Promise<void> {
  const zip = await new Promise<yauzl.ZipFile>((resolve, reject) => yauzl.open(archive, { lazyEntries: true, validateEntrySizes: true, strictFileNames: true }, (error, value) => error || !value ? reject(error ?? new Error("Node archive could not be opened")) : resolve(value)));
  if (zip.entryCount > 30000) { zip.close(); throw new Error("Node archive contains too many entries"); }
  await new Promise<void>((resolve, reject) => {
    let total = 0; const seen = new Set<string>(); let settled = false;
    const fail = (error: unknown) => { if (!settled) { settled = true; zip.close(); reject(error); } };
    zip.on("error", fail);
    zip.on("end", () => { if (!settled) { settled = true; resolve(); } });
    zip.on("entry", (entry: yauzl.Entry) => {
      void (async () => {
        const name = entry.fileName; const parts = name.replace(/\/$/u, "").split("/");
        const mode = entry.externalFileAttributes >>> 16;
        total += entry.uncompressedSize;
        if (!name.isWellFormed() || name.includes("\\") || name.includes(":") || name.includes("\0") || parts[0] !== expectedRoot || parts.some(part => !part || part === "." || part === "..") || total > 512 * 1024 * 1024 || (mode & 0o170000) === 0o120000 || seen.has(name.toLowerCase())) throw new Error("Node archive entry is unsafe");
        seen.add(name.toLowerCase()); const path = join(destination, ...parts);
        if (name.endsWith("/")) { await mkdir(path, { recursive: true, mode: 0o700 }); return; }
        await mkdir(dirname(path), { recursive: true, mode: 0o700 });
        const stream = await new Promise<NodeJS.ReadableStream>((resolve, reject) => zip.openReadStream(entry, (error, value) => error || !value ? reject(error ?? new Error("Node archive stream is unavailable")) : resolve(value)));
        let size = 0;
        const guard = new Transform({ transform(chunk: Buffer, _encoding, callback) { size += chunk.length; callback(size > entry.uncompressedSize ? new Error("Node archive size does not match") : null, chunk); } });
        await pipeline(stream, guard, createWriteStream(path, { flags: "wx", mode: 0o600 }));
        if (size !== entry.uncompressedSize) throw new Error("Node archive size does not match");
      })().then(() => zip.readEntry(), fail);
    });
    zip.readEntry();
  });
}
