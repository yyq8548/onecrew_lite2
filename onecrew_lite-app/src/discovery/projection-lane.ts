import { DISCOVERY_SCORE_CONCURRENCY } from "./score-gate.ts";

export const DISCOVERY_READ_AHEAD = 3;
export function discoveryScoreCapacity(remaining: number): number {
  if (!Number.isSafeInteger(remaining) || remaining < 0) throw new Error("discovery_remaining_invalid");
  return Math.min(remaining, DISCOVERY_SCORE_CONCURRENCY + DISCOVERY_READ_AHEAD);
}

// 与原LatestWork相同：当前投影排空前，只保存最后一个尚未开始的投影。
export class DiscoveryProjectionLane {
  #active: Promise<void> | null = null;
  #pending: (() => Promise<void>) | null = null;
  #errors: unknown[] = [];
  #closed = false;
  #settled = false;
  get busy(): boolean { return this.#active !== null || this.#pending !== null; }
  request(work: () => Promise<void>): void {
    if (this.#closed) throw new Error("discovery_projection_closed");
    this.#pending = work; this.#pump();
  }
  #pump(): void {
    if (this.#active !== null || this.#pending === null) return;
    const work = this.#pending; this.#pending = null;
    // 错误保留到reap/drain统一报告，防止后台Promise产生未处理拒绝。
    this.#settled = false;
    this.#active = Promise.resolve().then(work).then(() => undefined, error => { this.#errors.push(error); }).then(() => { this.#settled = true; });
  }
  #report(): void {
    if (!this.#errors.length) return;
    const errors = this.#errors; this.#errors = [];
    throw new AggregateError(errors, "discovery_projection_failed");
  }
  async reap(): Promise<void> {
    if (this.#active !== null && this.#settled) {
      const active = this.#active; await active;
      if (this.#active === active) { this.#active = null; this.#pump(); }
    }
    this.#report();
  }
  async drain(): Promise<void> {
    while (this.#active !== null) {
      const active = this.#active; await active;
      if (this.#active === active) { this.#active = null; this.#pump(); }
    }
    this.#report();
  }
  async close(): Promise<void> { this.#closed = true; await this.drain(); }
}
