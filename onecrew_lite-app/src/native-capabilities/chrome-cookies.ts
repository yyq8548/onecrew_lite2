import { execFile } from "node:child_process";
import { createDecipheriv, createHash, pbkdf2Sync, timingSafeEqual } from "node:crypto";
import { closeSync, constants, fstatSync, lstatSync, openSync, readFileSync, realpathSync } from "node:fs";
import { homedir } from "node:os";
import { isAbsolute, join, relative, resolve, sep } from "node:path";
import { DatabaseSync } from "node:sqlite";
import { promisify } from "node:util";
import { z } from "zod";
import { parseApplicationJson } from "../contracts/application-json.ts";

const runFile = promisify(execFile);
const utf8 = new TextDecoder("utf-8", { fatal: true });
const rawCookieSchema = z.object({
  host_key: z.string(),
  name: z.string(),
  value: z.string(),
  encrypted_value: z.instanceof(Uint8Array),
  path: z.string(),
  is_secure: z.union([z.literal(0), z.literal(1)]),
  is_httponly: z.union([z.literal(0), z.literal(1)]),
});
const localStateSchema = z.object({
  os_crypt: z.object({ encrypted_key: z.base64() }),
});

export interface BrowserCookie {
  name: string;
  value: string;
  domain: string;
  path: string;
  secure: boolean;
  http_only: boolean;
}

function isWithin(parent: string, child: string): boolean {
  const difference = relative(parent, child);
  return difference === "" || (!isAbsolute(difference) && difference !== ".." && !difference.startsWith(`..${sep}`));
}

function checkedProfileDirectory(profileDirectory: string): string {
  if (lstatSync(profileDirectory).isSymbolicLink()) throw new Error("Chrome profile directory must not be a symbolic link");
  const directory = realpathSync(profileDirectory);
  if (!lstatSync(directory).isDirectory()) throw new Error("Chrome profile directory is not a directory");
  const defaultRoot = process.platform === "win32"
    ? join(z.string().min(1).parse(process.env.LOCALAPPDATA), "Google", "Chrome", "User Data")
    : join(homedir(), "Library", "Application Support", "Google", "Chrome");
  const existingDefaultRoot = lstatSync(defaultRoot, { throwIfNoEntry: false });
  if (isWithin(existingDefaultRoot ? realpathSync(defaultRoot) : resolve(defaultRoot), directory)) {
    throw new Error("Importing the default Chrome profile is prohibited");
  }
  return directory;
}

function cookieStorePath(directory: string): string {
  const modern = join(directory, "Default", "Network", "Cookies");
  const legacy = join(directory, "Default", "Cookies");
  const candidate = lstatSync(modern, { throwIfNoEntry: false }) ? modern : legacy;
  if (!lstatSync(candidate, { throwIfNoEntry: false })) throw new Error("Chrome cookie store was not found");
  const resolved = realpathSync(candidate);
  if (!isWithin(directory, resolved) || !lstatSync(candidate).isFile() || lstatSync(candidate).isSymbolicLink() || lstatSync(candidate).nlink !== 1) {
    throw new Error("Chrome cookie store escapes the profile directory");
  }
  return resolved;
}

function localStateBytes(directory: string): Buffer {
  const path = join(directory, "Local State"), before = lstatSync(path, { bigint: true });
  if (!before.isFile() || before.isSymbolicLink() || before.nlink !== 1n || !isWithin(directory, realpathSync(path))) throw new Error("Chrome Local State is not an owned regular file");
  const fd = openSync(path, constants.O_RDONLY | (constants.O_NOFOLLOW ?? 0));
  try {
    const opened = fstatSync(fd, { bigint: true });
    if (opened.dev !== before.dev || opened.ino !== before.ino) throw new Error("Chrome Local State changed");
    const bytes = readFileSync(fd), after = fstatSync(fd, { bigint: true }), current = lstatSync(path, { bigint: true });
    if (after.size !== opened.size || after.mtimeNs !== opened.mtimeNs || after.ctimeNs !== opened.ctimeNs || current.dev !== opened.dev || current.ino !== opened.ino || current.isSymbolicLink() || current.nlink !== 1n || current.mtimeNs !== after.mtimeNs || current.ctimeNs !== after.ctimeNs) throw new Error("Chrome Local State changed");
    return bytes;
  } finally { closeSync(fd); }
}

