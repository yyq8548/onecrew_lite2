//! Thin Web projections over the direct preparation and trusted confirmation owner.
use crate::{ServiceState, web_result::WebResultBinding};
use onecrew_local_runtime::CreatorSaveTelegramAdministrators;
use onecrew_local_runtime::{
    CreatorSaveBatchSelection, CreatorSavePreparationContext, PrepareCreatorSaveInput,
    ResultOrigin, ResultSnapshot,
};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct WebCreatorSaveConfirmation {
    binding: WebResultBinding,
    selections: Vec<CreatorSaveBatchSelection>,
}

fn preparation_context(origin: &ResultOrigin) -> CreatorSavePreparationContext {
    CreatorSavePreparationContext {
        origin: origin.clone(),
        call_id: uuid::Uuid::new_v4().to_string(),
        operation_id: "onecrew.creator.candidate.batch.save".into(),
    }
}

pub(super) async fn onecrew_creator_save_telegram_administrators_read(
    state: &ServiceState,
    input: onecrew_local_runtime::TelegramCommunityAdministratorsInput,
) -> Result<CreatorSaveTelegramAdministrators, String> {
    state
        .platform()
        .await?
        .read_creator_save_telegram_administrators(input)
        .await
        .map_err(|error| error.to_string())
}

pub(super) async fn onecrew_creator_save_prepare(
    state: &ServiceState,
    input: PrepareCreatorSaveInput,
    origin: &ResultOrigin,
) -> Result<ResultSnapshot, String> {
    let platform = state.platform().await?;
    let result_ref = platform
        .prepare_creator_save(input, preparation_context(origin))
        .await
        .map_err(|error| error.to_string())?;
    platform
        .present_result(&result_ref, origin)
        .map_err(|error| error.to_string())
}

pub(super) async fn onecrew_creator_save_confirm(
    state: &ServiceState,
    input: WebCreatorSaveConfirmation,
    origin: &ResultOrigin,
) -> Result<ResultSnapshot, String> {
    state
        .platform()
        .await?
        .confirm_creator_save(input.binding.bind(origin)?, input.selections)
        .await
        .map_err(|error| error.to_string())
}

