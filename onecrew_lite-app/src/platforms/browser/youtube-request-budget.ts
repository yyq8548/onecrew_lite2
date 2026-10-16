import PQueue from "p-queue";
export type YoutubeRequest = <T>(operation: (signal: AbortSignal) => Promise<T>, signal: AbortSignal) => Promise<T>;
export class YoutubeRequestBudget {
  readonly #queue = new PQueue({ concurrency: 6 });
  readonly run: YoutubeRequest = async <T>(operation: (signal: AbortSignal) => Promise<T>, signal: AbortSignal): Promise<T> => {
    signal.throwIfAborted();
    const waiting = new AbortController(), timer = setTimeout(() => waiting.abort(new DOMException("YouTube request permit wait timed out", "TimeoutError")), 30_000);
    try {
      return await this.#queue.add(async ({ signal: taskSignal }) => { clearTimeout(timer); if (!taskSignal) throw new Error("youtube.request_signal_missing"); return operation(taskSignal); }, { signal: AbortSignal.any([signal, waiting.signal]) });
    } finally { clearTimeout(timer); }
  };
  async close(): Promise<void> { await this.#queue.onIdle(); }
}
