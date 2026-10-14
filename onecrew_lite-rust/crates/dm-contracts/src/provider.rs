use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::{
    DmCapability, DmReadError, DmReadbackCoverage, DmReceipt, DmSendError, DmTarget, DmThread,
    DmThreadReadback,
};

/// One direct message to send.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DmSendRequest {
    pub target: DmTarget,
    pub body: String,
}

/// One operator account's direct-message capability on one platform.
///
/// A provider owns the target kinds it accepts, its thread-id rule, its
/// stable-id check immediately before a write, and its capability mapping.
///
/// There is deliberately no inbox listing here: the conversation index is the
/// local ledger, so no read on this port can become a provider fan-out.
#[async_trait]
pub trait DirectMessageProvider: Send + Sync {
    /// What this account may do with this target right now.
    async fn capability(&self, target: &DmTarget) -> Result<DmCapability, DmReadError>;

    /// Send one message. A returned receipt means the provider accepted it.
    async fn send(&self, request: DmSendRequest) -> Result<DmReceipt, DmSendError>;

    /// Read one conversation with this target, newest `limit` messages.
    /// `Ok(None)` means no conversation was found within this bounded read.
    /// It does not prove a conversation or an earlier message is absent.
    async fn thread(
        &self,
        target: &DmTarget,
        external_thread_id: Option<&str>,
        limit: u16,
    ) -> Result<Option<DmThread>, DmReadError>;

    /// The same provider read used by sync, carrying its explicit coverage for
    /// recovery. Existing native thread endpoints return bounded pages, so no
    /// absence claim is made, even when the page contains fewer than `limit`.
    async fn readback(
        &self,
        target: &DmTarget,
        external_thread_id: Option<&str>,
        limit: u16,
    ) -> Result<DmThreadReadback, DmReadError> {
        Ok(DmThreadReadback {
            thread: self.thread(target, external_thread_id, limit).await?,
            coverage: DmReadbackCoverage::Incomplete,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        future::Future,
        task::{Context, Poll, Waker},
    };

    struct BoundedPage(usize);

    #[async_trait]
    impl DirectMessageProvider for BoundedPage {
        async fn capability(&self, _: &DmTarget) -> Result<DmCapability, DmReadError> {
            Ok(DmCapability::unknown(false))
        }
        async fn send(&self, _: DmSendRequest) -> Result<DmReceipt, DmSendError> {
            panic!("readback must never send")
        }
        async fn thread(
            &self,
            target: &DmTarget,
            _: Option<&str>,
            _: u16,
        ) -> Result<Option<DmThread>, DmReadError> {
            if self.0 == 0 {
                return Ok(None);
            }
            Ok(Some(DmThread {
                external_thread_id: "thread-1".into(),
                peer_external_id: target.external_id.clone(),
                messages: (0..self.0)
                    .map(|index| crate::DmMessage {
                        external_message_id: index.to_string(),
                        body: "different body".into(),
                        sent_at: 1_700_000_000,
                        outgoing: true,
                        provider: serde_json::json!({}),
                    })
                    .collect(),
            }))
        }
    }

    #[test]
    fn zero_short_and_full_pages_never_establish_absence() {
        let target = DmTarget {
            handle: "user".into(),
            external_id: "100".into(),
            kind: crate::DmTargetKind::User,
        };
        for count in [0, 5, 100] {
            let provider = BoundedPage(count);
            let mut future = std::pin::pin!(provider.readback(&target, None, 100));
            let Poll::Ready(Ok(result)) = future
                .as_mut()
                .poll(&mut Context::from_waker(Waker::noop()))
            else {
                panic!("offline fixture completes synchronously");
            };
            assert_eq!(result.coverage, DmReadbackCoverage::Incomplete);
            assert_eq!(
                result
                    .thread
                    .as_ref()
                    .map_or(0, |thread| thread.messages.len()),
                count
            );
        }
    }
}
