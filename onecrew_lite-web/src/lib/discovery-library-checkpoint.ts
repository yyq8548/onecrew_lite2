/** Called between completed provider reads, never from a polling query.
 * A failed write stops automatic sync for this run; manual recovery remains.
 * Returning false means another sync is busy and no write was attempted. */
export function createDiscoveryLibraryCheckpointSync(
  campaignId: string,
  sync: (campaignId: string) => Promise<boolean>,
) {
  let lastSynced: number | null = null;
  let stopped = false;
  return async (usedCalls: number, hasHits: boolean, finished: boolean): Promise<void> => {
    if (stopped || !hasHits || !Number.isSafeInteger(usedCalls) || usedCalls < 0
      || lastSynced === usedCalls
      || (lastSynced !== null && !finished && usedCalls - lastSynced < 10)) return;
    try {
      if (await sync(campaignId)) lastSynced = usedCalls;
    } catch {
      stopped = true;
    }
  };
}
