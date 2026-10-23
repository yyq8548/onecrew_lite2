import type { DatabaseSync } from "node:sqlite";

export const DISCOVERY_READ_CONCURRENCY = 2;

// 同平台保持串行；网页来源同时占用其来源平台和实际读取的平台。
function capacity(db: DatabaseSync, campaign: string): { full: boolean; platforms: string[] } {
  const active = db.prepare("SELECT platform FROM discovery_platform_runs WHERE discovery_campaign_id=? AND status='searching'").all(campaign);
  if (active.length >= DISCOVERY_READ_CONCURRENCY) return { full: true, platforms: [] };
  if (!active.length) return { full: false, platforms: [] };
  const derived = db.prepare(`SELECT web.target_platform AS platform FROM discovery_web_source_reads web JOIN discovery_platform_runs occupied
      ON occupied.discovery_campaign_id=web.discovery_campaign_id AND occupied.platform=web.origin_platform AND occupied.profile_ref=web.profile_ref
      WHERE web.discovery_campaign_id=?1 AND web.status='reading' AND occupied.status='searching'
    UNION SELECT candidate.platform FROM discovery_language_samples sample JOIN discovery_candidates candidate ON candidate.id=sample.candidate_id
      JOIN discovery_search_hits source ON source.id=sample.search_hit_id JOIN discovery_platform_runs occupied
        ON occupied.discovery_campaign_id=sample.discovery_campaign_id AND occupied.platform=source.platform AND occupied.profile_ref=source.profile_ref
      WHERE sample.discovery_campaign_id=?1 AND sample.status='reading' AND occupied.status='searching'`).all(campaign);
  return { full: false, platforms: [...new Set([...active, ...derived].map(row => String(row.platform)))] };
}

export function discoveryReadBusySql(db: DatabaseSync, campaign: string, platform: string): string {
  // 调用与后续prepare之间没有异步操作；领取方仍在同一写事务内检查。
  const occupied = capacity(db, campaign);
  return occupied.full ? "1" : occupied.platforms.length ? `(${platform} IN (${occupied.platforms.map(value => `'${value.replaceAll("'", "''")}'`).join(",")}))` : "0";
}

export function discoveryReadBusy(db: DatabaseSync, campaign: string, platforms: readonly string[]): boolean {
  const occupied = capacity(db, campaign);
  return occupied.full || platforms.some(platform => occupied.platforms.includes(platform));
}

export class DiscoveryReadGate {
  readonly #active = new Set<readonly string[]>();
  get occupied(): string[] { return [...new Set([...this.#active].flatMap(resources => [...resources]))]; }
  get full(): boolean { return this.#active.size >= DISCOVERY_READ_CONCURRENCY; }
  acquire(resources: readonly string[]): { release(): void } {
    if (this.full || resources.some(resource => this.occupied.includes(resource))) throw new Error("discovery_read_active");
    this.#active.add(resources);
    return { release: () => { this.#active.delete(resources); } };
  }
}
