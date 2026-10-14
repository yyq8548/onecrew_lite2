use serde::{Deserialize, Serialize};

/// What the provider returned for one accepted send.
///
/// These field names are a durable recovery payload: the Creator owner stages
/// a receipt in its idempotency record and replays it after a restart, so the
/// encoding may not change without a data-evolution assessment.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DmReceipt {
    pub external_message_id: String,
    pub external_thread_id: Option<String>,
}

/// One message in a thread read.
///
/// The typed fields are only what ordering, dedupe, readback and last contact
/// need. Everything else stays in `provider`, exactly as the client returned
/// it — no reshaping, no unit conversion, no invented display values.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DmMessage {
    pub external_message_id: String,
    pub body: String,
    /// Unix seconds, in the unit the provider documents. A provider that
    /// cannot say when a message was sent fails the read instead of picking a
    /// time.
    pub sent_at: u64,
    pub outgoing: bool,
    /// The client model exactly as it was returned. Who sent the message, and
    /// everything else the platform said about it, lives here.
    pub provider: serde_json::Value,
}

/// One conversation as the provider currently sees it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DmThread {
    pub external_thread_id: String,
    /// Stable id of the peer the provider actually read, so the caller can
    /// prove the thread belongs to the target it asked for.
    pub peer_external_id: String,
    pub messages: Vec<DmMessage>,
}

/// Coverage is evidence from the provider, independent of a page's length.
/// A short or empty page alone never establishes complete history.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DmReadbackCoverage {
    Incomplete,
    /// The provider explicitly proved the entire thread history was read.
    Complete,
}

#[derive(Debug, Clone)]
pub struct DmThreadReadback {
    pub thread: Option<DmThread>,
    pub coverage: DmReadbackCoverage,
}
