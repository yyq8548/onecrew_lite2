use std::path::PathBuf;

use rusqlite::Connection;

use crate::{
    AccountAvailability, AccountKind, AccountRefId, CommitContactTargetResolutionInput,
    CommitContactTargetResolutionResult, CommunityAdministratorRole, CommunityAdministratorSource,
    CommunityReference, CommunityTargetKind, Confidence, ContactChannel, ContactLocatorInput,
    CreateCreatorInput, CreatorCandidateOrigin, CreatorContactRouteAction, CreatorDatabaseStatus,
    CreatorId, CreatorOrigin, DiscoveredContactRoute, DiscoveryId,
    EmailAuthConnectionDeletionClaim, EmailAuthConnectionId, EmailCapability, EmailDirection,
    EmailFolder, EmailMessageInput, EmailProviderKind, EmailRecoveryStage, EmailSecretRecordId,
    EmailSnapshotInput, EmailThreadFolder, IdentityResolution, LinkState, LocatorKind,
    LocatorResolutionWriteResult, MailboxIdentityMappingInput, MailboxProjectionKind, MailboxRefId,
    MailboxRefInput, ManualCreatorContactRouteAddInput, ManualCreatorContactRouteUpdateInput,
    ManualCreatorContactRouteUpdateStatus, ObservationSourceInput, OperatorAccountDisconnectClaim,
    Platform, PlatformAccountCandidate, PlatformAccountObservationInput, RelationshipKind,
    RelationshipValue, SaveEmailDraftInput, SaveEmailTemplateInput, ServiceCredentialDeleteClaim,
    SourceKind, TargetKind, UpsertPlatformAccountInput,
    repository::{
        AppendMessageInput, ContactRepository, ConversationRepository, CreatorRepository,
        EmailDraftRepository, EmailRepository, EmailTemplateRepository, IdempotencyRepository,
        MailboxRepository, OperatorAccountRepository, RelationshipRepository, TargetRepository,
    },
};

use super::{SqliteRepository, schema};

fn repository() -> (tempfile::TempDir, SqliteRepository) {
    let directory = tempfile::tempdir().expect("temp directory");
    let repository = SqliteRepository::open(directory.path().join("crm.db")).expect("repository");
    (directory, repository)
}

fn current_database() -> (tempfile::TempDir, PathBuf) {
    let (directory, repository) = repository();
    let path = directory.path().join("crm.db");
    drop(repository);
    (directory, path)
}

fn current_schema_error(path: &std::path::Path) -> String {
    match SqliteRepository::open(path) {
        Ok(_) => panic!("non-current Creator schema must be rejected"),
        Err(error) => error,
    }
}

/// A platform-search origin owned by a fresh X operator account: the only
/// provenance an agent search produces.
fn platform_search_origin(repository: &SqliteRepository) -> CreatorCandidateOrigin {
    let account_ref = repository
        .upsert_account_ref(
            AccountRefId::new("x-profile-phase-2"),
            Platform::X,
            "profile",
            "@operator",
        )
        .expect("account ref");
    CreatorCandidateOrigin::PlatformSearch {
        discovery_id: DiscoveryId::new("discovery-phase-2"),
        account_ref_id: account_ref.id,
        query: "crypto".into(),
        search_kind: "users".into(),
    }
}

fn entry(
    origin: CreatorCandidateOrigin,
    external_id: &str,
    handle: &str,
) -> crate::CanonicalCreatorSaveInput {
    crate::CanonicalCreatorSaveInput {
        row_id: format!("x:user:{external_id}"),
        provenance: crate::CreatorSaveProvenance::Discovery {
            candidate_id: format!("x:user:{external_id}"),
            origin,
            selection_context: None,
        },
        account: PlatformAccountCandidate {
            platform: Platform::X,
            external_id: Some(external_id.into()),
            target_kind: Some(TargetKind::User),
            handle: Some(handle.into()),
            display_name: "Phase Two Creator".into(),
            profile_url: Some(format!("https://x.com/{handle}")),
        },
        profile: crate::CreatorSaveProfile {
            follower_count: Some(42),
            ..Default::default()
        },
        profile_observed_at: 1_700_000_000,
        contact_evidence: vec![],
        notes: None,
        tags: None,
        quality: Default::default(),
    }
}

