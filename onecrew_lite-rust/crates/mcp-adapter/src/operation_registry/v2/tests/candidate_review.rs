//! 候选只准备一次，原卡确认后仍能回读各自的数值评分与证据。
use std::sync::{Arc, atomic::Ordering};

use onecrew_creator_runtime::{CreatorId, CreatorSaveIntent, CreatorSaveProvenance};
use onecrew_local_runtime::{PendingActionView, PresentationSection};

use super::{creator_save_support as support, fixtures::result_owner};
use crate::operation_registry::{
    descriptor::SurfaceId,
    v2::contract::{ResultStateInput, ResultStateOutput},
};

#[tokio::test]
async fn candidate_review_keeps_individual_evidence_and_scores_through_one_confirmation() {
    let (_directory, platform, origin) = result_owner();
    let entries = support::entries(&platform, 50)
        .into_iter()
        .enumerate()
        .map(|(index, entry)| {
            let CreatorSaveProvenance::DirectSelection {
                operator_account_ref_id,
                ..
            } = &entry.provenance
            else {
                panic!("fixture provenance")
            };
            onecrew_creator_runtime::CanonicalCreatorSaveInput {
                provenance: CreatorSaveProvenance::ProviderRead {
                    operation_id: onecrew_local_runtime::OperationId::X_POSTS_SEARCH_ADVANCED
                        .as_str()
                        .into(),
                    source_ref: format!("post-{index}"),
                    source_url: Some(format!("https://x.com/person{index}/status/{}", index + 1)),
                    description: Some(format!(
                        "Candidate {index}: published a working agent example"
                    )),
                    operator_account_ref_id: operator_account_ref_id.clone(),
                },
                ..entry
            }
        })
        .collect();
    let fixture = support::registry_with_entries(Arc::clone(&platform), entries);
    let fixture = support::Fixture {
        candidates: fixture.candidates.into_iter().enumerate().map(|(index, candidate)| {
            onecrew_local_runtime::CreatorSaveCandidateInput {
                intent: CreatorSaveIntent::AddNew,
                provenance: onecrew_local_runtime::CreatorSaveProvenanceInput::SearchSelection {
                    origin: onecrew_local_runtime::CreatorSaveSearchOrigin::ProviderRead {
                        operation_id: onecrew_local_runtime::OperationId::X_POSTS_SEARCH_ADVANCED.as_str().into(),
                        source_ref: format!("post-{index}"),
                    },
                    source_url: Some(format!("https://x.com/person{index}/status/{}", index + 1)),
                    description: Some(format!("Candidate {index}: published a working agent example")),
                },
                quality: onecrew_creator_runtime::CreatorQualityInput {
                    score: [Some(95), Some(85), Some(70), Some(40), Some(87), Some(0), None][index % 7],
                    assessment: Some(format!("Candidate {index}: demonstrated agent task {index}; evidence is specific to this candidate.")),
                    ..candidate.quality
                },
                ..candidate
            }
        }).collect(),
        ..fixture
    };
    let available = support::prepare_review(&fixture, &platform, &origin).await;
    assert_eq!(support::saved_count(&platform), 0);
    assert_eq!(fixture.prepares.load(Ordering::SeqCst), 1);
    let Some(PendingActionView::CreatorSaveBatch { review }) = &available.pending_action else {
        panic!("one selectable candidate review")
    };
    assert_eq!(review.rows.len(), 50);
    assert_eq!(review.summary.eligible_new, 50);
    for (index, row) in review.rows.iter().enumerate() {
        assert_eq!(row.preview.score, fixture.candidates[index].quality.score);
        assert_eq!(
            row.preview.assessment,
            fixture.candidates[index].quality.assessment
        );
        let wire = serde_json::to_value(&row.preview).unwrap();
        assert_eq!(
            wire["source_url"],
            format!("https://x.com/person{index}/status/{}", index + 1)
        );
        assert!(wire.get("grade").is_none(), "grade remains a UI derivation");
    }
    let selected_indices = [0, 1, 2, 3, 4, 5, 6, 24, 49];
    let selections = support::selections(&available, 50)
        .into_iter()
        .enumerate()
        .filter_map(|(index, selection)| selected_indices.contains(&index).then_some(selection))
        .collect::<Vec<_>>();
    let request = ResultStateInput::ConfirmCreatorSaveBatch {
        binding: available.binding.clone(),
        selections: selections.clone(),
    };
    let ResultStateOutput::Snapshot {
        snapshot: terminal, ..
    } = fixture
        .registry
        .result_state_v2(&platform, SurfaceId::Full, request)
        .await
        .unwrap()
    else {
        panic!("saved result on original card")
    };
    assert_eq!(
        fixture.prepares.load(Ordering::SeqCst),
        1,
        "confirm cannot re-prepare or refetch"
    );
    assert_eq!(terminal.binding.result_ref, available.binding.result_ref);
    assert_eq!(support::saved_count(&platform), selected_indices.len());
    let rows = terminal
        .presentation
        .sections
        .iter()
        .flat_map(|section| match section {
            PresentationSection::CreatorSaveRows { rows } => rows.as_slice(),
            _ => &[],
        })
        .collect::<Vec<_>>();
    assert_eq!(rows.len(), 50);
    for index in selected_indices {
        let id = CreatorId::new(rows[index].creator_id.as_ref().unwrap());
        let record = platform.creators().unwrap().get_creator(&id).unwrap();
        assert_eq!(
            record.creator.score,
            fixture.candidates[index].quality.score
        );
        assert_eq!(
            record.creator.assessment,
            fixture.candidates[index].quality.assessment
        );
    }
    assert!(
        fixture
            .registry
            .result_state_v2(
                &platform,
                SurfaceId::Full,
                ResultStateInput::ConfirmCreatorSaveBatch {
                    binding: available.binding,
                    selections
                }
            )
            .await
            .is_err()
    );
    assert_eq!(support::saved_count(&platform), selected_indices.len());
}

