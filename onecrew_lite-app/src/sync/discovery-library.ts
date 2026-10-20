import { isDeepStrictEqual } from "node:util";
import { TableDataError, type createEmbeddedTables } from "onecrew_lite_tables/embedded";
import type { AccountContext, AccountContextService } from "../identity/account-context.ts";
import type { AccountDatabases } from "../identity/account-databases.ts";

export class LibraryProjectionError extends Error {
  readonly status = 503;
  readonly code: string;
  constructor(code: string, options?: ErrorOptions) { super(code, options); this.code = code; }
}

export function mapLibraryProjectionError(error: unknown, write: boolean): never {
  if (!(error instanceof TableDataError)) throw error;
  const stale = new Set(["data.projection_stale", "data.identity_stale"]);
  const conflicts = new Set([
    "data.identity_source_version_conflict", "data.identity_version_conflict",
    "data.identity_sources_changed", "data.identity_sources_incomplete", "data.identity_stored_invalid",
    "data.projection_version_conflict", "data.projection_row_conflict", "data.projection_table_conflict",
    "data.projection_readback_conflict", "data.projection_field_conflict", "data.projection_field_name_conflict",
  ]);
  const code = stale.has(error.code) ? "library_source_stale"
    : conflicts.has(error.code) ? "library_projection_conflict"
    : error.code === "data.projection_row_missing" ? "library_row_missing"
    : error.code === "data.projection_unmanaged_table" ? "library_unmanaged_table"
    : [401, 403, 409].includes(error.status) ? "library_account_changed"
    : error.status === 404 ? "library_table_missing"
    : [400, 413].includes(error.status) ? "library_request_invalid"
    : error.status === 503 ? "library_service_unavailable"
    : write ? "library_write_failed" : "library_read_failed";
  throw new LibraryProjectionError(code, { cause: error });
}

function libraryCall<T>(work: () => Promise<T>, write = false): Promise<T> {
  return work().then(undefined, error => mapLibraryProjectionError(error, write));
}

const columns = [
  { name: "Campaign ID", data_type: "string" }, { name: "Search hit ID", data_type: "string" },
  { name: "Platform", data_type: "string" }, { name: "Author label", data_type: "string" },
  { name: "Followers", data_type: "string" }, { name: "Author ID", data_type: "string" },
  { name: "Author URL", data_type: "website" }, { name: "Content title", data_type: "string" },
  { name: "Source URL", data_type: "website" }, { name: "Search keyword", data_type: "string" },
  { name: "Status", data_type: "string" }, { name: "Captured at (Unix s)", data_type: "string" },
  { name: "Match evaluation", data_type: "score" }, { name: "Content category", data_type: "string" },
  { name: "Pinterest monthly views", data_type: "string" }, { name: "Pinterest scale passed by", data_type: "string" },
  { name: "Biography status", data_type: "string" },
] as const;
type Tables = Awaited<ReturnType<typeof createEmbeddedTables>>;
type SavedRow = Awaited<ReturnType<Tables["data"]["getDiscoveryState"]>>["rows"][number];
type IngestRow = Parameters<Tables["data"]["ingestDiscoverySources"]>[1]["rows"][number];
function timestamp(value: number | bigint): number {
  const result = Number(value);
  if (!Number.isSafeInteger(result) || result <= 0) throw new LibraryProjectionError("library_contact_timestamp_invalid");
  return result;
}

