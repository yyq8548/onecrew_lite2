import { randomUUID } from "node:crypto";
import { open, rename } from "node:fs/promises";
import { join } from "node:path";
import { verifyPrivateFile } from "../storage/private-backup.ts";

export async function writeMigrationJournal(directory: string, value: unknown): Promise<void> {
  const temporary = join(directory, `journal-${randomUUID()}.json`);
  const file = await open(temporary, "wx", 0o600);
  try {
    await file.writeFile(JSON.stringify(value));
    await file.sync();
  } finally { await file.close(); }
  await verifyPrivateFile(temporary);
  await rename(temporary, join(directory, "journal.json"));
}
