use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
    mpsc,
};

use async_trait::async_trait;
use onecrew_dm_contracts::{DmMessage, DmReadError, DmTarget, DmThread};
use rusqlite::Connection;

mod durability;
mod saved_route_flow;
mod sync_tests;

use crate::*;

#[derive(Clone, Default)]
struct TestGateway {
    /// When the thread readback answers, the instant its one outgoing message
    /// was sent. Unix seconds, in the past unless a test moves it.
    thread_message_sent_at: Arc<Option<u64>>,
    message_readback_mismatch: bool,
    conversation_absent: Arc<AtomicBool>,
    /// The thread read fails instead of answering, so absence is unproven.
    conversation_unavailable: Arc<AtomicBool>,
    /// The thread id each readback asked for, in order.
    conversation_requests: Arc<Mutex<Vec<Option<String>>>>,
    conversation_barrier: Arc<Mutex<Option<ConversationBarrier>>>,
}

#[derive(Clone)]
struct ConversationBarrier {
    entered: Arc<tokio::sync::Barrier>,
    release: Arc<tokio::sync::Barrier>,
}

#[async_trait]
impl DirectMessageAccess for TestGateway {
    async fn thread(
        &self,
        _: &crate::AccountRef,
        _: &DmTarget,
        external_thread_id: Option<&str>,
        _: u16,
    ) -> Result<Option<DmThread>, DmReadError> {
        self.conversation_requests
            .lock()
            .unwrap()
            .push(external_thread_id.map(str::to_owned));
        let barrier = self.conversation_barrier.lock().unwrap().clone();
        if let Some(barrier) = barrier {
            barrier.entered.wait().await;
            barrier.release.wait().await;
        }
        if self.conversation_unavailable.load(Ordering::SeqCst) {
            return Err(DmReadError {
                diagnostic: onecrew_dm_contracts::DmDiagnostic::provider(
                    "test",
                    "test_thread_reader",
                    "conversation read failed",
                ),
            });
        }
        if self.conversation_absent.load(Ordering::SeqCst) {
            return Ok(None);
        }
        Ok(Some(DmThread {
            external_thread_id: "thread-1".into(),
            peer_external_id: "target-1".into(),
            messages: if self.message_readback_mismatch {
                Vec::new()
            } else {
                vec![DmMessage {
                    external_message_id: "message-1".into(),
                    body: "hello".into(),
                    sent_at: self.thread_message_sent_at.unwrap_or(1),
                    outgoing: true,
                    // Shaped like a platform model the runtime knows nothing
                    // about, so a test can prove it is stored unread.
                    provider: serde_json::json!({
                        "id": "message-1",
                        "sender": { "id": "operator", "verified": true },
                        "reactions": ["🔥"],
                    }),
                }]
            },
        }))
    }
}

fn runtime(gateway: TestGateway) -> (tempfile::TempDir, crate::CreatorRuntime, TestGateway) {
    let directory = tempfile::tempdir().unwrap();
    let gateway = Arc::new(gateway);
    let runtime = crate::CreatorRuntime::open(
        directory.path(),
        CreatorGateways::default().with_direct_messages(gateway.clone()),
    )
    .unwrap();
    (directory, runtime, gateway.as_ref().clone())
}

async fn hold_repository_read(
    runtime: &crate::CreatorRuntime,
) -> (
    tokio::task::JoinHandle<Result<(), crate::CreatorRepositoryDispatchFailure>>,
    mpsc::Sender<()>,
) {
    let dispatch = runtime.repository_dispatch.clone();
    let (entered_tx, entered_rx) = tokio::sync::oneshot::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let handle = tokio::spawn(async move {
        dispatch
            .read(move || {
                entered_tx.send(()).unwrap();
                release_rx.recv().unwrap();
            })
            .await
    });
    entered_rx.await.unwrap();
    (handle, release_tx)
}

async fn hold_repository_write(
    runtime: &crate::CreatorRuntime,
) -> (
    tokio::task::JoinHandle<Result<(), crate::CreatorRepositoryDispatchFailure>>,
    mpsc::Sender<()>,
) {
    let dispatch = runtime.repository_dispatch.clone();
    let (entered_tx, entered_rx) = tokio::sync::oneshot::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let handle = tokio::spawn(async move {
        dispatch
            .write(move || {
                entered_tx.send(()).unwrap();
                release_rx.recv().unwrap();
            })
            .await
    });
    entered_rx.await.unwrap();
    (handle, release_tx)
}

async fn assert_runtime_health() {
    tokio::time::timeout(
        std::time::Duration::from_millis(50),
        tokio::spawn(async { tokio::task::yield_now().await }),
    )
    .await
    .expect("repository segment starved the Tokio workers")
    .unwrap();
}

fn outreach_ids(
    runtime: &crate::CreatorRuntime,
) -> (
    crate::CreatorId,
    crate::CreatorContactRouteId,
    crate::ResolvedTargetId,
    crate::AccountRefId,
) {
    outreach_ids_for(runtime, Platform::X)
}

