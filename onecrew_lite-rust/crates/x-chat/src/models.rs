use serde::{Deserialize, Serialize};

use crate::XChatDmCapability;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct XChatDmCapabilityProbe {
    pub target_user_id: String,
    pub capability: XChatDmCapability,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct XChatParticipant {
    pub id: String,
    pub name: String,
    pub username: String,
    pub profile_image_url: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct XChatMessage {
    pub id: String,
    pub conversation_id: String,
    pub sender_id: String,
    pub text: String,
    pub created_at_msec: Option<i64>,
    pub verified: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct XChatConversation {
    pub id: String,
    pub participants: Vec<XChatParticipant>,
    pub messages: Vec<XChatMessage>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct XChatSendResult {
    pub message_id: String,
    pub conversation_id: String,
    pub username: String,
}
