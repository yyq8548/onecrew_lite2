use std::sync::Arc;

use super::{DirectMessageAccess, EmailGateway};

/// The remaining Creator-owned effects: Email and explicit conversation sync.
/// Outreach sends are composed by platform-runtime from typed Creator intent.
#[derive(Clone, Default)]
pub struct CreatorGateways {
    pub email: Option<Arc<dyn EmailGateway>>,
    pub direct_messages: Option<Arc<dyn DirectMessageAccess>>,
}

impl CreatorGateways {
    pub fn with_email(mut self, email: Arc<dyn EmailGateway>) -> Self {
        self.email = Some(email);
        self
    }

    pub fn with_direct_messages(mut self, direct_messages: Arc<dyn DirectMessageAccess>) -> Self {
        self.direct_messages = Some(direct_messages);
        self
    }
}
