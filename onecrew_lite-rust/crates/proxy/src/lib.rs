//! Residential (家宽) proxy support: the `ProxyConfig` model (always available)
//! plus an egress-IP probe (behind the `net` feature). Kept in one crate so the
//! proxy logic isn't scattered across the transport/runtime crates.

mod config;
pub use config::{ProxyConfig, ProxyKind};

#[cfg(feature = "net")]
mod probe;
#[cfg(feature = "net")]
pub use probe::{EgressInfo, ProxyError, check_egress_ip, iana_offset_seconds, probe};

#[cfg(feature = "net")]
mod forwarder;
#[cfg(feature = "net")]
pub use forwarder::{Forwarder, spawn as spawn_forwarder};
