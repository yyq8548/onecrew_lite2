import { createDecipheriv } from "node:crypto";
import { constants } from "node:fs";
import { lstat, open } from "node:fs/promises";
import { resolve } from "node:path";
import { DatabaseSync } from "node:sqlite";
import { verifyPrivateFile } from "./private-backup.ts";
import { validateVaultSchema } from "./vault-schema.ts";
import { verifyLegacyPrivateFile } from "./legacy-private-source.ts";

export async function readVaultMasterKey(path: string, contract: "private-copy" | "legacy-source" = "private-copy"): Promise<Buffer> {
  const absolute = resolve(path);
  const verify = contract === "legacy-source" ? verifyLegacyPrivateFile : verifyPrivateFile;
  await verify(absolute);
  const before = await lstat(absolute, { bigint: true });
  if (!before.isFile() || before.isSymbolicLink() || before.size !== 32n || before.nlink !== 1n) {
    throw new Error("Vault master key must be a regular 32-byte file with exactly one hard link");
  }
  const file = await open(absolute, constants.O_RDONLY | (constants.O_NOFOLLOW ?? 0));
  const bytes = Buffer.alloc(33);
  try {
    const opened = await file.stat({ bigint: true });
    if (!opened.isFile() || opened.dev !== before.dev || opened.ino !== before.ino || opened.size !== 32n || opened.nlink !== 1n) {
      throw new Error("Vault master key identity changed while opening");
    }
    let count = 0;
    while (count < bytes.length) {
      const result = await file.read(bytes, count, bytes.length - count, count);
      if (result.bytesRead === 0) break;
      count += result.bytesRead;
    }
    if (count !== 32) throw new Error("Vault master key must contain exactly 32 bytes");
    await verify(absolute);
    const after = await lstat(absolute, { bigint: true });
    const final = await file.stat({ bigint: true });
    if (after.isSymbolicLink() || !after.isFile() || after.dev !== opened.dev || after.ino !== opened.ino ||
        after.size !== 32n || after.nlink !== 1n || final.size !== 32n || final.nlink !== 1n ||
        final.mtimeNs !== opened.mtimeNs || final.ctimeNs !== opened.ctimeNs) {
      throw new Error("Vault master key changed while reading");
    }
    return Buffer.from(bytes.subarray(0, 32));
  } finally {
    bytes.fill(0);
    await file.close();
  }
}

// Rust aes_gcm::Aead 使用 12 字节 nonce + 密文 + 16 字节认证标签，无 AAD。
// 调用者负责清零输入密钥和返回明文字节，不应记录或持久保存明文。
export function decryptVaultEnvelope(key: Uint8Array, envelope: Uint8Array): Buffer {
  if (key.byteLength !== 32) throw new Error("Vault master key must contain exactly 32 bytes");
  if (envelope.byteLength < 28) throw new Error("Vault encrypted envelope is truncated");
  const decipher = createDecipheriv("aes-256-gcm", key, envelope.subarray(0, 12), { authTagLength: 16 });
  decipher.setAuthTag(envelope.subarray(envelope.byteLength - 16));
  const partial = decipher.update(envelope.subarray(12, envelope.byteLength - 16));
  try {
    return Buffer.concat([partial, decipher.final()]);
  } finally {
    partial.fill(0);
  }
}

export type VaultSecretTable = "profiles" | "service_credentials";

// 仅接受已升级到 Rust v4 的数据库；返回原始明文字节，避免擅自更改各凭据 JSON 格式。
export function readVaultSecret(path: string, key: Uint8Array, table: VaultSecretTable, id: string): Buffer {
  if (table !== "profiles" && table !== "service_credentials") throw new Error("Unsupported vault secret table");
  const db = new DatabaseSync(path, { readOnly: true });
  try {
    db.exec("PRAGMA query_only = ON; PRAGMA busy_timeout = 5000; BEGIN");
    if (Number(db.prepare("PRAGMA user_version").get()!.user_version) !== 4) throw new Error("Vault secret reading requires schema version 4");
    validateVaultSchema(db);
    const row = db.prepare(`SELECT encrypted_secret FROM ${table} WHERE id = ?`).get(id);
    if (!row) throw new Error("Vault secret was not found");
    if (!(row.encrypted_secret instanceof Uint8Array)) throw new Error("Vault encrypted_secret must be a BLOB");
    return decryptVaultEnvelope(key, row.encrypted_secret);
  } finally {
    db.close();
  }
}
