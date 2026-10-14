(() => {
  if (window.onecrewAnalytics) return;
  const surface = document.currentScript?.dataset.surface;
  if (surface !== "marketing" && surface !== "access") return;
  const measurement = "G-E8PCYC26GN";
  const siteOrigin = "__ONECREW_SITE_ORIGIN__";
  const accessOrigin = "__ONECREW_ACCESS_ORIGIN__";
  const accessPaths = new Set(["/", "/login", "/register", "/forgot-password", "/account", "/pairing/authorize", "/pairing/rate-limited"]);
  const campaignKeys = new Set(["utm_source", "utm_medium", "utm_campaign", "utm_term", "utm_content", "utm_id"]);
  const clickKeys = new Set(["gclid", "dclid", "gbraid", "wbraid"]);
  function referrer(value) {
    try { const url = new URL(value); return /^https?:$/.test(url.protocol) ? url.origin + "/" : ""; }
    catch { return ""; }
  }
  function pageContext() {
    const current = new URL(window.location.href);
    const path = surface === "access"
      ? accessPaths.has(current.pathname) ? current.pathname : current.pathname.startsWith("/d/") ? "/download" : "/"
      : current.pathname;
    const safe = new URL(path, current.origin);
    for (const [key, value] of current.searchParams) {
      if (campaignKeys.has(key) && /^[\p{L}\p{N}_. ~-]{1,100}$/u.test(value)) safe.searchParams.set(key, value);
      if (clickKeys.has(key) && /^[A-Za-z0-9_.~-]{1,256}$/.test(value)) safe.searchParams.set(key, value);
    }
    return { page_location: safe.href, page_referrer: referrer(document.referrer), page_title: surface === "access" ? "OneCrew Access" : document.title };
  }
  const layer = window.dataLayer = window.dataLayer || [];
  window.gtag = function () { layer.push(arguments); };
  window.gtag("js", new Date());
  // Enhanced measurement must be reviewed separately. This only disables the
  // default page_view; explicit page views and conversions have one owner below.
  window.gtag("config", measurement, {
    cookie_domain: new URL(siteOrigin).hostname, cookie_path: "/",
    send_page_view: false, allow_google_signals: false, allow_ad_personalization_signals: false,
    ...pageContext(),
  });
  let previousPage = referrer(document.referrer);
  let previousKey;
  let contextRequest = 0;
  function ready() {
    const request = ++contextRequest;
    return new Promise(resolve => {
      let client, session, received = 0;
      const timer = setTimeout(() => resolve(false), 5000);
      async function finish() {
        if (++received !== 2) return;
        clearTimeout(timer);
        if (request !== contextRequest || typeof client !== "string" || !client || !session) { resolve(false); return; }
        try {
          if (surface === "access") {
            const bytes = await crypto.subtle.digest("SHA-256", new TextEncoder().encode(JSON.stringify([measurement, client, String(session)])));
            if (request !== contextRequest) { resolve(false); return; }
            const hash = Array.from(new Uint8Array(bytes), byte => byte.toString(16).padStart(2, "0")).join("");
            document.cookie = `__Host-onecrew-ga-context=${hash}; Path=/; Secure; SameSite=Lax; Max-Age=1800`;
            if (!document.cookie.split(";").some(part => part.trim() === `__Host-onecrew-ga-context=${hash}`)) { resolve(false); return; }
          }
          resolve(true);
          window.dispatchEvent(new Event("onecrew:analytics-ready"));
        } catch { resolve(false); }
      }
      window.gtag("get", measurement, "client_id", value => { client = value; void finish(); });
      window.gtag("get", measurement, "session_id", value => { session = value; void finish(); });
    });
  }
  const send = (name, extra = {}) => window.gtag("event", name, { ...pageContext(), ...extra, send_to: measurement });
  window.onecrewAnalytics = {
    surface,
    pageView(key) {
      if (key === previousKey) return;
      previousKey = key;
      send("page_view", { page_referrer: previousPage });
      previousPage = pageContext().page_location;
      if (surface === "access") void ready();
    },
    business(name) {
      if (surface === "access" && (name === "sign_up" || name === "generate_lead")) send(name);
    },
    ready,
  };
  if (surface === "marketing") document.addEventListener("click", event => {
    try {
      const anchor = event.target?.closest?.("a[href]");
      if (!anchor) return;
      const target = new URL(anchor.href, window.location.href);
      if (target.origin === accessOrigin && target.pathname === "/account" && target.searchParams.get("apply") === "1") {
        send("cta_click", { cta_id: "try_onecrew" });
      }
    } catch { /* Analytics cannot interrupt navigation. */ }
  });
})();