#[tokio::test]
async fn candidate_review_add_new_keeps_an_existing_creators_original_assessment() {
    let (_directory, platform, origin) = result_owner();
    let fixture = support::registry(Arc::clone(&platform), 2);
    let fixture = support::Fixture {
        candidates: fixture
            .candidates
            .into_iter()
            .map(
                |candidate| onecrew_local_runtime::CreatorSaveCandidateInput {
                    intent: CreatorSaveIntent::AddNew,
                    quality: onecrew_creator_runtime::CreatorQualityInput {
                        score: Some(87),
                        assessment: Some("Original individual assessment".into()),
                        ..candidate.quality
                    },
                    ..candidate
                },
            )
            .collect(),
        ..fixture
    };
    let initial = support::prepare_review(&fixture, &platform, &origin).await;
    let ResultStateOutput::Snapshot { snapshot, .. } = fixture
        .registry
        .result_state_v2(
            &platform,
            SurfaceId::Full,
            ResultStateInput::ConfirmCreatorSaveBatch {
                binding: initial.binding.clone(),
                selections: support::selections(&initial, 1),
            },
        )
        .await
        .unwrap()
    else {
        panic!("seeded one Creator")
    };
    let creator_id = snapshot
        .presentation
        .sections
        .iter()
        .find_map(|section| match section {
            PresentationSection::CreatorSaveRows { rows } => rows[0].creator_id.clone(),
            _ => None,
        })
        .unwrap();
    let fixture = support::Fixture {
        candidates: fixture
            .candidates
            .into_iter()
            .map(
                |candidate| onecrew_local_runtime::CreatorSaveCandidateInput {
                    quality: onecrew_creator_runtime::CreatorQualityInput {
                        score: Some(95),
                        assessment: Some(
                            "New task assessment must not overwrite an existing Creator".into(),
                        ),
                        ..candidate.quality
                    },
                    ..candidate
                },
            )
            .collect(),
        ..fixture
    };
    let review = support::prepare_review(&fixture, &platform, &origin).await;
    let Some(PendingActionView::CreatorSaveBatch { review: batch }) = &review.pending_action else {
        panic!("mixed review")
    };
    assert_eq!(batch.summary.existing_skipped, 1);
    assert_eq!(batch.summary.eligible_new, 1);
    assert_eq!(support::saved_count(&platform), 1);
    let selections = support::selections(&review, 2)
        .into_iter()
        .skip(1)
        .collect();
    fixture
        .registry
        .result_state_v2(
            &platform,
            SurfaceId::Full,
            ResultStateInput::ConfirmCreatorSaveBatch {
                binding: review.binding,
                selections,
            },
        )
        .await
        .unwrap();
    let record = platform
        .creators()
        .unwrap()
        .get_creator(&CreatorId::new(creator_id))
        .unwrap();
    assert_eq!(record.creator.score, Some(87));
    assert_eq!(
        record.creator.assessment.as_deref(),
        Some("Original individual assessment")
    );
    assert_eq!(support::saved_count(&platform), 2);
}
