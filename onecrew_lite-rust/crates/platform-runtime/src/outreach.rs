//! Trusted external writes composed with canonical Creator recording.
//!
//! Preparation reads providers but never claims or writes. Submitted authority
//! freezes the precise intent, while the Creator ledger controls every replay.
mod copy;
mod effects;
mod execute;
mod models;
mod prepare;
mod projection;
mod reads;
mod recovery;
mod targets;

pub(crate) use copy::{localize_review, review_confirmation_text};
pub(crate) use models::PreparedOutreachAction;
pub use models::{OutreachCompletion, OutreachError, OutreachPhase, OutreachReceiptReference};
