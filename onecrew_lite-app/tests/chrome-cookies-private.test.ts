import { test } from "node:test";
import assert from "node:assert/strict";
import { createCipheriv, createHash, randomBytes, randomUUID } from "node:crypto";
import { mkdir, open, rm, writeFile } from "node:fs/promises";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { DatabaseSync } from "node:sqlite";
import { readChromeCookies } from "../src/native-capabilities/chrome-cookies.ts";
import { createPrivateDirectory } from "../src/storage/private-backup.ts";

test("owned Windows Chrome v24 SQLite uses real DPAPI and authenticates GCM, host prefix and strict Local State", { skip: process.platform !== "win32" }, async () => {
  const { Dpapi } = await import("@primno/dpapi");
  const scratch = fileURLToPath(new URL("../../.scratch/", import.meta.url)); await mkdir(scratch, { recursive: true });
  const root = await createPrivateDirectory(join(scratch, `cookie-${randomUUID()}`)), profile = await createPrivateDirectory(join(root, "owned-profile"));
  const directory = await createPrivateDirectory(join(profile, "Default")), network = await createPrivateDirectory(join(directory, "Network")), path = join(network, "Cookies");
  const key = randomBytes(32), nonce = randomBytes(12), cookieValue = randomBytes(24).toString("base64url"), host = ".x.com";
  const cipher = createCipheriv("aes-256-gcm", key, nonce), envelope = Buffer.concat([Buffer.from("v10"), nonce, cipher.update(Buffer.concat([createHash("sha256").update(host).digest(), Buffer.from(cookieValue)])), cipher.final(), cipher.getAuthTag()]);
  const state = { os_crypt: { encrypted_key: Buffer.concat([Buffer.from("DPAPI"), Dpapi.protectData(key, null, "CurrentUser")]).toString("base64") } };
  try {
    await writeFile(join(profile, "Local State"), JSON.stringify(state), { flag: "wx", mode: 0o600 });
    { await using file = await open(path, "wx", 0o600); }
    { using database = new DatabaseSync(path);
      database.exec("CREATE TABLE meta(key TEXT PRIMARY KEY,value TEXT NOT NULL); INSERT INTO meta VALUES('version','24'); CREATE TABLE cookies(host_key TEXT,name TEXT,value TEXT,encrypted_value BLOB,path TEXT,is_secure INTEGER,is_httponly INTEGER)");
      database.prepare("INSERT INTO cookies VALUES(?,'auth_token','',?,'/',1,1)").run(host, envelope); }
    const cookies = await readChromeCookies(profile, ["x.com"]); assert.equal(cookies.length, 1); assert.equal(cookies[0]!.value, cookieValue);
    assert.deepEqual(await readChromeCookies(profile, ["instagram.com"]), []);
    { using database = new DatabaseSync(path); database.prepare("UPDATE cookies SET host_key='.api.x.com'").run(); }
    await assert.rejects(readChromeCookies(profile, ["x.com"]), /host integrity/u);
    { using database = new DatabaseSync(path); database.prepare("UPDATE cookies SET host_key=?,encrypted_value=?").run(host, Buffer.concat([Buffer.from("v20"), envelope.subarray(3)])); }
    await assert.rejects(readChromeCookies(profile, ["x.com"]), /App-Bound/u);
    await writeFile(join(profile, "Local State"), `{"os_crypt":${JSON.stringify(state.os_crypt)},"os_crypt":${JSON.stringify(state.os_crypt)}}`);
    await assert.rejects(readChromeCookies(profile, ["x.com"]), /Duplicate key/u);
  } finally { key.fill(0); await rm(root, { recursive: true }); }
});
