#![deny(unreachable_pub)]

//! Authenticated X Web transport and session mechanics.
//!
//! This crate owns HTTP construction, Chrome-compatible emulation, residential
//! proxy routing, request metadata, retries, and provider-protocol failures.
//! Typed operations and application policy belong to higher layers.

mod error;
mod session;
mod transaction;
mod transport;
mod wreq_transport;

pub use error::TransportError;
pub use session::XSession;
pub use transport::JsonTransport;
pub use wreq_transport::{WreqXTransport, emulated_client};
// Re-export so client crates name the HTTP method type without depending on the
// concrete HTTP client (now wreq).
pub use wreq::Method;
