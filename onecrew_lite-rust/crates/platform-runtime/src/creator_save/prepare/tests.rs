use super::*;
use onecrew_creator_runtime::{
    CreatorQualityInput, CreatorSaveIntent, CreatorSaveProfile, Platform, PlatformAccountCandidate,
    TargetKind,
};

#[tokio::test]
async fn a_blank_or_oversized_profile_field_costs_that_field_and_not_the_card() {
    let directory = tempfile::tempdir().unwrap();
    let platform = LocalPlatform::open(directory.path()).unwrap();
    let account = Uuid::from_u128(1);
    let creators = platform.creators().unwrap();
    creators
        .sync_account_ref(
            AccountRefId::new(account.to_string()),
            Platform::X,
            "profile",
            "@operator",
        )
        .unwrap();
    let mut blank = read();
    blank.account.external_id = Some("blank-1".into());
    blank.account.handle = Some("blank".into());
    blank.account.profile_url = Some("https://x.com/blank".into());
    // A provider may publish a bio that is only whitespace. The projection used
    // to offer it as a field, and the validator it feeds refuses blank text.
    blank.profile.description = Some("   \n\t  ".into());
    blank.profile.location = Some("  ".into());
    let mut oversized = read();
    oversized.account.external_id = Some("oversized-1".into());
    oversized.account.handle = Some("oversized".into());
    oversized.account.display_name = "Oversized Person".into();
    oversized.account.profile_url = Some("https://x.com/oversized".into());
    // Multibyte, so a byte-indexed truncation that ignores character
    // boundaries would panic rather than merely fail.
    oversized.profile.description = Some("背景".repeat(8000));
    let entries: Vec<_> = [blank, oversized]
        .into_iter()
        .enumerate()
        .map(|(index, read)| CanonicalCreatorSaveInput {
            row_id: format!("row-{index}"),
            account: read.account,
            profile: read.profile,
            profile_observed_at: 1_800_000_000,
            provenance: CreatorSaveProvenance::DirectSelection {
                source_ref: "https://x.com/person".into(),
                source_url: Some("https://x.com/person".into()),
                description: None,
                operator_account_ref_id: Some(AccountRefId::new(account.to_string())),
            },
            contact_evidence: Vec::new(),
            notes: None,
            tags: None,
            quality: CreatorQualityInput::default(),
        })
        .collect();
    let prepared = creators
        .prepare_creator_save_batch_async(entries, CreatorSaveIntent::SaveOrUpdate)
        .await
        .unwrap();
    let rows = prepared.rows();

    let sections = projection::details(&rows);
    let crate::PresentationSection::Fields { fields, .. } = &sections[0] else {
        panic!("expected a fields section")
    };
    assert!(
        fields.iter().all(|field| field.label != "Description")
            && fields.iter().all(|field| field.label != "Location"),
        "a whitespace-only value must be absent, not offered as an empty field"
    );
    let crate::PresentationSection::Fields { heading, fields } = &sections[1] else {
        panic!("expected a fields section")
    };
    assert_eq!(heading.as_deref(), Some("Oversized Person"));
    let description = fields
        .iter()
        .find(|field| field.label == "Description")
        .unwrap();
    assert!(description.value.len() <= 8 * 1024);
    assert!(description.value.ends_with("(truncated)"));

    let context = OwnerReviewContext::new(
        crate::ResultOrigin::new("bounded-projection").unwrap(),
        "save",
        "onecrew.creator.candidate.batch.save",
        None,
    )
    .unwrap();
    let frame = singleton_frame(&context);
    let review = seal_prepared_review(prepared, context, &[], &[], false, PREPARED_TTL).unwrap();
    // The whole card, and every provider read behind it, used to be lost
    // because the projection offered text its own validator refuses.
    platform
        .mint_owner_review(review, frame, None, Vec::new())
        .unwrap();
}

