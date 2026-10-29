import { lstat, readFile, readdir, mkdir, mkdtemp, open, rename } from "node:fs/promises";
import { createRequire } from "node:module";
import { dirname, isAbsolute, join, parse } from "node:path";
import { OpenCliError } from "./errors.ts";
const nativeLock = createRequire(import.meta.url)("fs-native-extensions") as { tryLock(fd: number): boolean };

// 只读适配器分发清单；测试文件不作为可执行插件分发。
export const OPENCLI_ADDON_FILES = Object.freeze({
  "onecrew-zhihu": ["cli.js", "read.mjs"],
  "onecrew-xiaohongshu": ["cli.js", "page.mjs", "read.mjs"],
  "onecrew-youtube": ["language.mjs", "language.js", "evidence.mjs", "channel.mjs", "channel-contact.js", "video-evidence.js", "hidden-email.mjs", "hidden-email-store.mjs"],
  "onecrew-tiktok": ["search.mjs", "search.js"],
  "onecrew-bilibili": ["search.mjs", "search.js"],
  "onecrew-tiktok-profile": ["profile.mjs", "read.js"],
  "onecrew-bilibili-profile": ["profile.mjs", "read.js"],
  "onecrew-bilibili-video": ["language-page.mjs", "language.mjs", "language.js", "video.mjs", "read.js"],
  "onecrew-twitter-identity": ["profile.mjs", "read.js"],
  "onecrew-twitter-post-identity": ["post.mjs", "read.js"],
  "onecrew-twitter-post-content": ["language.mjs", "content.mjs", "read.js"],
  "onecrew-site-auth": ["probe.mjs", "whoami.js"],
  "onecrew-browser-search": ["youtube-authors.mjs", "source-url.mjs", "instagram-visible.mjs", "douyin-evidence.mjs", "douyin-comments.mjs", "douyin-profile.js", "linkedin-search.mjs", "linkedin-evidence.mjs", "linkedin-comments.mjs", "linkedin-profile.js", "weibo-read.mjs", "weibo-read.js", "weibo-post.js", "instagram-comments.mjs", "instagram-comments.js", "facebook-comments.mjs", "facebook-reel-comments.mjs", "facebook-recent.mjs", "facebook-comments.js", "facebook-profile.mjs", "facebook-identity.mjs", "facebook-native-profile.mjs", "facebook-native-original.mjs", "facebook-profile.js", "instagram-profile.mjs", "instagram-profile.js", "tiktok-comments.mjs", "tiktok-comments.js", "tiktok-language-recent.mjs", "tiktok-language-recent.js", "instagram-search.mjs", "instagram-search.js", "instagram-recent.mjs", "instagram-recent.js", "comments.mjs", "comments.js", "original-language.mjs", "owned-page.mjs", "search.js"],
} as const);
for (const files of Object.values(OPENCLI_ADDON_FILES)) Object.freeze(files);
export type OpenCliAddonId = keyof typeof OPENCLI_ADDON_FILES;
export type OpenCliAddonState = "ready" | "missing" | "conflict" | "unavailable";

async function directoryState(path: string): Promise<OpenCliAddonState | null> {
  const paths = [];
  for (let current = path; ; current = dirname(current)) { paths.push(current); if (current === parse(current).root) break; }
  for (const current of paths.reverse()) {
    const result = await lstat(current).then(metadata => ({ metadata }), error => ({ error: error as NodeJS.ErrnoException }));
    if ("error" in result) return result.error.code === "ENOENT" ? "missing" : "unavailable";
    if (!result.metadata.isDirectory() || result.metadata.isSymbolicLink()) return "conflict";
  }
  return null;
}

