//! Encrypted X Chat operations over the X transport and vendored crypto core.
//!
//! This crate owns chat-key handling, recovery integration, message parsing,
//! and the chat service API. Account persistence and Creator messaging policy stay
//! with their application owners.

mod api;
mod crypto;
mod error;
pub mod juicebox;
mod keys;
mod models;
mod parser;
mod service;

pub use api::XChatApi;
pub use crypto::NativeXChat;
pub use error::XChatError;
pub use juicebox::{WreqJuiceboxApi, WreqJuiceboxHttp};
pub use keys::{ChatKeyMaterial, XChatDmCapability, XChatDmPermissions};
pub use models::{
    XChatConversation, XChatDmCapabilityProbe, XChatMessage, XChatParticipant, XChatSendResult,
};
pub use service::XChatService;