fn x_candidate(handle: &str) -> crate::CreatorSaveCandidateInput {
    crate::CreatorSaveCandidateInput {
        intent: onecrew_creator_runtime::CreatorSaveIntent::SaveOrUpdate,
        target: crate::CreatorSaveTarget::X {
            user_id: "123".into(),
            handle: handle.into(),
        },
        connection: CreatorSaveConnection::X {
            account_ref_id: Uuid::from_u128(1),
        },
        provenance: CreatorSaveProvenanceInput::DirectSelection {
            source_url: "https://x.com/person".into(),
            description: None,
        },
        notes: None,
        tags: None,
        quality: CreatorQualityInput::default(),
    }
}

#[test]
fn one_unreadable_profile_does_not_discard_the_reads_that_succeeded() {
    let mut warnings = PrepareWarnings::default();
    let mut second = read();
    second.account.display_name = "Second Person".into();
    let (canonical, labels, failures) = canonicalize(
        vec![
            (x_candidate("first"), Ok(read())),
            (
                x_candidate("missing"),
                Err(CreatorSaveError::ProviderRead {
                    code: "x_profile_read".into(),
                    message: "profile is unavailable".into(),
                }),
            ),
            (x_candidate("second"), Ok(second)),
        ],
        &mut warnings,
    )
    .unwrap();
    assert_eq!(
        canonical.len(),
        2,
        "one failed read discarded the whole batch"
    );
    assert_eq!(labels.len(), 2);
    assert_eq!(failures.len(), 1);
    assert_eq!(failures[0].display_name.as_deref(), Some("@missing on X"));
    assert_eq!(
        failures[0].reason,
        onecrew_creator_runtime::CreatorSavePreparedReason::PreparationFailed
    );
    let rendered = warnings.render();
    assert_eq!(rendered.len(), 1);
    assert!(
        rendered[0].contains("@missing on X"),
        "the warning must name the selection it left out: {}",
        rendered[0]
    );
}

#[test]
fn a_shared_precondition_failure_is_not_degraded_to_one_row() {
    // An unavailable Creator owner is not this candidate's bad data, and must
    // not be reported as one unusable Creator among the rest.
    let mut warnings = PrepareWarnings::default();
    let error = canonicalize(
        vec![
            (x_candidate("first"), Ok(read())),
            (
                x_candidate("second"),
                Err(CreatorSaveError::CreatorUnavailable),
            ),
        ],
        &mut warnings,
    )
    .unwrap_err();
    assert!(matches!(error, CreatorSaveError::CreatorUnavailable));
}

#[test]
fn a_batch_whose_every_read_failed_keeps_its_own_reason() {
    let mut warnings = PrepareWarnings::default();
    let (canonical, labels, failures) = canonicalize(
        vec![(
            x_candidate("missing"),
            Err(CreatorSaveError::IdentityMismatch),
        )],
        &mut warnings,
    )
    .unwrap();
    assert!(canonical.is_empty());
    assert!(labels.is_empty());
    assert_eq!(failures.len(), 1);
    assert_eq!(failures[0].display_name.as_deref(), Some("@missing on X"));
    assert_eq!(
        failures[0].reason,
        onecrew_creator_runtime::CreatorSavePreparedReason::PreparationFailed
    );
}

#[test]
fn a_repeated_warning_keeps_every_creator_it_is_about() {
    let mut warnings = PrepareWarnings::default();
    for index in 0..48 {
        warnings.add(
            "A linked contact page could not be read.",
            Some(&format!("Creator {index}")),
        );
    }
    let rendered = warnings.render();
    // One notice rather than forty-eight, and not one that names nobody.
    assert_eq!(rendered.len(), 1);
    assert!(rendered[0].contains("Creator 0"), "{}", rendered[0]);
    assert!(rendered[0].contains("and 38 more"), "{}", rendered[0]);
}

fn read() -> reads::ReadCreatorProfile {
    reads::ReadCreatorProfile {
        observed_at: SystemTime::now(),
        account: PlatformAccountCandidate {
            platform: Platform::X,
            external_id: Some("123".into()),
            target_kind: Some(TargetKind::User),
            handle: Some("person".into()),
            display_name: "Person".into(),
            profile_url: Some("https://x.com/person".into()),
        },
        profile: CreatorSaveProfile::default(),
        evidence: Vec::new(),
        warnings: Vec::new(),
        administrator: None,
    }
}

