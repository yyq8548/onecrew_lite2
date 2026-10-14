//! The soft fingerprint we align to the profile's 家宽 egress region.
//!
//! We use the user's real Chrome (its true UA / hardware fingerprint), so the
//! only things set per profile are the region-derived signals — timezone (via
//! the `TZ` env var) and Accept-Language / `navigator.language` (via
//! `--accept-lang`). WebRTC is forced to never leak the real IP behind the
//! proxy (a launch flag). Canvas / WebGL / GPU / fonts stay the machine's real
//! values — shared across profiles — which is fine for our architecture: a
//! per-account social browser is opened only for the one-time login, and X /
//! Instagram operation afterwards runs through the wreq API client (no
//! browser, no canvas). See the fingerprint-isolation analysis for why
//! hardware-fingerprint spoofing isn't needed here.

#[derive(Debug, Clone, Default)]
pub struct Fingerprint {
    /// IANA timezone (e.g. `America/New_York`) matched to the 家宽 egress.
    pub timezone: Option<String>,
    /// `Accept-Language` / `navigator.language` (e.g. `en-US,en;q=0.9`).
    pub accept_language: Option<String>,
}

impl Fingerprint {
    /// A fingerprint aligned to a 家宽 region. Pass `None` to leave a signal at
    /// the browser/system default.
    pub fn aligned(timezone: Option<String>, accept_language: Option<String>) -> Self {
        Self {
            timezone,
            accept_language,
        }
    }
}
