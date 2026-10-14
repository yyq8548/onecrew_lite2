import type { LocalDatabase } from '../../database.js';
type SearchJob = {
  id: string;
  tenant_id: string;
  platform: string;
  query: string;
  limit: number;
  status: string;
  error: string | null;
  result_json: string | null;
  created_at: string;
  updated_at: string;
};
const searchDb = (db: LocalDatabase['db']) => db.withTables<{ onecrew_search_jobs: SearchJob }>();
export const recoverInterruptedSearches = async (baseDb: LocalDatabase['db']) => {
  const db = searchDb(baseDb);
  await db
    .updateTable('onecrew_search_jobs')
    .set({
      status: 'failed',
      error: 'Search interrupted by server restart',
      updated_at: new Date().toISOString(),
    })
    .where('status', 'in', ['queued', 'running'])
    .execute();
};