async function profileKey(directory: string): Promise<Buffer> {
  if (process.platform === "win32") {
    const { Dpapi, isPlatformSupported } = await import("@primno/dpapi");
    if (!isPlatformSupported) throw new Error("Windows DPAPI native binding is unavailable");
    const state = localStateSchema.parse(parseApplicationJson(localStateBytes(directory)));
    const wrapped = Buffer.from(state.os_crypt.encrypted_key, "base64");
    if (!wrapped.subarray(0, 5).equals(Buffer.from("DPAPI"))) {
      throw new Error("Chrome encryption key has no DPAPI header");
    }
    const unwrapped = Dpapi.unprotectData(wrapped.subarray(5), null, "CurrentUser");
    const key = Buffer.from(unwrapped);
    unwrapped.fill(0);
    if (key.length !== 32) {
      key.fill(0);
      throw new Error("Chrome DPAPI key must contain 32 bytes");
    }
    return key;
  }
  if (process.platform === "darwin") {
    const { stdout } = await runFile("/usr/bin/security", [
      "find-generic-password", "-w", "-s", "Chrome Safe Storage", "-a", "Chrome",
    ], { encoding: "buffer", timeout: 30_000, maxBuffer: 65_536 });
    const password = stdout.at(-1) === 10 ? stdout.subarray(0, -1) : stdout;
    try {
      return pbkdf2Sync(password, "saltysalt", 1003, 16, "sha1");
    } finally {
      stdout.fill(0);
    }
  }
  throw new Error("Chrome cookie migration supports macOS and Windows only");
}

function decryptCookie(key: Buffer, encryptedValue: Uint8Array): Buffer {
  const encrypted = Buffer.from(encryptedValue);
  if (encrypted.subarray(0, 3).equals(Buffer.from("v20"))) {
    throw new Error("Chrome App-Bound v20 cookies are unsupported");
  }
  if (!encrypted.subarray(0, 3).equals(Buffer.from("v10"))) {
    throw new Error("Unsupported Chrome cookie encryption format");
  }
  if (process.platform === "win32") {
    if (encrypted.length < 31) throw new Error("Truncated Chrome AES-GCM cookie");
    const decipher = createDecipheriv("aes-256-gcm", key, encrypted.subarray(3, 15));
    decipher.setAuthTag(encrypted.subarray(-16));
    return Buffer.concat([decipher.update(encrypted.subarray(15, -16)), decipher.final()]);
  }
  if (process.platform === "darwin") {
    const decipher = createDecipheriv("aes-128-cbc", key, Buffer.alloc(16, 0x20));
    return Buffer.concat([decipher.update(encrypted.subarray(3)), decipher.final()]);
  }
  throw new Error("Unsupported Chrome cookie platform");
}

export function hostMatchesSuffix(host: string, suffix: string): boolean {
  const domain = host.replace(/^\.+/, "");
  const expected = suffix.replace(/^\.+/, "");
  return expected.length > 0 && (domain === expected || domain.endsWith(`.${expected}`));
}

// 调用前必须关闭 OneCrew 自有 Chrome；只读打开保留锁冲突和权限错误。
export async function readChromeCookies(profileDirectory: string, hostSuffixes: readonly string[]): Promise<BrowserCookie[]> {
  const directory = checkedProfileDirectory(profileDirectory);
  const database = new DatabaseSync(cookieStorePath(directory), { readOnly: true });
  let key: Buffer | undefined;
  try {
    const versionRow = database.prepare("SELECT value FROM meta WHERE key = 'version'").get();
    const version = Number(z.string().regex(/^(0|[1-9][0-9]*)$/).parse(versionRow?.value));
    if (!Number.isSafeInteger(version) || version < 0) throw new Error("Invalid Chrome cookie schema version");
    key = await profileKey(directory);
    const cookies: BrowserCookie[] = [];
    for (const row of database.prepare(
      "SELECT host_key, name, value, encrypted_value, path, is_secure, is_httponly FROM cookies",
    ).iterate()) {
      const raw = rawCookieSchema.parse(row);
      if (hostSuffixes.length > 0 && !hostSuffixes.some((suffix) => hostMatchesSuffix(raw.host_key, suffix))) continue;
      let value = raw.value;
      if (raw.encrypted_value.length > 0) {
        const plaintext = decryptCookie(key, raw.encrypted_value);
        try {
          const prefixLength = version >= 24 ? 32 : 0;
          if (prefixLength > 0) {
            const expected = createHash("sha256").update(raw.host_key, "utf8").digest();
            if (plaintext.length < 32 || !timingSafeEqual(plaintext.subarray(0, 32), expected)) {
              throw new Error("Chrome cookie host integrity verification failed");
            }
          }
          value = utf8.decode(plaintext.subarray(prefixLength));
        } finally {
          plaintext.fill(0);
        }
      }
      if (value.length > 0) cookies.push({
        name: raw.name, value, domain: raw.host_key, path: raw.path,
        secure: raw.is_secure === 1, http_only: raw.is_httponly === 1,
      });
    }
    return cookies;
  } finally {
    key?.fill(0);
    database.close();
  }
}