#[derive(Debug)]
struct SavedCreatorFixture {
    creator: crate::CreatorRecord,
    account: PlatformAccountCandidate,
    created: bool,
}

fn save_input(
    mut input: crate::CanonicalCreatorSaveInput,
    notes: Option<&str>,
) -> crate::CanonicalCreatorSaveInput {
    input.notes = notes.map(str::to_owned);
    input
}

fn persist_entry(
    repository: &SqliteRepository,
    input: crate::CanonicalCreatorSaveInput,
) -> Result<SavedCreatorFixture, String> {
    let account = input.account.clone();
    let prepared = repository
        .prepare_creator_save_batch(vec![input], crate::CreatorSaveIntent::SaveOrUpdate)
        .map_err(|error| error.to_string())?;
    let selections = prepared
        .rows()
        .into_iter()
        .map(|row| crate::CreatorSaveRouteSelection {
            row_id: row.row_id,
            route_ids: row
                .route_options
                .into_iter()
                .map(|route| route.route_id)
                .collect(),
        })
        .collect();
    let selected = prepared
        .select_routes(selections)
        .map_err(|error| error.to_string())?;
    let (mut result, _) =
        repository.execute_creator_save_batch(selected, uuid::Uuid::new_v4().to_string())?;
    match result.outcomes.remove(0) {
        crate::CreatorSaveRowOutcome::Failed { error, .. } => Err(error),
        crate::CreatorSaveRowOutcome::Saved {
            creator, created, ..
        } => Ok(SavedCreatorFixture {
            creator: *creator,
            account,
            created,
        }),
        crate::CreatorSaveRowOutcome::Skipped { .. } => {
            Err("save fixture was unexpectedly skipped".into())
        }
    }
}

mod account_lifecycle;
mod contact_route_mutations;
mod contact_targets;
mod creator_candidates;
mod creator_groups;
mod creator_import;
mod creator_last_touch;
mod creator_list_filters;
mod creator_manual_batch;
mod creator_pinning;
mod creator_tags;
mod creator_transfer;
mod email_sync_state;
mod engagement_relationships;
mod execution_contract;
mod mail_state;
mod mailbox_projection;
mod observation_sources;
mod release_mapping;
mod reply_triage;
mod schema_contract;
mod schema_contract_v36;
mod schema_contract_v37;
mod schema_contract_v38;
mod schema_contract_v39;
mod schema_contract_v40;
mod schema_contract_v41;
mod schema_contract_v42;
mod schema_contract_v43;
mod schema_contract_v44;
mod schema_contract_v45;
mod schema_contract_v46;
mod schema_contract_v47;
mod schema_contract_v48;
mod schema_contract_v49;
mod schema_contract_v50;
mod schema_contract_v51;
mod schema_contract_v52;
mod schema_contract_v53;
mod schema_contract_v54;
mod schema_contract_v55;
mod schema_contract_v56;
mod schema_contract_v57;
mod schema_contract_v58;
mod schema_contract_v59;
mod schema_contract_v60;
mod schema_contract_v61;
mod schema_contract_v62;
mod schema_contract_v63;
mod schema_contract_v64;
mod schema_contract_v65;
mod schema_contract_v66;
mod schema_contract_v67;
mod schema_contract_v68;
mod schema_contract_v69;
mod schema_contract_v70;
mod schema_contract_v71;
mod schema_contract_v72;
mod topology;

mod discovery_search_order;
mod schema_contract_v73;
mod schema_contract_v74;
mod schema_contract_v75;
mod schema_contract_v76;
mod schema_contract_v77;
mod schema_contract_v78;
mod schema_contract_v79;
mod schema_contract_v80;
mod schema_contract_v81;
mod schema_contract_v84;
mod schema_contract_v85;
mod schema_contract_v86;

mod schema_contract_v87;
mod schema_contract_v88;