fn no_op_context() -> OwnerReviewContext {
    OwnerReviewContext::new(
        crate::ResultOrigin::new("no-op-review").unwrap(),
        "save",
        "onecrew.creator.candidate.batch.save",
        None,
    )
    .unwrap()
}

fn no_op_fixture() -> (tempfile::TempDir, LocalPlatform, CanonicalCreatorSaveInput) {
    let directory = tempfile::tempdir().unwrap();
    let platform = LocalPlatform::open(directory.path()).unwrap();
    platform
        .creators()
        .unwrap()
        .sync_account_ref(
            AccountRefId::new(Uuid::from_u128(1).to_string()),
            Platform::X,
            "profile",
            "@operator",
        )
        .unwrap();
    let (mut inputs, _, failures) = canonicalize(
        vec![(x_candidate("person"), Ok(read()))],
        &mut PrepareWarnings::default(),
    )
    .unwrap();
    assert!(failures.is_empty());
    (directory, platform, inputs.remove(0))
}

#[tokio::test]
async fn all_existing_add_new_preparation_completes_without_confirmation() {
    let (_directory, platform, input) = no_op_fixture();
    let creators = platform.creators().unwrap();
    let prepared = creators
        .prepare_creator_save_batch_async(vec![input.clone()], CreatorSaveIntent::SaveOrUpdate)
        .await
        .unwrap();
    let selections = prepared
        .rows()
        .iter()
        .map(|row| onecrew_creator_runtime::CreatorSaveRouteSelection {
            row_id: row.row_id.clone(),
            route_ids: row
                .route_options
                .iter()
                .filter(|route| route.required)
                .map(|route| route.route_id.clone())
                .collect(),
        })
        .collect();
    creators
        .execute_creator_save_batch_async(
            prepared.select_routes(selections).unwrap(),
            "existing-seed".into(),
        )
        .await
        .unwrap();
    let prepared = creators
        .prepare_creator_save_batch_async(vec![input], CreatorSaveIntent::AddNew)
        .await
        .unwrap();
    let review =
        seal_prepared_review(prepared, no_op_context(), &[], &[], true, PREPARED_TTL).unwrap();
    assert!(review.is_noop());
    let result = review.into_noop().unwrap();
    assert!(matches!(result.terminal(), TerminalResult::Resolved(_)));
    assert_eq!(result.output()["summary"]["total"], 1);
    assert_eq!(result.output()["summary"]["existing_skipped"], 1);
    assert_eq!(result.output()["summary"]["updated"], 0);
    assert_eq!(
        creators
            .list_creators(Default::default())
            .unwrap()
            .items
            .len(),
        1
    );
}

#[tokio::test]
async fn all_conflicting_updates_complete_as_failed_without_confirmation() {
    let (_directory, platform, first) = no_op_fixture();
    let second = CanonicalCreatorSaveInput {
        row_id: "contradictory-update".into(),
        notes: Some("Different reviewed notes".into()),
        ..first.clone()
    };
    let creators = platform.creators().unwrap();
    let prepared = creators
        .prepare_creator_save_batch_async(vec![first, second], CreatorSaveIntent::SaveOrUpdate)
        .await
        .unwrap();
    let review =
        seal_prepared_review(prepared, no_op_context(), &[], &[], false, PREPARED_TTL).unwrap();
    assert!(review.is_noop());
    let result = review.into_noop().unwrap();
    assert!(matches!(result.terminal(), TerminalResult::Failed(_)));
    assert_eq!(result.output()["summary"]["total"], 2);
    assert_eq!(result.output()["summary"]["failed"], 2);
    assert_eq!(result.output()["rows"][0]["reason"], "conflicting_update");
    assert!(
        creators
            .list_creators(Default::default())
            .unwrap()
            .items
            .is_empty()
    );
}

