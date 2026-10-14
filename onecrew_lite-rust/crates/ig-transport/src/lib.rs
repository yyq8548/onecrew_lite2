//! Instagram private-API transport and authenticated session state.
//!
//! This crate owns device identity, request construction, session material,
//! and provider-protocol failures. Typed operations and application workflows
//! belong to the client and facade crates above it.

mod device;
mod error;
mod session;
mod transport;

pub use device::{DeviceSettings, Uuids};
pub use error::IgError;
pub use session::{AuthorizationData, IgGeo, IgSession};
pub use transport::IgTransport;