export class OpenCliAddons {
  get configurationDirectory(): string { return dirname(this.#installedRoot); }
  readonly #bundledRoot: string;
  readonly #installedRoot: string;
  constructor(bundledRoot: string, userHome: string) {
    if (!isAbsolute(bundledRoot) || !isAbsolute(userHome)) throw new OpenCliError("opencli_invalid_input");
    this.#bundledRoot = bundledRoot;
    this.#installedRoot = join(userHome, ".opencli", "clis");
  }
  async #bundledFile(id: OpenCliAddonId, name: string): Promise<Buffer> {
    const source = join(this.#bundledRoot, id, name);
    const metadata = await lstat(source);
    if (!metadata.isFile() || metadata.isSymbolicLink()) throw new OpenCliError("opencli_invalid_output");
    return readFile(source);
  }
  async #state(id: OpenCliAddonId, expectedFile: (name: string) => Promise<Buffer>): Promise<OpenCliAddonState> {
    const directory = join(this.#installedRoot, id);
    const state = await directoryState(directory);
    if (state) return state;
    const entries = await readdir(directory).then(value => ({ value }), error => ({ error: error as NodeJS.ErrnoException }));
    if ("error" in entries) return "unavailable";
    const names = OPENCLI_ADDON_FILES[id];
    if (entries.value.length !== names.length || entries.value.some(name => !names.some(expected => expected === name))) return "conflict";
    for (const name of names) {
      const expected = await expectedFile(name);
      const path = join(directory, name);
      const metadata = await lstat(path).then(value => ({ value }), error => ({ error: error as NodeJS.ErrnoException }));
      if ("error" in metadata) return metadata.error.code === "ENOENT" ? "missing" : "unavailable";
      if (!metadata.value.isFile() || metadata.value.isSymbolicLink() || metadata.value.size !== expected.length) return "conflict";
      const actual = await readFile(path).then(value => ({ value }), error => ({ error: error as NodeJS.ErrnoException }));
      if ("error" in actual) return "unavailable";
      if (!actual.value.equals(expected)) return "conflict";
    }
    return "ready";
  }
  async state(id: OpenCliAddonId): Promise<OpenCliAddonState> { return this.#state(id, name => this.#bundledFile(id, name)); }
  async requireReady(id: OpenCliAddonId): Promise<void> { if (await this.state(id) !== "ready") throw new OpenCliError("opencli_read_command_unavailable"); }
  async prepare(ids: readonly OpenCliAddonId[]): Promise<() => Promise<void>> {
    const snapshots = await Promise.all([...new Set(ids)].map(async id => {
      const files = new Map<string, Buffer>();
      const state = await this.#state(id, async name => {
        const bytes = await this.#bundledFile(id, name);
        files.set(name, bytes);
        return bytes;
      });
      if (state !== "ready") throw new OpenCliError("opencli_read_command_unavailable");
      return { id, files };
    }));
    return async () => {
      await Promise.all(snapshots.map(async ({ id, files }) => {
        if (await this.#state(id, async name => files.get(name)!) !== "ready") throw new OpenCliError("opencli_read_command_unavailable");
      }));
    };
  }
  async status(): Promise<{ id: OpenCliAddonId; state: OpenCliAddonState }[]> {
    return Promise.all((Object.keys(OPENCLI_ADDON_FILES) as OpenCliAddonId[]).map(async id => ({ id, state: await this.state(id) })));
  }
  async #parents(): Promise<void> {
    const config = dirname(this.#installedRoot); const home = dirname(config);
    if (await directoryState(home) !== null) throw new OpenCliError("opencli_unavailable");
    for (const path of [config, this.#installedRoot]) {
      const state = await directoryState(path);
      if (state === "missing") await mkdir(path, { mode: 0o700 });
      else if (state !== null) throw new OpenCliError("opencli_unavailable");
    }
  }
  async #stage(directory: string, id: OpenCliAddonId): Promise<void> {
    await mkdir(directory, { mode: 0o700 });
    for (const name of OPENCLI_ADDON_FILES[id]) {
      const source = join(this.#bundledRoot, id, name); const metadata = await lstat(source);
      if (!metadata.isFile() || metadata.isSymbolicLink()) throw new OpenCliError("opencli_invalid_output");
      const bytes = await readFile(source); const file = await open(join(directory, name), "wx", 0o600);
      try { await file.writeFile(bytes); await file.sync(); } finally { await file.close(); }
    }
  }
  // 仅专用安装操作调用；读取命令永远不会自动安装或修复。
  async install(approved: boolean): Promise<{ id: OpenCliAddonId; state: OpenCliAddonState }[]> { return this.#publish(approved, false); }
  async repair(approved: boolean): Promise<{ id: OpenCliAddonId; state: OpenCliAddonState }[]> { return this.#publish(approved, true); }
  async #publish(approved: boolean, repair: boolean): Promise<{ id: OpenCliAddonId; state: OpenCliAddonState }[]> {
    if (approved !== true) throw new OpenCliError("opencli_invalid_input");
    const before = await this.status();
    if (before.every(item => item.state === "ready")) return before;
    if (before.some(item => item.state === "unavailable" || !repair && item.state === "conflict")) throw new OpenCliError("opencli_read_command_unavailable");
    await this.#parents();
    const config = dirname(this.#installedRoot); const lockPath = join(config, "onecrew-repair.lock");
    const lockInfo = await lstat(lockPath).then(value => value, error => { if ((error as NodeJS.ErrnoException).code === "ENOENT") return null; throw error; });
    if (lockInfo && (!lockInfo.isFile() || lockInfo.isSymbolicLink())) throw new OpenCliError("opencli_unavailable");
    const lease = await open(lockPath, "a+", 0o600);
    try {
      if (!nativeLock.tryLock(lease.fd)) throw new OpenCliError("opencli_unavailable");
      const opened = await lease.stat(); const actual = await lstat(lockPath);
      if (actual.isSymbolicLink() || actual.dev !== opened.dev || actual.ino !== opened.ino) throw new OpenCliError("opencli_unavailable");
      const current = await this.status();
      if (current.some(item => item.state === "unavailable" || !repair && item.state === "conflict")) throw new OpenCliError("opencli_read_command_unavailable");
      const selected = current.filter(item => item.state !== "ready");
      const stage = await mkdtemp(join(config, "onecrew-staging-"));
      const backups = join(config, "onecrew-backups"); const backupState = await directoryState(backups);
      if (backupState === "missing") await mkdir(backups, { mode: 0o700 }); else if (backupState !== null) throw new OpenCliError("opencli_unavailable");
      const backup = await mkdtemp(join(backups, "addons-"));
      for (const item of selected) await this.#stage(join(stage, item.id), item.id);
      const moved: OpenCliAddonId[] = []; const published: OpenCliAddonId[] = [];
      const result = await (async () => {
        for (const item of selected) {
          const destination = join(this.#installedRoot, item.id);
          if (item.state === "conflict") {
            const info = await lstat(destination);
            if (!info.isDirectory() || info.isSymbolicLink()) throw new OpenCliError("opencli_unavailable");
            await rename(destination, join(backup, item.id)); moved.push(item.id);
          }
          await rename(join(stage, item.id), destination); published.push(item.id);
        }
        const after = await this.status();
        if (after.some(item => item.state !== "ready")) throw new OpenCliError("opencli_read_command_unavailable");
        return after;
      })().then(value => ({ value }), error => ({ error }));
      if ("error" in result) {
        for (const id of published.reverse()) await rename(join(this.#installedRoot, id), join(stage, id));
        for (const id of moved.reverse()) await rename(join(backup, id), join(this.#installedRoot, id));
        throw result.error;
      }
      return result.value;
    } finally { await lease.close(); }
  }
}
