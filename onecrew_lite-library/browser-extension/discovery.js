// Discover only the local OneCrew service; never send credentials during probing.
globalThis.OneCrewDiscovery = {
  async find(preferred) {
    const ports = Array.from({ length: 20 }, (_, i) => 8091 + i);
    const candidates = ports.map((port) => `http://127.0.0.1:${port}`);
    if (candidates.includes(preferred)) candidates.splice(candidates.indexOf(preferred), 1);
    if (ports.some((port) => preferred === `http://127.0.0.1:${port}`))
      candidates.unshift(preferred);
    for (const url of candidates) {
      try {
        const response = await fetch(`${url}/healthz`, {
          signal: AbortSignal.timeout(600),
          redirect: 'error',
        });
        if (!response.ok) continue;
        const result = await response.json();
        if (result.ok === true && result.service === 'onecrew' && result.protocol_version === 1)
          return url;
      } catch {
        /* Try the next bounded loopback port. */
      }
    }
    return null;
  },
};
