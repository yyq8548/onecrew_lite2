//! Connection rejection is exercised through real bindings and an empty store.
//! Provider failures are covered by the frozen presentations beside the
//! platform read bindings, through the shared read-failure projection or a
//! remaining toolkit classifier; no test owner reconstructs the removed
//! Creator taxonomy.
use super::*;

#[test]
fn all_six_read_bindings_preserve_connection_failure_phase_effect_and_recovery() {
    let (_directory, _platform, registry) = production_registry();
    let mut calls = search_calls();
    for (_, _, arguments) in &mut calls {
        arguments["search_plan_policy"] = json!("user_requested_direct");
    }
    calls.push((
        "x.post_thread.get",
        ConnectionRef::OperatorAccount {
            toolkit: OperatorToolkitId::X,
            operator_account_id: UNKNOWN_CONNECTION.into(),
        },
        json!({"content_id":"12345"}),
    ));
    for (operation, connection, arguments) in calls {
        for (invalid_id, expected_code) in [
            ("invalid-uuid", "connection_id_invalid"),
            (UNKNOWN_CONNECTION, "connection_not_found_or_mismatch"),
        ] {
            let mut selected = connection.clone();
            match &mut selected {
                ConnectionRef::OperatorAccount {
                    operator_account_id,
                    ..
                } => *operator_account_id = invalid_id.into(),
                ConnectionRef::ServiceCredential { credential_id, .. } => {
                    *credential_id = invalid_id.into()
                }
                ConnectionRef::Mailbox { .. } => unreachable!(),
            }
            let failure =
                execute_registered(&registry, operation, &selected, arguments.clone()).unwrap_err();
            let projected = project_execute_failure(&failure);
            assert_eq!(
                projected.code.as_str(),
                expected_code,
                "{operation}/{invalid_id}"
            );
            assert_eq!(
                projected.phase,
                ExecuteFailurePhase::AfterInvocation,
                "{operation}"
            );
            assert_eq!(
                projected.effect,
                ExecuteEffectState::KnownNoEffect,
                "{operation}"
            );
            assert_eq!(
                projected.recovery,
                Some(RecoveryDirective::CorrectInput),
                "{operation}"
            );
            assert!(!projected.retry_safe, "{operation}");
            assert!(!failure.effect_may_have_occurred, "{operation}");
            assert_eq!(projected.provider_status, None, "{operation}");
            let wire = serde_json::to_string(&projected).unwrap();
            for absent in ["provider_status", "cookie=", "proxy=", "token=", invalid_id] {
                assert!(!wire.contains(absent), "{operation}: {wire}");
            }
        }
    }
}

#[test]
fn wrong_toolkit_connection_is_rejected_before_any_binding_runs() {
    let (_directory, _platform, registry) = production_registry();
    for (operation, connection, mut arguments) in search_calls() {
        arguments["search_plan_policy"] = json!("user_requested_direct");
        let wrong = ConnectionRef::OperatorAccount {
            toolkit: if matches!(
                connection,
                ConnectionRef::OperatorAccount {
                    toolkit: OperatorToolkitId::X,
                    ..
                }
            ) {
                OperatorToolkitId::Instagram
            } else {
                OperatorToolkitId::X
            },
            operator_account_id: UNKNOWN_CONNECTION.into(),
        };
        let failure = execute_registered(&registry, operation, &wrong, arguments).unwrap_err();
        assert_eq!(
            failure.kind,
            ExecuteFailureKind::ConnectionMismatch,
            "{operation}"
        );
        assert_eq!(
            failure.phase,
            ExecuteFailurePhase::BeforeInvocation,
            "{operation}"
        );
        let projected = project_execute_failure(&failure);
        assert_eq!(
            projected.effect,
            ExecuteEffectState::NotInvoked,
            "{operation}"
        );
        assert!(!failure.effect_may_have_occurred, "{operation}");
    }
}

#[test]
fn every_search_connection_check_is_visible_on_the_searches_surfaces() {
    let (_directory, _platform, registry) = production_registry();
    for (operation, _, _) in search_calls() {
        let toolkit = operation.split('.').next().unwrap();
        let check = EntryId::parse(format!("{toolkit}.connection.check")).unwrap();
        for surface in registry_operation(&registry, operation)
            .metadata
            .surfaces
            .iter()
        {
            assert!(
                registry.entry(&check, surface).is_some(),
                "{operation}: {check:?} on {surface:?}"
            );
        }
    }
}