export function createDiscoveryLibraryProjection(accounts: AccountContextService, databases: AccountDatabases, tables: Tables, origin: string) {
  async function tenant(context: AccountContext) {
    accounts.assertCurrent(context);
    const actual = await tables.tenants.resolve(context.principal.user);
    accounts.assertCurrent(context);
    if (actual.tenantId !== context.principal.tenantId || actual.baseId !== context.principal.baseId) throw new LibraryProjectionError("library_account_changed");
    return actual;
  }
  async function readiness(context: AccountContext): Promise<void> {
    const selected = await tenant(context);
    await libraryCall(() => tables.service.listTables(selected, { page: 1, size: 1 }));
    accounts.assertCurrent(context);
  }
  return {
    readiness,
    async get(context: AccountContext, campaign: string) {
      const resolved = await databases.discoveryResolver().resolve(context);
      resolved.assertOwned(); resolved.repository.getPlan(campaign);
      const selected = await tenant(context);
      const table = await libraryCall(() => tables.data.getDiscoveryCampaignTable(selected, campaign));
      accounts.assertCurrent(context); resolved.assertOwned();
      if (!table) return null;
      const rows = await libraryCall(() => tables.data.queryRows(selected, table.id, 1, 1));
      accounts.assertCurrent(context); resolved.assertOwned();
      return { campaign_id: campaign, table_id: table.id, web_url: `${origin}/library/${encodeURIComponent(table.id)}`, total_hits: rows.total, inserted_rows: 0 };
    },
    async publish(context: AccountContext, campaign: string, revision: number) {
      const selected = await tenant(context);
      const resolved = await databases.discoveryResolver().resolve(context);
      const assertCurrent = () => { accounts.assertCurrent(context); resolved.assertOwned(); };
      assertCurrent();
      const publication = resolved.repository.prepareLibraryPublication(campaign, revision);
      const snapshot = publication.material.sources;
      const plan = resolved.repository.getPlan(campaign);
      if (plan.revision !== revision) throw new LibraryProjectionError("library_source_stale");
      if (snapshot.rows.length > 10_000 || new Set(snapshot.rows.map(row => row.hit.id)).size !== snapshot.rows.length) throw new LibraryProjectionError("library_capacity_exceeded");
      const name = resolved.repository.getLibraryTableName(campaign);
      const existing = await libraryCall(() => tables.data.getDiscoveryCampaignTable(selected, campaign)); assertCurrent();
      const repairValues = existing !== null && ["Content category", "Followers", "Pinterest monthly views", "Pinterest scale passed by", "Biography status"].some(field => !existing.columns.some(column => column.name === field));
      const table = await libraryCall(() => tables.data.ensureDiscoveryCampaignTable(selected, { campaign_id: campaign, name: name.name, previous_auto_name: name.previous_auto_name, columns: columns.map(column => ({ ...column })) }), true); assertCurrent();
      const fields = new Map<string, string>();
      for (const column of columns) {
        const matches = table.columns.filter(field => field.name === column.name && field.data_type === column.data_type);
        if (matches.length !== 1) throw new LibraryProjectionError("library_table_schema_changed");
        fields.set(column.name, matches[0]!.id);
      }
      const state = async () => {
        const rows = new Map<string, SavedRow>();
        let after: string | undefined;
        for (;;) {
          const cursor = after;
          const page = await libraryCall(() => tables.data.getDiscoveryState(selected, { table_id: table.id, campaign_id: campaign, limit: 100, ...(cursor ? { after_hit_id: cursor } : {}) })); assertCurrent();
          if (page.table_id !== table.id || page.campaign_id !== campaign) throw new LibraryProjectionError("library_readback_incomplete");
          for (const row of page.rows) {
            if (rows.has(row.search_hit_id)) throw new LibraryProjectionError("library_readback_incomplete");
            rows.set(row.search_hit_id, row);
          }
          if (!page.has_more) return rows;
          const last = page.rows.at(-1)?.search_hit_id;
          if (!last || last === after || rows.size > 10_000) throw new LibraryProjectionError("library_capacity_exceeded");
          after = last;
        }
      };
      const known = await state();
      const expected = new Set(snapshot.rows.map(row => row.hit.id));
      if ([...known.keys()].some(id => !expected.has(id))) throw new LibraryProjectionError("library_foreign_source_row");
      const matches = (source: typeof snapshot.rows[number], saved: SavedRow | undefined) => saved !== undefined && saved.criteria_revision === snapshot.criteria_revision && saved.source_revision === snapshot.source_revision && saved.candidate_id === source.candidate_id && saved.native_author_id === source.native_author_id && saved.verdict === source.verdict && saved.evidence_version === source.evidence_version && isDeepStrictEqual(saved.reasons, source.reasons) && isDeepStrictEqual(saved.contacts, source.contacts ?? []);
      for (let offset = 0; offset < snapshot.rows.length; offset += 100) {
        assertCurrent();
        const batch = snapshot.rows.slice(offset, offset + 100);
        const enrichment = batch.some(row => row.pinterest_scale_passed_by !== undefined || row.bio_status !== undefined);
        if (!repairValues && !enrichment && batch.every(row => matches(row, known.get(row.hit.id)))) continue;
        const rows: IngestRow[] = batch.map(row => {
          const hit = row.hit;
          const values: IngestRow["values"] = {
            "Campaign ID": campaign, "Search hit ID": hit.id, "Platform": row.native_platform ?? hit.platform,
            "Author label": hit.author_label, "Followers": row.followers ?? null, "Author ID": hit.author_id,
            "Author URL": hit.author_url, "Content title": hit.title, "Source URL": hit.source_url,
            "Search keyword": hit.search_keyword, "Status": row.verdict === "qualified" ? "Recommended candidate" : row.verdict === "rejected" ? "Does not meet requirements" : row.verdict === "pending" ? "Awaiting required evidence" : "Awaiting account read", "Captured at (Unix s)": String(hit.captured_at),
            "Match evaluation": row.verdict === "qualified" || row.verdict === "pending" && row.reasons.includes("related_alternative") && row.match_rating?.level === "medium" || row.verdict === "rejected" && row.match_rating?.level === "low" ? row.match_rating ?? null : null,
            "Content category": typeof plan.criteria.category === "string" ? plan.criteria.category : "",
            "Pinterest monthly views": row.pinterest_monthly_views_raw ?? null,
            "Pinterest scale passed by": row.pinterest_scale_passed_by?.join(",") ?? null,
            "Biography status": row.bio_status ?? null,
          };
          return { search_hit_id: hit.id, candidate_id: row.candidate_id, native_author_id: row.native_author_id, verdict: row.verdict as IngestRow["verdict"], reasons: row.reasons, evidence_version: row.evidence_version, contacts: (row.contacts ?? []).map(contact => ({ ...contact, captured_at: timestamp(contact.captured_at) })), values: Object.fromEntries(Object.entries(values).map(([key, value]) => [fields.get(key)!, value])) };
        });
        await libraryCall(() => tables.data.ingestDiscoverySources(selected, { table_id: table.id, campaign_id: campaign, criteria_revision: snapshot.criteria_revision, source_revision: snapshot.source_revision, rows }), true); assertCurrent();
      }
      const finalRows = await state();
      if (finalRows.size !== snapshot.rows.length || snapshot.rows.some(row => !matches(row, finalRows.get(row.hit.id))) || [...known].some(([id, row]) => finalRows.get(id)?.row_id !== row.row_id)) throw new LibraryProjectionError("library_readback_incomplete");
      const currentMaterial = () => {
        assertCurrent();
        const current = resolved.repository.readLibraryMaterial(campaign, revision);
        if (current.sources.snapshot_hash !== snapshot.snapshot_hash || current.identity.snapshot_hash !== publication.material.identity.snapshot_hash) throw new LibraryProjectionError("library_source_stale");
      };
      currentMaterial();
      const identity = publication.material.identity;
      const projection = { table_id: table.id, campaign_id: campaign, projection_format: 1 as const, criteria_revision: snapshot.criteria_revision, source_revision: snapshot.source_revision, source_hash: snapshot.snapshot_hash, identity_revision: publication.identity_revision, identity_hash: identity.snapshot_hash, source_rows: snapshot.rows.map(row => ({ search_hit_id: row.hit.id, candidate_id: row.candidate_id, native_author_id: row.native_author_id, evidence_version: row.evidence_version })), groups: identity.groups.map(group => ({ group_id: group.group_id, has_same_author_review: group.has_same_author_review, decision_ids: group.decision_ids, members: group.members.map(member => ({ candidate_id: member.candidate_id, platform: member.platform, native_author_id: member.native_author_id, display_name: member.display_name, profile_url: member.profile_url, source_hit_ids: member.sources.map(row => row.hit.id) })) })) };
      const expectedIdentity = { state: "current", projection };
      const receipt = await libraryCall(() => tables.data.publishDiscoveryIdentity(selected, projection), true); assertCurrent();
      if (!isDeepStrictEqual(receipt, expectedIdentity)) throw new LibraryProjectionError("library_readback_incomplete");
      const readback = await libraryCall(() => tables.data.getDiscoveryIdentity(selected, { table_id: table.id, campaign_id: campaign })); assertCurrent();
      if (!isDeepStrictEqual(readback, expectedIdentity)) throw new LibraryProjectionError("library_readback_incomplete");
      currentMaterial();
      return { campaign_id: campaign, table_id: table.id, web_url: `${origin}/library/${encodeURIComponent(table.id)}`, total_hits: snapshot.rows.length, inserted_rows: finalRows.size - known.size };
    },
  };
}
