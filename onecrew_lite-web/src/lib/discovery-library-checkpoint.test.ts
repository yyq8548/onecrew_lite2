import { describe, expect, it, vi } from "vitest";
import { createDiscoveryLibraryCheckpointSync } from "./discovery-library-checkpoint";

describe("discovery Library checkpoints", () => {
  it("publishes first results, bounded updates, and the final partial batch", async () => {
    const sync = vi.fn().mockResolvedValue(true);
    const checkpoint = createDiscoveryLibraryCheckpointSync("audio", sync);
    await checkpoint(1, false, false);
    await checkpoint(2, true, false);
    await checkpoint(11, true, false);
    await checkpoint(12, true, false);
    await checkpoint(15, true, true);
    await checkpoint(15, true, true);
    expect(sync.mock.calls).toEqual([["audio"], ["audio"], ["audio"]]);
  });

  it("retries a busy checkpoint but never replays a failed write automatically", async () => {
    const sync = vi.fn().mockResolvedValueOnce(false).mockResolvedValueOnce(true)
      .mockRejectedValueOnce(new Error("uncertain write"));
    const checkpoint = createDiscoveryLibraryCheckpointSync("audio", sync);
    await checkpoint(1, true, false);
    await checkpoint(2, true, false);
    await checkpoint(12, true, false);
    await checkpoint(22, true, false);
    await checkpoint(23, true, true);
    expect(sync).toHaveBeenCalledTimes(3);
  });

  it("awaits synchronization before allowing the next provider read", async () => {
    let finish!: (value: boolean) => void;
    const events: string[] = [];
    const checkpoint = createDiscoveryLibraryCheckpointSync("audio", () => {
      events.push("sync");
      return new Promise<boolean>(resolve => { finish = resolve; });
    });
    const nextRead = checkpoint(1, true, false).then(() => events.push("next read"));
    expect(events).toEqual(["sync"]);
    finish(true);
    await nextRead;
    expect(events).toEqual(["sync", "next read"]);
  });
});
