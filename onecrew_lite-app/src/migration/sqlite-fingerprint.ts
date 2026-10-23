import { createHash } from "node:crypto";
import { DatabaseSync } from "node:sqlite";

export interface SqliteFingerprint {
  version: number;
  schemaSha256: string;
  tables: { name: string; rows: number; sha256: string }[];
}

function identifier(value: string): string {
  return `"${value.replaceAll('"', '""')}"`;
}

function digestFields(fields: readonly (string | Uint8Array)[]): string {
  const hash = createHash("sha256");
  for (const field of fields) {
    const bytes = typeof field === "string" ? Buffer.from(field, "utf8") : field;
    hash.update(String(bytes.byteLength)).update(":").update(bytes);
  }
  return hash.digest("hex");
}

// 在同一个只读事务中比较逻辑数据，保留 SQLite 类型、完整整数和原始文本字节。
export function fingerprintSqlite(path: string): SqliteFingerprint {
  const db = new DatabaseSync(path, { readOnly: true });
  try {
    db.exec("PRAGMA query_only=ON; PRAGMA busy_timeout=5000; BEGIN");
    const schema = db.prepare("SELECT type,name,tbl_name,sql FROM sqlite_schema WHERE name NOT LIKE 'sqlite_stat%' ORDER BY type,name").all();
    const schemaSha256 = digestFields(schema.map(row => JSON.stringify(row)));
    const names = db.prepare("SELECT name FROM sqlite_schema WHERE type='table' AND name NOT LIKE 'sqlite_stat%' ORDER BY name").all();
    const tables = names.map(row => {
      const name = String(row.name);
      const columns = db.prepare("SELECT name FROM pragma_table_xinfo(?) WHERE hidden != 1 ORDER BY cid").all(name).map(column => String(column.name));
      if (columns.length === 0) throw new Error("SQLite table has no readable columns");
      const projection = columns.flatMap(column => {
        const quoted = identifier(column);
        return [`typeof(${quoted})`, `CASE WHEN typeof(${quoted})='real' THEN NULL ELSE CAST(${quoted} AS BLOB) END`, `CASE WHEN typeof(${quoted})='real' THEN ${quoted} ELSE NULL END`];
      }).join(",");
      const statement = db.prepare(`SELECT ${projection} FROM ${identifier(name)}`);
      statement.setReturnArrays(true);
      const rows: string[] = [];
      for (const values of statement.iterate() as unknown as Iterable<(string | number | Uint8Array | null)[]>) {
        rows.push(digestFields(values.map(value => {
          if (value === null) return new Uint8Array();
          if (typeof value !== "number") return value;
          const bytes = Buffer.alloc(8);
          bytes.writeDoubleBE(value);
          return bytes;
        })));
      }
      rows.sort();
      return { name, rows: rows.length, sha256: digestFields([digestFields(columns), ...rows]) };
    });
    return { version: Number(db.prepare("PRAGMA user_version").get()!.user_version), schemaSha256, tables };
  } finally {
    db.close();
  }
}
