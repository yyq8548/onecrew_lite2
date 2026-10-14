//! Toolkit functions: `connection admission -> facade`.
//!
//! Each function admits its connection (the vault projection on the blocking
//! pool, then the Creator `account_refs` gate under Creator admission) and
//! opens the facade through the runtime's one `PlatformSessions` (YouTube
//! through `CredentialServices`). X, Instagram and Telegram reads delegate to
//! the shared orchestration in `read`, and YouTube reads to its credential
//! read, which checks a bound search the same way and also commits credential
//! health: every read reports [`PlatformReadError`], whose platform failures
//! carry the platform's own evidence, and peer-facing reads return
//! [`PlatformNativeData`]. Protocol adapters project these outcomes without
//! inspecting lower-level errors.
//! `link_hub` delegates to the independent public-web capability.

mod direct_messages;
pub use direct_messages::DmToolkitError;
pub mod instagram;
pub mod link_hub;
mod read;
pub use read::{PlatformNativeData, PlatformReadError};
pub mod telegram;
pub mod x;
pub mod youtube;
