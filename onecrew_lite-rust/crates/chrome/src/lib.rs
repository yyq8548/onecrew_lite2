#![deny(unreachable_pub)]

//! Per-profile Google Chrome control for the login flow.
//!
//! We drive the user's *own installed Chrome* (its real fingerprint), not a
//! bundled/anti-detect build, so X / Instagram won't flag it. The whole path is
//! pure Rust with no CDP/automation protocol: launch Chrome with a fresh,
//! isolated `--user-data-dir` routed through the profile's 家宽 (a loopback
//! no-auth forwarder), let the human log in, then read this profile's `Cookies`
//! store directly and decrypt the values.
//!
//! Detection, regional fingerprint inputs, process lifecycle, cookie access,
//! and orchestration remain private implementation concerns behind the safe
//! public session API.

mod cookies;
mod detect;
mod fingerprint;
mod launch;
mod session;
mod setup_page;

pub use detect::{
    ChromeChannel, ChromeInstallScope, DetectedChrome, chrome_version, detect_chrome,
    detect_chrome_install,
};
pub use fingerprint::Fingerprint;
pub use session::Chrome;
pub use setup_page::{ChromeSetupPage, open_setup_page};

#[derive(Debug, thiserror::Error)]
pub enum ChromeError {
    #[error("Google Chrome is not installed")]
    BrowserNotFound,
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Cookies DB read failed: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("failed to read Chrome Safe Storage key from Keychain: {0}")]
    Keychain(String),
    #[error("Chrome cookie store was not found under app-owned profile {0}")]
    CookieStoreNotFound(std::path::PathBuf),
    #[error("Chrome cookie store is still locked after browser shutdown: {0}")]
    CookieStoreLocked(std::path::PathBuf),
    #[error("Chrome Local State was not found under app-owned profile {0}")]
    LocalStateNotFound(std::path::PathBuf),
    #[error("Chrome Local State has no usable DPAPI cookie key: {0}")]
    CookieKeyInvalid(String),
    #[error("Windows DPAPI could not decrypt the app-owned Chrome cookie key: {0}")]
    Dpapi(String),
    #[error(
        "Chrome App-Bound (v20) cookies are not supported; only this App's isolated Chrome profile may be imported"
    )]
    AppBoundCookieUnsupported,
    #[error("Chrome cookie ciphertext is invalid or cannot be decrypted")]
    CookieDecryptionFailed,
    #[error("Chrome cookie host integrity check failed for {0}")]
    CookieHostIntegrityFailed(String),
    #[error("failed to request a graceful Chrome shutdown: {0}")]
    GracefulClose(String),
    #[error("Chrome did not exit within the graceful shutdown timeout")]
    CloseTimeout,
    #[error("cookie decryption is not supported on this platform")]
    UnsupportedPlatform,
}
