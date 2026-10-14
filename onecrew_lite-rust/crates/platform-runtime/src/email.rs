//! Sealed owner review for Creator email sends.
//!
//! Sending email is an external write, so the confirmed intent is frozen here
//! at preparation and consumed exactly once as a store-minted grant. No field
//! an Agent can set decides whether mail leaves: preparation seals the precise
//! send, the owner reviews it, and execution consumes that authority. This is
//! the same shape as outreach, for the same reason.
mod copy;
mod execute;
mod models;
mod prepare;
mod projection;

pub use models::EmailCallOutput;
pub(crate) use models::PreparedEmailAction;
