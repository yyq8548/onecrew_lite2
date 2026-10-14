# OneCrew public site Worker

This Worker serves only the pre-rendered marketing site and its static assets. It owns no D1,
secrets, authentication, account, pairing, or API bindings.

`wrangler.toml` is the deployment source for `SITE_ORIGIN`, `WWW_ORIGIN`, and
`ACCESS_ORIGIN`, plus the apex and www custom-domain routes. HTTP and www
requests are redirected permanently to the HTTPS apex origin.
The Web build wrapper reads those non-secret values and exposes them to Vite as
`VITE_SITE_URL` and `VITE_ACCESS_URL`. The source copies of `robots.txt`,
`sitemap.xml`, and `llms.txt` remain under `web/public`; the Site build renders
their placeholders into `site/build/client`.

`marketing-pages.json` is the single path manifest. React Router Framework Mode
builds each of its 24 Chinese `/zh` paths and matching 24 default English
unprefixed paths with `ssr: false` pre-rendering. The root directly serves English
with HTTP 200 regardless of browser language or user agent. Old `/en` URLs
permanently redirect once to their exact unprefixed English counterpart.
Each canonical path has its own HTML document,
metadata, alternate-language links, and hydratable client entry. The Site
Worker serves only the corresponding `<path>/index.html`; unknown and sensitive
paths remain real 404 responses.

Each page declares its own canonical, reciprocal English/Chinese alternates,
and the English URL as `x-default`. Sitemap entries contain only these 48 final
URLs. `development/check-site-seo.mjs` checks the built HTML and sitemap on every
Site build, alongside the per-page CSP checks. Google's AI Search features use
the same indexability requirements; no crawler-specific content or special AI
markup is introduced. Successful checks do not guarantee Google indexing.

From the repository root:

```sh
pnpm run dev:site
pnpm run test:site
pnpm run build:site
pnpm run build:site-worker
```

The last command is a local Wrangler dry run. A real deployment is a separate,
explicit operation.

## GA4

Marketing pages load `/ga4-init.js` once before the async Google tag for `G-E8PCYC26GN`. The Site Worker allowlists the initializer and the basic Analytics CSP sources; existing per-page hashes and inline restrictions remain in place. No Google Ads sources or manual `page_view` events are added.

SPA page views use GA4 enhanced measurement: keep Page loads and Page changes based on browser history events enabled in the data stream. Validate English/Chinese homepages and navigation to an inner page in Realtime/DebugView, including current title, URL and exactly one page view per navigation. Debug mode is for test sessions only. Do not place personal information or access tokens in marketing URLs; automatic measurement includes page URLs. No existing consent manager was found in the marketing app.
