export const DISCOVERY_SCORE_CONCURRENCY = 2;
export interface DiscoveryScorePermit { release(): void }

export class DiscoveryScoreGate {
  readonly #authors = new Set<string>();
  acquire(author: string): DiscoveryScorePermit {
    if (this.#authors.size >= DISCOVERY_SCORE_CONCURRENCY || this.#authors.has(author)) throw new Error("initial_match_busy");
    this.#authors.add(author); let released = false;
    return Object.freeze({ release: () => { if (!released) { released = true; this.#authors.delete(author); } } });
  }
}
