import type { DatabaseSync } from "node:sqlite";
import { z } from "zod";

export type DiscoveryReadHealthKind = "profile" | "content" | "web_source" | "facebook_identity";
export interface DiscoveryOperationReadStop { platform: string; profile_ref: string; operation: DiscoveryReadHealthKind; code: string }
export function discoveryOperationStopped(rows: readonly { author: string; code: string | null }[]): boolean {
  const authors = new Set<string>();
  for (const { author, code } of rows) {
    if (code === null) break;
    if (code === "opencli_risk_control" || code === "opencli_auth_required") return true;
    if (!["opencli_timeout", "opencli_command_failed", "opencli_invalid_output", "opencli_output_too_large", "provider_response_shape_invalid", "provider_response_invalid_json"].includes(code) && !code.endsWith("_timeout")) break;
    if (author) authors.add(author); if (authors.size >= 3) return true;
  }
  return false;
}
export function loadDiscoveryReadStops(db: DatabaseSync, campaign: string, kind: DiscoveryReadHealthKind): DiscoveryOperationReadStop[] {
  const sources = {
    profile: { table: "discovery_profile_reads", platform: "p.platform", author: "p.author_id" },
    content: { table: "discovery_content_reads", platform: "p.platform", author: "p.author_id" },
    web_source: { table: "discovery_web_source_reads", platform: "p.origin_platform", author: "COALESCE(p.native_author_id,p.source_url)" },
    facebook_identity: { table: "discovery_facebook_identity_reads", platform: "p.platform", author: "p.source_url" },
  } as const;
  const source = sources[kind];
  const rows = db.prepare(`SELECT ${source.platform} AS platform,p.profile_ref,${source.author} AS author_id,p.stop_code FROM ${source.table} p JOIN discovery_campaigns c ON c.id=p.discovery_campaign_id WHERE c.id=? AND p.expected_revision=c.current_revision AND p.status!='reading' ORDER BY p.call_number DESC`).all(campaign);
  const scopes = new Map<string, { platform: string; profile_ref: string; rows: { author: string; code: string | null }[] }>();
  for (const raw of rows) {
    const row = z.object({ platform: z.string(), profile_ref: z.string(), author_id: z.string(), stop_code: z.string().nullable() }).parse(raw), key = JSON.stringify([row.platform, row.profile_ref]);
    let scope = scopes.get(key);
    if (!scope) { scope = { platform: row.platform, profile_ref: row.profile_ref, rows: [] }; scopes.set(key, scope); }
    scope.rows.push({ author: row.author_id, code: row.stop_code });
  }
  return [...scopes.values()].filter(scope => discoveryOperationStopped(scope.rows)).map(scope => ({ platform: scope.platform, profile_ref: scope.profile_ref, operation: kind, code: scope.rows[0]!.code! })).sort((left, right) => Buffer.compare(Buffer.from(left.platform), Buffer.from(right.platform)) || Buffer.compare(Buffer.from(left.profile_ref), Buffer.from(right.profile_ref)));
}
export function discoveryReadExcludedHits(db: DatabaseSync, campaign: string, kind: DiscoveryReadHealthKind): string[] {
  return loadDiscoveryReadStops(db, campaign, kind).flatMap(stop => db.prepare("SELECT id FROM discovery_search_hits WHERE discovery_campaign_id=? AND platform=? AND profile_ref=?").all(campaign, stop.platform, stop.profile_ref).map(row => z.string().parse(row.id)));
}
export function assertDiscoveryReadHealthy(db: DatabaseSync, campaign: string, kind: DiscoveryReadHealthKind, hit: string): void {
  if (discoveryReadExcludedHits(db, campaign, kind).includes(hit)) throw new Error("discovery_read_operation_stopped");
}
