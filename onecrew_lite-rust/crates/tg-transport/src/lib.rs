//! Telegram MTProto transport built on grammers.
//!
//! Owns connection lifecycle, phone-code login, and session persistence to a
//! vault JSON string. The read/write operation surface lives in `tg-client`.

mod connection;
mod error;
mod session;

pub use connection::{
    Connection, LoginCodeDelivery, LoginCodeMethod, LoginCodeRequest, LoginCodeToken,
    PasswordCheck, QrLoginChallenge, QrLoginExport, SignInStep,
};
pub use error::TgError;
pub use session::VaultSession;

// Re-exported so the runtime can hold pending-login state and higher layers can
// build operations against the same grammers version.
pub use grammers_client::client::PasswordToken;
pub use grammers_client::{self, Client};
pub use grammers_tl_types as tl;
