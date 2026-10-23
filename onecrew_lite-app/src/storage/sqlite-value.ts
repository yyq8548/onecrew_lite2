export function legacySqliteText(value: unknown): string {
  if (typeof value !== "string") throw new Error("Legacy SQLite value must have TEXT storage type");
  return value;
}
