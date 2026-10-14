export async function isDatabaseReady(db: D1Database) {
  const row = await db
    .prepare("SELECT 1 AS ready")
    .first<{ ready: number }>();
  return row?.ready === 1;
}
