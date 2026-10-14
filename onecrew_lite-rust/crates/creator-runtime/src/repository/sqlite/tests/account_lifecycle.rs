use super::*;
use crate::AccountLifecycleClaimError;

/// Disconnecting an operator account deactivates only that reference: the
/// recovery state carries nothing else, the claim is scoped to one account,
/// and the historical reference and audit rows survive.
#[test]
fn operator_disconnect_is_scoped_recoverable_and_preserves_history() {
    let (_directory, repository) = repository();
    let account = repository
        .upsert_account_ref(
            AccountRefId::new("operator-disconnect"),
            Platform::X,
            "profile",
            "@operator",
        )
        .unwrap();
    let other = repository
        .upsert_account_ref(
            AccountRefId::new("operator-other"),
            Platform::X,
            "profile",
            "@other",
        )
        .unwrap();
    let origin = CreatorCandidateOrigin::PlatformSearch {
        discovery_id: DiscoveryId::new("discovery-before-disconnect"),
        account_ref_id: account.id.clone(),
        query: "crypto".into(),
        search_kind: "users".into(),
    };
    let saved = persist_entry(
        &repository,
        save_input(
            entry(origin, "x-disconnect-creator", "disconnect_creator"),
            None,
        ),
    )
    .unwrap();

    let recovery = match repository
        .begin_operator_account_disconnect("disconnect-key", &account.id)
        .unwrap()
    {
        OperatorAccountDisconnectClaim::Acquired(recovery) => recovery,
        claim => panic!("unexpected claim: {claim:?}"),
    };
    assert_eq!(recovery.account_ref_id, account.id);
    let inactive = repository.get_account_ref(&account.id).unwrap();
    assert!(!inactive.active);
    assert_eq!(inactive.availability, AccountAvailability::Available);
    assert!(
        repository
            .require_operable_account_ref(&account.id)
            .is_err()
    );
    assert_eq!(
        repository
            .pending_operator_account_disconnects()
            .unwrap()
            .len(),
        1
    );
    let mismatch = repository
        .begin_operator_account_disconnect("disconnect-key", &other.id)
        .unwrap_err();
    assert_eq!(mismatch, AccountLifecycleClaimError::IdempotencyConflict);

    repository
        .mark_operator_account_disconnect_remote_started("disconnect-key", &account.id)
        .unwrap();
    repository
        .mark_operator_account_disconnect_remote_verified("disconnect-key", &account.id)
        .unwrap();
    let result = repository
        .complete_operator_account_disconnect("disconnect-key", &account.id)
        .unwrap();
    assert!(!result.account_ref.active);
    assert_eq!(
        result.account_ref.availability,
        AccountAvailability::Unavailable
    );
    assert!(!result.replayed);
    let replay = repository
        .complete_operator_account_disconnect("disconnect-key", &account.id)
        .unwrap();
    assert!(replay.replayed);
    assert!(
        repository
            .pending_operator_account_disconnects()
            .unwrap()
            .is_empty()
    );

    // The Creator discovered through this account keeps its provenance.
    let creator = repository.get_creator(&saved.creator.creator.id).unwrap();
    assert!(matches!(
        &creator.origins[0],
        CreatorOrigin::PlatformSearch { account_ref_id, .. } if account_ref_id == &account.id
    ));

    let connection = repository.lock().unwrap();
    let audit_count: u64 = connection
        .query_row(
            "SELECT COUNT(*) FROM audit_events WHERE account_ref_id = ?1",
            [account.id.as_str()],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(audit_count, 2);
    let completed_payload: serde_json::Value = connection
        .query_row(
            "SELECT payload_json FROM audit_events
             WHERE account_ref_id = ?1 AND kind = 'operator_account_disconnected'",
            [account.id.as_str()],
            |row| row.get::<_, String>(0),
        )
        .map(|payload| serde_json::from_str(&payload).unwrap())
        .unwrap();
    assert_eq!(
        completed_payload,
        serde_json::json!({
            "account_ref_id": account.id.as_str(),
            "vault_profile_deleted": true,
        })
    );
    let account_count: u64 = connection
        .query_row(
            "SELECT COUNT(*) FROM account_refs WHERE id = ?1",
            [account.id.as_str()],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(account_count, 1, "historical account ref must remain");
}

#[test]
fn operator_disconnect_rejects_service_credentials_without_mutation() {
    let (_directory, repository) = repository();
    let credential = repository
        .upsert_account_ref_with_state(
            AccountRefId::new("youtube-service"),
            Platform::YouTube,
            AccountKind::ServiceCredential,
            "YouTube API",
            AccountAvailability::Available,
        )
        .unwrap();
    let error = repository
        .begin_operator_account_disconnect("service-delete", &credential.id)
        .unwrap_err();
    assert_eq!(error, AccountLifecycleClaimError::WrongKind);
    let unchanged = repository.get_account_ref(&credential.id).unwrap();
    assert!(unchanged.active);
    assert_eq!(unchanged.availability, AccountAvailability::Available);
    assert!(
        repository
            .pending_operator_account_disconnects()
            .unwrap()
            .is_empty()
    );
}

/// Deleting a service credential deactivates only that reference and keeps
/// the historical reference and audit rows; no local record is unbound.
#[test]
fn service_credential_delete_is_recoverable_and_preserves_history() {
    let (_directory, repository) = repository();
    let credential = repository
        .upsert_account_ref_with_state(
            AccountRefId::new("youtube-service-delete"),
            Platform::YouTube,
            AccountKind::ServiceCredential,
            "YouTube API",
            AccountAvailability::Available,
        )
        .unwrap();

    let recovery = match repository
        .begin_service_credential_delete("delete-youtube-key", &credential.id)
        .unwrap()
    {
        ServiceCredentialDeleteClaim::Acquired(recovery) => recovery,
        claim => panic!("unexpected claim: {claim:?}"),
    };
    assert_eq!(recovery.account_ref_id, credential.id);
    assert!(!repository.get_account_ref(&credential.id).unwrap().active);
    assert_eq!(
        repository
            .pending_service_credential_deletes()
            .unwrap()
            .len(),
        1
    );

    repository
        .mark_service_credential_delete_remote_verified("delete-youtube-key", &credential.id)
        .unwrap();
    let result = repository
        .complete_service_credential_delete("delete-youtube-key", &credential.id)
        .unwrap();
    assert!(!result.account_ref.active);
    assert_eq!(
        result.account_ref.availability,
        AccountAvailability::Unavailable
    );
    assert!(!result.replayed);
    assert!(
        repository
            .complete_service_credential_delete("delete-youtube-key", &credential.id)
            .unwrap()
            .replayed
    );

    let connection = repository.lock().unwrap();
    let audit_kinds = connection
        .prepare(
            "SELECT kind FROM audit_events WHERE account_ref_id = ?1 ORDER BY created_at, kind",
        )
        .unwrap()
        .query_map([credential.id.as_str()], |row| row.get::<_, String>(0))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(
        audit_kinds,
        vec![
            "service_credential_delete_started".to_string(),
            "service_credential_deleted".to_string(),
        ]
    );
    let historical_ref_count: u64 = connection
        .query_row(
            "SELECT COUNT(*) FROM account_refs WHERE id = ?1",
            [credential.id.as_str()],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(historical_ref_count, 1);
}
