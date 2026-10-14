# OneCrew standard GA4 and business-success analytics

Measurement ID: `G-E8PCYC26GN`. Marketing and Access use the same standard Google
tag, explicit root cookie domain/path, and one initializer per document.

The browser is the only GA4 sender. The backend records successful business
operations in `analytics_success_event`; it never sends events to Google.

- `sign_up` is recorded by Better Auth's real `user.create.after` hook. This
  covers password, email OTP and Google account creation. Synthetic duplicate-email
  registration responses cannot create a receipt. The receipt is dispatched only
  after the owner has a verified email and authenticated session; anonymous register
  responses intentionally do not reveal whether an account was newly created.
- `generate_lead` is recorded only after the application operation is persisted.
  Its deduplication key is the submission's idempotency key. Retries cannot create
  another event; a valid reapplication uses a new operation and may count once.

`POST /api/analytics/claim` requires the current verified session, exact Origin,
and an empty JSON body. An Access host-only Secure/SameSite=Lax cookie carries a
SHA-256 fingerprint of the measurement ID plus the official gtag client/session
IDs. It is attribution metadata, not an authentication credential. The backend
captures this context when the business operation succeeds. One atomic D1 update
claims at most 20 pending events for that same user and attribution context within
30 minutes. Other browsers, changed sessions, expired receipts and receipts that
had no context at creation cannot be reassigned to the current visit. A retry does
not change the original context. The raw GA identifiers are not stored in D1.
If receipt persistence fails, a replay deliberately does not recreate it with a
new time or context; only the request that commits the business operation records it.
Only event names leave this endpoint. User IDs, operation IDs, form values and
authentication data are not GA4 parameters. Receipts remain as deduplication
records and are deleted with their owning user.

The initializer has separate marketing/Access contexts. It only retains approved
campaign/click parameters, reduces external referrers to origins, and maps private
download paths to a fixed path. Access uses a fixed title. Form values, receipt IDs,
user IDs and tokens are never event parameters. This does not replace the required
review of GA4 automatic collection settings.

`AnalyticsNavigation` explicitly sends one page_view per navigation key. The
marketing click listener sends cta_click only for a real matching CTA activation.
Neither page views nor CTA clicks prove business success. Access callbacks drain
server-confirmed receipts through the standard tag; there is no iframe or message bridge.

The official gtag get callbacks confirm readiness and refresh the attribution
context before claiming. Late readiness resumes a timed-out flush once. Blocked tag
loading leaves receipts pending. Claims are **at-most-once dispatch**, not guaranteed Google delivery:
closing the page or losing the network after a claim may lose a metric, but does
not replay it or affect registration/application success. All telemetry failures
are contained. There is no promise of a transaction spanning D1 and Google.

Deployment is gated on the OneCrew administrator's automatic-collection settings
review. `send_page_view:false` does not disable enhanced measurement. Coordinate
the manual-pageview switch across Site and Access, then verify client/session and
channel continuity.

Migration 0012 is already applied and must remain immutable; 0013 adds the original
attribution context and must be applied before enabling the new Access code.
Missing original attribution deliberately loses a metric instead of attributing it
to a later browser/session. This is not a claim of complete cross-session or offline
conversion coverage. Tests use isolated
data and fake GA4 delivery; do not create production accounts or emit success
events merely to test a button or page load.
