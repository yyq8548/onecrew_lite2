let inFlight: Promise<void> | undefined;
let requested = false;
let awaitingTag = false;

function resumeWhenReady() {
  if (awaitingTag) return;
  awaitingTag = true;
  window.addEventListener("onecrew:analytics-ready", () => {
    awaitingTag = false;
    void flushBusinessAnalytics();
  }, { once: true });
}

/** Claim only after the standard tag is ready; Google delivery is best effort. */
export function flushBusinessAnalytics(): Promise<void> {
  requested = true;
  if (inFlight) return inFlight;
  inFlight = (async () => {
    requested = false;
    const analytics = window.onecrewAnalytics;
    if (analytics?.surface !== "access") return;
    if (!(await analytics.ready())) { resumeWhenReady(); return; }
    do {
      requested = false;
      const response = await fetch("/api/analytics/claim", {
        method: "POST", credentials: "same-origin", cache: "no-store",
        headers: { "Content-Type": "application/json" }, body: "{}", signal: AbortSignal.timeout(5000),
      });
      if (!response.ok) return;
      const data: unknown = await response.json();
      if (!data || typeof data !== "object" || !("events" in data) || !Array.isArray(data.events) || data.events.length > 20) return;
      for (const event of data.events) if (event === "sign_up" || event === "generate_lead") analytics.business(event);
    } while (requested);
  })().catch(() => undefined).finally(() => {
    inFlight = undefined;
    if (requested) void flushBusinessAnalytics();
  });
  return inFlight;
}
