//! Canonical save fixtures shared by Creator owner tests. Provider discovery,
//! write effects and their scripted transports belong to platform-runtime.
use crate::{
    AccountRefId, ContactChannel, CreatorRuntime, LocatorKind, Platform, SourceKind, TargetKind,
};

/// Direct canonical profile evidence for Creator behavior tests. Provider
/// lookup and search authorization are tested by their runtime owners.
pub(super) fn x_save_entry(
    account_ref_id: AccountRefId,
    with_telegram: bool,
) -> crate::CanonicalCreatorSaveInput {
    let mut contact_evidence = vec![crate::CreatorSaveContactEvidence::Observation {
        channel: ContactChannel::LinkHub,
        kind: LocatorKind::Url,
        value: "https://linktr.ee/creator_42".into(),
        source: SourceKind::ProfileWebsite,
        source_url: Some("https://x.com/creator_42".into()),
    }];
    if with_telegram {
        contact_evidence.push(crate::CreatorSaveContactEvidence::Observation {
            channel: ContactChannel::Telegram,
            kind: LocatorKind::Handle,
            value: "https://t.me/kriptomessitg".into(),
            source: SourceKind::LinkHub,
            source_url: Some("https://linktr.ee/creator_42".into()),
        });
    }
    crate::CanonicalCreatorSaveInput {
        row_id: "x-profile-42".into(),
        account: crate::PlatformAccountCandidate {
            platform: Platform::X,
            external_id: Some("x-creator-42".into()),
            target_kind: Some(TargetKind::User),
            handle: Some("creator_42".into()),
            display_name: "Creator 42".into(),
            profile_url: Some("https://x.com/creator_42".into()),
        },
        profile: crate::CreatorSaveProfile {
            follower_count: Some(42),
            ..Default::default()
        },
        profile_observed_at: 1_700_000_000,
        provenance: crate::CreatorSaveProvenance::ProviderRead {
            operation_id: "x.accounts.get".into(),
            source_ref: "x-profile-42".into(),
            source_url: Some("https://x.com/creator_42".into()),
            description: None,
            operator_account_ref_id: Some(account_ref_id),
        },
        contact_evidence,
        notes: None,
        tags: None,
        quality: Default::default(),
    }
}

pub(super) fn select_required_routes(
    prepared: crate::PreparedCreatorSaveBatch,
) -> Result<crate::SelectedPreparedCreatorSaveBatch, String> {
    let selections = prepared
        .rows()
        .into_iter()
        .map(|row| crate::CreatorSaveRouteSelection {
            row_id: row.row_id,
            route_ids: row
                .route_options
                .into_iter()
                .filter(|route| route.required)
                .map(|route| route.route_id)
                .collect(),
        })
        .collect();
    prepared
        .select_routes(selections)
        .map_err(|error| error.to_string())
}

/// Exercise all public save stages for one fixture; no snapshot lookup or
/// production compatibility API is involved.
pub(super) fn save_fixture(
    runtime: &CreatorRuntime,
    input: crate::CanonicalCreatorSaveInput,
    execution_key: &str,
) -> Result<crate::CreatorSaveRowOutcome, String> {
    let prepared = runtime
        .prepare_creator_save_batch(vec![input], crate::CreatorSaveIntent::SaveOrUpdate)
        .map_err(|error| error.to_string())?;
    let selected = select_required_routes(prepared)?;
    let outcome = runtime
        .execute_creator_save_batch(selected, execution_key.to_owned())?
        .outcomes
        .into_iter()
        .next()
        .ok_or("save produced no row")?;
    match outcome {
        crate::CreatorSaveRowOutcome::Failed { error, .. } => Err(error),
        saved => Ok(saved),
    }
}

pub(super) async fn assert_runtime_health() {
    tokio::time::timeout(
        std::time::Duration::from_millis(50),
        tokio::spawn(async { tokio::task::yield_now().await }),
    )
    .await
    .expect("repository segment starved the Tokio workers")
    .unwrap();
}