#[test]
fn every_failed_read_remains_in_the_terminal_no_op_summary() {
    let (canonical, _, failures) = canonicalize(
        vec![(
            x_candidate("missing"),
            Err(CreatorSaveError::IdentityMismatch),
        )],
        &mut PrepareWarnings::default(),
    )
    .unwrap();
    assert!(canonical.is_empty());
    let review = no_op_review(
        no_op_context(),
        crate::CreatorSaveBatchReview {
            rows: Vec::new(),
            failures,
            intent: CreatorSaveIntent::AddNew,
            summary: onecrew_creator_runtime::CreatorSaveBatchSummary {
                total: 1,
                failed: 1,
                ..Default::default()
            },
        },
        true,
    )
    .unwrap();
    assert!(review.is_noop());
    let result = review.into_noop().unwrap();
    assert!(matches!(result.terminal(), TerminalResult::Unknown(_)));
    assert_eq!(result.output()["summary"]["total"], 1);
    assert_eq!(result.output()["summary"]["failed"], 1);
    assert_eq!(result.output()["summary"]["unknown"], 0);
    assert_eq!(result.output()["rows"][0]["status"], "not_run");
    assert_eq!(result.output()["rows"][0]["reason"], "preparation_failed");
}

#[tokio::test]
async fn source_context_and_absent_descriptions_survive_mapping_and_canonical_prepare() {
    let directory = tempfile::tempdir().unwrap();
    let platform = LocalPlatform::open(directory.path()).unwrap();
    let account = Uuid::from_u128(1);
    let creators = platform.creators().unwrap();
    creators
        .sync_account_ref(
            AccountRefId::new(account.to_string()),
            Platform::X,
            "profile",
            "@operator",
        )
        .unwrap();
    let sources = vec![
        CreatorSaveProvenanceInput::DirectSelection {
            source_url: "https://x.com/person".into(),
            description: None,
        },
        CreatorSaveProvenanceInput::SearchSelection {
            origin: CreatorSaveSearchOrigin::ProviderRead {
                operation_id: "x.users.search".into(),
                source_ref: "provider-page".into(),
            },
            source_url: None,
            description: None,
        },
        CreatorSaveProvenanceInput::SearchSelection {
            origin: CreatorSaveSearchOrigin::Discovery {
                discovery_id: "original-discovery".into(),
                candidate_id: "original-candidate".into(),
                query: "real search query".into(),
                search_kind: "users".into(),
                operation_id: "x.users.search".into(),
                source_ref: "original-result-page".into(),
            },
            source_url: Some("https://x.com/search?q=real".into()),
            description: Some("Original source description".into()),
        },
    ];
    for (index, source) in sources.into_iter().enumerate() {
        let candidate = crate::CreatorSaveCandidateInput {
            intent: onecrew_creator_runtime::CreatorSaveIntent::SaveOrUpdate,
            target: crate::CreatorSaveTarget::X {
                user_id: "123".into(),
                handle: "person".into(),
            },
            connection: CreatorSaveConnection::X {
                account_ref_id: account,
            },
            provenance: source,
            notes: None,
            tags: None,
            quality: CreatorQualityInput::default(),
        };
        candidate.validate().unwrap();
        let mut read = read();
        let provenance = provenance(&candidate, &mut read).unwrap();
        let prepared = creators
            .prepare_creator_save_batch_async(
                vec![CanonicalCreatorSaveInput {
                    row_id: format!("row-{index}"),
                    account: read.account,
                    profile: read.profile,
                    profile_observed_at: 1_800_000_000,
                    provenance,
                    contact_evidence: Vec::new(),
                    notes: None,
                    tags: None,
                    quality: CreatorQualityInput::default(),
                }],
                CreatorSaveIntent::SaveOrUpdate,
            )
            .await
            .unwrap();
        match &prepared.rows()[0].provenance {
            CreatorSaveProvenance::DirectSelection { description, .. }
            | CreatorSaveProvenance::ProviderRead { description, .. } => {
                assert!(description.is_none())
            }
            CreatorSaveProvenance::Discovery {
                candidate_id,
                origin,
                selection_context,
            } => {
                assert_eq!(candidate_id, "original-candidate");
                assert_eq!(origin.discovery_id().as_str(), "original-discovery");
                let context = selection_context.as_ref().unwrap();
                assert_eq!(context.operation_id, "x.users.search");
                assert_eq!(context.source_ref, "original-result-page");
                assert_eq!(
                    context.source_url.as_deref(),
                    Some("https://x.com/search?q=real")
                );
                assert_eq!(
                    context.description.as_deref(),
                    Some("Original source description")
                );
            }
            _ => panic!("unexpected provenance"),
        }
    }
}