fn outreach_ids_for(
    runtime: &crate::CreatorRuntime,
    platform: Platform,
) -> (
    crate::CreatorId,
    crate::CreatorContactRouteId,
    crate::ResolvedTargetId,
    crate::AccountRefId,
) {
    let creator = runtime
        .repository
        .create_creator(CreateCreatorInput {
            tags: None,
            display_name: "Creator".into(),
            notes: None,
        })
        .unwrap();
    let operator = runtime
        .sync_account_ref(
            crate::AccountRefId::new("operator"),
            platform,
            "profile",
            "@operator",
        )
        .unwrap();
    let account = runtime
        .repository
        .upsert_platform_account(UpsertPlatformAccountInput {
            platform,
            target_kind: TargetKind::User,
            external_id: "target-1".into(),
        })
        .unwrap();
    runtime
        .repository
        .link_creator_account(
            &creator.creator.id,
            &account.id,
            LinkState::Confirmed,
            Confidence::Exact,
        )
        .unwrap();
    let target = runtime
        .repository
        .upsert_resolved_target(
            &operator.id,
            platform,
            TargetKind::User,
            "target-1",
            Some(&account.id),
            serde_json::json!({ "handle": "creator" }),
        )
        .unwrap();
    let channel = match platform {
        Platform::X => ContactChannel::X,
        Platform::Instagram => ContactChannel::Instagram,
        other => panic!("unsupported outreach test platform: {other:?}"),
    };
    let route = runtime
        .add_manual_creator_contact_route(ManualCreatorContactRouteAddInput {
            creator_id: creator.creator.id.clone(),
            channel,
            value: "creator".into(),
            source_url: None,
        })
        .unwrap();
    let route_record = runtime
        .repository
        .creator_contact_routes(&creator.creator.id)
        .unwrap()
        .into_iter()
        .find(|record| record.route.id == route.route_id)
        .unwrap();
    runtime
        .repository
        .record_locator_resolution(&route_record.locator.id, &operator.id, &target.id)
        .unwrap();
    (creator.creator.id, route.route_id, target.id, operator.id)
}

fn action_state(directory: &tempfile::TempDir, key: &str) -> Option<String> {
    Connection::open(directory.path().join("crm.db"))
        .unwrap()
        .query_row(
            "SELECT state FROM idempotency_records WHERE key = ?1",
            [key],
            |row| row.get(0),
        )
        .ok()
}

/// The stored `public_json` of the one message in the ledger.
fn message_public_json(directory: &tempfile::TempDir) -> serde_json::Value {
    let stored: String = Connection::open(directory.path().join("crm.db"))
        .unwrap()
        .query_row("SELECT public_json FROM messages", [], |row| row.get(0))
        .unwrap();
    serde_json::from_str(&stored).unwrap()
}

fn fact_count(directory: &tempfile::TempDir, table: &str) -> u64 {
    let query = format!("SELECT COUNT(*) FROM {table}");
    Connection::open(directory.path().join("crm.db"))
        .unwrap()
        .query_row(&query, [], |row| row.get(0))
        .unwrap()
}

type OutreachIds = (
    CreatorId,
    CreatorContactRouteId,
    ResolvedTargetId,
    AccountRefId,
);

async fn sync_fixture(
    runtime: &CreatorRuntime,
    ids: &OutreachIds,
) -> Result<ConversationView, String> {
    runtime
        .sync_conversation(
            ConversationSyncInput {
                creator_id: ids.0.clone(),
                contact_route_id: ids.1.clone(),
                resolved_target_id: ids.2.clone(),
                operator_account_ref_id: ids.3.clone(),
                external_thread_id: None,
                limit: 100,
            },
            None,
        )
        .await
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "fixture thread absent".into())
}

async fn route_context(runtime: &CreatorRuntime, ids: &OutreachIds) -> CreatorRouteOutreachContext {
    runtime
        .creator_direct_message_context(PrepareDirectMessageInput {
            creator_id: ids.0.clone(),
            contact_route_id: ids.1.clone(),
            resolved_target_id: ids.2.clone(),
            operator_account_ref_id: ids.3.clone(),
        })
        .await
        .expect("route context")
}

async fn content_intent(
    runtime: &CreatorRuntime,
    ids: &OutreachIds,
    body: &str,
) -> PreparedCreatorOutreachIntent {
    let route = route_context(runtime, ids).await;
    let context = runtime
        .creator_content_reply_context(PrepareContentReplyInput {
            creator_id: ids.0.clone(),
            target_platform_account_id: route.target.platform_account_id,
            operator_account_ref_id: ids.3.clone(),
            content_id: "post-42".into(),
            reply_to_id: Some("comment-1".into()),
        })
        .await
        .expect("content context");
    PreparedCreatorOutreachIntent::content_reply(context, body.into())
        .expect("canonical content intent")
}
