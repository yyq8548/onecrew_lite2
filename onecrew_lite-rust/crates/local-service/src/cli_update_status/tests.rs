use super::*;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

fn identity(id: &str) -> UpdateIdentity {
    UpdateIdentity {
        installation_id: id.into(),
        credential_id: format!("credential-{id}"),
    }
}

#[tokio::test(start_paused = true)]
async fn version_state_cache_identity_and_timeout_are_independent_of_business() {
    let owner = CliUpdateStatusOwner::default();
    let available = owner
        .read(Some(identity("one")), "0.5.22", || async {
            Ok("0.5.23".into())
        })
        .await;
    assert_eq!(available.state, CliUpdateState::Available);
    let cached = owner
        .read(Some(identity("one")), "0.5.22", || async {
            panic!("cached status must not fetch")
        })
        .await;
    assert_eq!(cached.source, Some(StatusSource::MemoryCache));
    assert_eq!(cached.checked_at, available.checked_at);
    let ahead = owner
        .read(Some(identity("two")), "0.5.24", || async {
            Ok("0.5.23".into())
        })
        .await;
    assert_eq!(ahead.state, CliUpdateState::Current);
    tokio::time::advance(CACHE_TTL).await;
    let timed_out = owner
        .read(Some(identity("two")), "0.5.24", std::future::pending)
        .await;
    assert_eq!(timed_out.diagnostic_code, Some(CheckFailure::TimedOut));
    let skipped = owner
        .read(None, "0.5.24", || async {
            panic!("inactive status must not fetch")
        })
        .await;
    assert_eq!(skipped.state, CliUpdateState::NotChecked);
}

#[tokio::test(start_paused = true)]
async fn concurrent_status_calls_share_one_metadata_request() {
    let owner = CliUpdateStatusOwner::default();
    let count = Arc::new(AtomicUsize::new(0));
    let fetch = || {
        let count = count.clone();
        async move {
            count.fetch_add(1, Ordering::SeqCst);
            tokio::time::sleep(Duration::from_millis(10)).await;
            Ok("0.5.22".to_owned())
        }
    };
    let (first, second) = tokio::join!(
        owner.read(Some(identity("one")), "0.5.22", fetch),
        owner.read(Some(identity("one")), "0.5.22", fetch),
    );
    assert_eq!(count.load(Ordering::SeqCst), 1);
    assert_eq!(first.state, CliUpdateState::Current);
    assert_eq!(second.source, Some(StatusSource::MemoryCache));
}

#[tokio::test(start_paused = true)]
async fn expiry_refetches_but_failures_are_cached_and_inactive_calls_skip_immediately() {
    let owner = CliUpdateStatusOwner::default();
    let first = owner
        .read(Some(identity("one")), "0.5.22", || async {
            Err(CheckFailure::CheckFailed)
        })
        .await;
    assert_eq!(first.state, CliUpdateState::Unavailable);
    let cached = owner
        .read(Some(identity("one")), "0.5.22", || async {
            panic!("negative cache must coalesce")
        })
        .await;
    assert_eq!(cached.source, Some(StatusSource::MemoryCache));
    tokio::time::advance(CACHE_TTL + Duration::from_secs(1)).await;
    let fresh = owner
        .read(Some(identity("one")), "0.5.22", || async {
            Ok("0.5.23".into())
        })
        .await;
    assert_eq!(fresh.state, CliUpdateState::Available);
    assert_eq!(fresh.source, Some(StatusSource::FreshCheck));
    let _held = owner.cache.lock().await;
    let skipped = owner
        .read(None, "0.5.22", || async {
            panic!("inactive request cannot reach HTTP")
        })
        .await;
    assert_eq!(skipped.state, CliUpdateState::NotChecked);
    assert!(skipped.checked_at.is_none());
}

#[test]
fn only_ready_and_active_login_has_an_update_identity() {
    let status = onecrew_creator_sync::CreatorSyncStatus {
        state: CreatorSyncAdmissionState::Active,
        action: None,
        issue: None,
        installation_id: Some("installation".into()),
        credential_id: Some("credential".into()),
    };
    let login = crate::login::LoginOwner::default().projection(status.clone());
    assert!(UpdateIdentity::eligible(true, Some(&login)).is_some());
    assert!(UpdateIdentity::eligible(false, Some(&login)).is_none());
    assert!(UpdateIdentity::eligible(true, None).is_none());
    let status = onecrew_creator_sync::CreatorSyncStatus {
        state: CreatorSyncAdmissionState::PairingRequired,
        ..status
    };
    let login = crate::login::LoginOwner::default().projection(status);
    assert!(UpdateIdentity::eligible(true, Some(&login)).is_none());
}
