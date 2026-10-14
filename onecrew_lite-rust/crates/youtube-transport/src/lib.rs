//! YouTube HTTP transport and provider-error decoding.
//!
//! This crate owns authenticated request mechanics and safe normalization of
//! provider failures. Typed YouTube operations and application workflows live
//! in the client and facade crates above it.

mod error;
mod transport;

pub use error::{YoutubeApiFailureKind, YoutubeRequestFailureKind, YoutubeTransportError};
pub use transport::{MAX_IN_FLIGHT_REQUESTS, YoutubeRequestLimiter, YoutubeTransport};
