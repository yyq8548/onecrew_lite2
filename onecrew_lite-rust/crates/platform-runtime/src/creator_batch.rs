//! Sealed owner review for manual Creator batch creation.
//!
//! Creating Creators from a submitted batch is a local write, so the confirmed
//! intent is frozen here at preparation and consumed exactly once as a
//! store-minted grant. No field an Agent can set decides whether the batch is
//! written: preparation seals the exact items, the owner reviews who is about
//! to be created, and execution consumes that authority. This is the same shape
//! as email and outreach, for the same reason — an input `confirmed` flag is
//! only ever validated at registration and never read at execution, which let a
//! resumed call write the Agent's original unconfirmed arguments.
mod execute;
mod models;
mod prepare;
mod projection;

pub(crate) use models::PreparedCreatorBatchAction;
pub(crate) use projection::into_owner_completion;