pub(super) async fn onecrew_creator_save_result_get(
    state: &ServiceState,
    input: WebResultBinding,
    origin: &ResultOrigin,
) -> Result<ResultSnapshot, String> {
    state
        .platform()
        .await?
        .read_current_result(&input.bind(origin)?)
        .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn web_review_carries_the_real_save_operation_identity() {
        use onecrew_creator_runtime::{
            AccountRefId, CanonicalCreatorSaveInput, CreatorSaveProfile, CreatorSaveProvenance,
            Platform, PlatformAccountCandidate, TargetKind,
        };
        let directory = tempfile::tempdir().unwrap();
        let platform = onecrew_local_runtime::LocalPlatform::open(directory.path()).unwrap();
        let account_ref = AccountRefId::new("00000000-0000-0000-0000-000000000001");
        platform
            .creators()
            .unwrap()
            .sync_account_ref(account_ref.clone(), Platform::X, "profile", "fixture")
            .unwrap();
        let origin = ResultOrigin::new("web:save-review-fixture").unwrap();
        let result_ref = onecrew_local_runtime::test_support::mint_creator_save_review(
            &platform,
            vec![CanonicalCreatorSaveInput {
                row_id: "reviewed-row".into(),
                account: PlatformAccountCandidate {
                    platform: Platform::X,
                    external_id: Some("123".into()),
                    target_kind: Some(TargetKind::User),
                    handle: Some("fixture_creator".into()),
                    display_name: "Fixture Creator".into(),
                    profile_url: Some("https://x.com/fixture_creator".into()),
                },
                profile: CreatorSaveProfile::default(),
                profile_observed_at: 1_800_000_000,
                provenance: CreatorSaveProvenance::DirectSelection {
                    source_ref: "https://x.com/fixture_creator".into(),
                    source_url: Some("https://x.com/fixture_creator".into()),
                    description: None,
                    operator_account_ref_id: Some(account_ref),
                },
                contact_evidence: vec![],
                notes: None,
                tags: None,
                quality: Default::default(),
            }],
            preparation_context(&origin),
            onecrew_creator_runtime::CreatorSaveIntent::SaveOrUpdate,
        )
        .await
        .unwrap();
        let snapshot = platform.present_result(&result_ref, &origin).unwrap();
        let wire = serde_json::to_value(snapshot).unwrap();
        assert_eq!(wire["binding"]["origin"], origin.as_str());
        assert_eq!(
            wire["batch"]["outcomes"][0]["operation_id"],
            "onecrew.creator.candidate.batch.save"
        );
        assert_eq!(wire["pending_action"]["kind"], "creator_save_batch");
        assert_eq!(
            wire["pending_action"]["review"]["rows"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
    }

    #[test]
    fn browser_cannot_submit_an_origin_or_prepared_payload() {
        assert!(
            serde_json::from_value::<WebResultBinding>(serde_json::json!({
                "origin":"another-browser", "prepared":{},
            }))
            .is_err()
        );
    }

    #[test]
    fn a_review_binding_cannot_be_read_or_submitted_from_another_session() {
        use onecrew_local_runtime::{
            CallDisposition, ExecutedBatch, ExecutedCallOutcome, ExecutedResultInput,
            InitialResultState, LocalPlatform, NoticeTone, OwnerActionPayload, PendingUserAction,
            PresentationProjection, PresentationSection, ResultPresentationKind, ResultStoreError,
            ResultViewKind,
        };
        let directory =
            std::env::temp_dir().join(format!("onecrew-web-result-test-{}", uuid::Uuid::new_v4()));
        let platform = LocalPlatform::open(&directory).unwrap();
        let first = ResultOrigin::new("web:first").unwrap();
        let second = ResultOrigin::new("web:second").unwrap();
        let result_ref = platform
            .mint_execution_result(ExecutedResultInput {
                origin: first.clone(),
                view_kind: ResultViewKind::Confirmation,
                batch: ExecutedBatch {
                    outcomes: vec![ExecutedCallOutcome {
                        call_id: "call".into(),
                        operation_id: "onecrew.creator.candidate.batch.save".into(),
                        disposition: CallDisposition::Succeeded,
                        summary: "Prepared".into(),
                        resource_refs: vec![],
                    }],
                },
                presentation: PresentationProjection {
                    kind: ResultPresentationKind::new("record_preview").unwrap(),
                    title: "Review".into(),
                    summary: None,
                    sections: vec![PresentationSection::Notice {
                        heading: None,
                        body: "Review before saving".into(),
                        tone: NoticeTone::Neutral,
                    }],
                },
                pending_action: Some(PendingUserAction::Confirm {
                    prompt: "Save?".into(),
                    label: "Confirm".into(),
                    payload: OwnerActionPayload::new(b"fixture".to_vec()).unwrap(),
                }),
                initial_state: InitialResultState::Available,
            })
            .unwrap();
        let snapshot = platform.present_result(&result_ref, &first).unwrap();
        let mut wire = serde_json::to_value(&snapshot.binding).unwrap();
        assert!(
            serde_json::from_value::<WebResultBinding>(wire.clone()).is_err(),
            "origin must not be a wire input"
        );
        wire.as_object_mut().unwrap().remove("origin");
        let binding = serde_json::from_value::<WebResultBinding>(wire.clone())
            .unwrap()
            .bind(&second)
            .unwrap();
        assert_eq!(
            platform.read_current_result(&binding).unwrap_err(),
            ResultStoreError::OriginMismatch
        );
        assert_eq!(
            platform.submit_result_confirmation(&binding).unwrap_err(),
            ResultStoreError::OriginMismatch
        );
        let binding = serde_json::from_value::<WebResultBinding>(wire)
            .unwrap()
            .bind(&first)
            .unwrap();
        assert!(platform.read_current_result(&binding).is_ok());
        assert!(platform.submit_result_confirmation(&binding).is_ok());
        drop(platform);
        std::fs::remove_dir_all(directory).unwrap();
    }
}
