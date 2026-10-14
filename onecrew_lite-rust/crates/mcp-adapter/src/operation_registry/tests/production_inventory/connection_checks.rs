use super::*;
use onecrew_local_runtime::ToolkitId;

use crate::operation_registry::{descriptor::MailboxToolkitId, execution::BoundedPublicValue};

fn production_registry() -> (
    tempfile::TempDir,
    crate::operation_registry::OperationRegistry,
) {
    let directory = tempfile::tempdir().unwrap();
    let platform = Arc::new(onecrew_local_runtime::LocalPlatform::open(directory.path()).unwrap());
    let registry = crate::operation_registry::build_registry(
        production_owners(platform),
        std::sync::Arc::new(
            onecrew_local_runtime::CapabilityCatalog::build().expect("production catalog"),
        ),
    )
    .unwrap();
    (directory, registry)
}

fn checks() -> [(&'static str, ToolkitId, EffectClass, ConnectionRequirement); 5] {
    [
        (
            "x.connection.check",
            ToolkitId::X,
            EffectClass::ReadOpenWorld,
            ConnectionRequirement::OperatorAccount {
                toolkit: OperatorToolkitId::X,
            },
        ),
        (
            "instagram.connection.check",
            ToolkitId::Instagram,
            EffectClass::ReadOpenWorld,
            ConnectionRequirement::OperatorAccount {
                toolkit: OperatorToolkitId::Instagram,
            },
        ),
        (
            "telegram.connection.check",
            ToolkitId::Telegram,
            EffectClass::ReadOpenWorld,
            ConnectionRequirement::OperatorAccount {
                toolkit: OperatorToolkitId::Telegram,
            },
        ),
        (
            "youtube.connection.check",
            ToolkitId::Youtube,
            EffectClass::SyncOpenWorld,
            ConnectionRequirement::ServiceCredential {
                toolkit: ServiceToolkitId::Youtube,
            },
        ),
        (
            "email.connection.check",
            ToolkitId::Email,
            EffectClass::SyncOpenWorld,
            ConnectionRequirement::Mailbox {
                toolkit: MailboxToolkitId::Email,
            },
        ),
    ]
}

#[test]
fn connection_checks_are_exact_unapproved_typed_and_secret_free() {
    let (_directory, registry) = production_registry();
    let directory = registry.toolkit_directory(SurfaceId::Full);
    for (id, toolkit, effect, connection) in checks() {
        let operation = registry_operation(&registry, id);
        assert_eq!(operation.metadata.toolkit, toolkit, "{id}");
        assert_eq!(operation.effect, effect, "{id}");
        assert_eq!(operation.approval, ApprovalClass::None, "{id}");
        assert!(operation.approval_request.is_none(), "{id}");
        assert_eq!(operation.secret, SecretClass::None, "{id}");
        assert!(operation.sensitive_input_paths.is_empty(), "{id}");
        assert_eq!(operation.connection, connection, "{id}");
        assert!(
            operation.metadata.summary.contains("network_unreachable"),
            "{id}"
        );
        for surface in [
            SurfaceId::Full,
            SurfaceId::Compact,
            SurfaceId::Accounts,
            SurfaceId::Mail,
        ] {
            assert!(
                operation.metadata.surfaces.allows(surface),
                "{id} {surface:?}"
            );
        }
        assert_eq!(
            operation.metadata.surfaces.allows(SurfaceId::Creators),
            toolkit != ToolkitId::Email,
            "{id} runs on Creators exactly when that surface's provider reads recover to it"
        );

        let input = operation.input.document();
        assert!(
            input["properties"]
                .as_object()
                .is_none_or(serde_json::Map::is_empty),
            "{id} takes no arguments"
        );
        let output = operation.output.document();
        let mut properties = output["properties"]
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect::<Vec<_>>();
        properties.sort_unstable();
        assert_eq!(
            properties,
            [
                "account_handle",
                "checked_at",
                "egress",
                "proxy_configured",
                "status"
            ],
            "{id}"
        );
        let output_text = serde_json::to_string(output).unwrap();
        for status in [
            "ok",
            "proxy_unreachable",
            "network_unreachable",
            "credential_rejected",
            "provider_unavailable",
        ] {
            assert!(output_text.contains(status), "{id} {status}");
        }
        for forbidden in [
            "password", "api_key", "token", "cookie", "endpoint", "secret",
        ] {
            assert!(!output_text.contains(forbidden), "{id} exposes {forbidden}");
        }

        let guide = directory
            .iter()
            .find(|guide| guide.toolkit == toolkit)
            .unwrap();
        assert!(
            guide.operations.iter().any(|operation| operation.id == id),
            "{id} is in its toolkit guide"
        );
        assert!(
            guide.gotchas.iter().any(|gotcha| gotcha.contains(id)),
            "{id} has a guide gotcha"
        );
    }
}

#[test]
fn every_provider_read_can_run_its_connection_check_on_each_of_its_surfaces() {
    let (_directory, registry) = production_registry();
    let mut reads = 0;
    for (id, surfaces) in registry.operation_ids_and_surfaces() {
        let operation = registry_operation(&registry, id.as_str());
        let check = match operation.metadata.toolkit {
            ToolkitId::X => "x.connection.check",
            ToolkitId::Instagram => "instagram.connection.check",
            ToolkitId::Telegram => "telegram.connection.check",
            ToolkitId::Youtube => "youtube.connection.check",
            _ => continue,
        };
        if operation.metadata.domain_id != "reads" {
            continue;
        }
        reads += 1;
        let check = EntryId::parse(check).unwrap();
        for surface in surfaces {
            assert!(
                registry.entry(&check, surface).is_some(),
                "{id} recovers to {check}, which is not executable on {surface:?}"
            );
        }
    }
    assert!(reads >= 10, "only {reads} provider reads were checked");
}

#[test]
fn connection_checks_fail_closed_on_unknown_or_mismatched_connections() {
    let (_directory, registry) = production_registry();
    let inventory = RecoveryDirective::ReadStatus {
        reference: BoundedPublicValue::parse("onecrew_connection_list").unwrap(),
    };
    for (id, connection) in [
        (
            "x.connection.check",
            ConnectionRef::OperatorAccount {
                toolkit: OperatorToolkitId::X,
                operator_account_id: "missing-operator".to_owned(),
            },
        ),
        (
            "telegram.connection.check",
            ConnectionRef::OperatorAccount {
                toolkit: OperatorToolkitId::Telegram,
                operator_account_id: "00000000-0000-4000-8000-000000000001".to_owned(),
            },
        ),
        (
            "youtube.connection.check",
            ConnectionRef::ServiceCredential {
                toolkit: ServiceToolkitId::Youtube,
                credential_id: "missing-credential".to_owned(),
            },
        ),
        (
            "email.connection.check",
            ConnectionRef::Mailbox {
                toolkit: MailboxToolkitId::Email,
                mailbox_ref_id: "missing-mailbox".to_owned(),
            },
        ),
    ] {
        let failure = execute_registered(&registry, id, &connection, json!({})).unwrap_err();
        let projected = project_execute_failure(&failure);
        assert_eq!(projected.code.as_str(), "connection_unavailable", "{id}");
        assert_eq!(projected.effect, ExecuteEffectState::KnownNoEffect, "{id}");
        assert_eq!(projected.recovery, Some(inventory.clone()), "{id}");
        assert!(
            !serde_json::to_string(&projected)
                .unwrap()
                .contains("missing-")
        );
    }

    let mismatch = execute_registered(
        &registry,
        "instagram.connection.check",
        &ConnectionRef::OperatorAccount {
            toolkit: OperatorToolkitId::X,
            operator_account_id: "x-operator".to_owned(),
        },
        json!({}),
    )
    .unwrap_err();
    assert_eq!(mismatch.kind, ExecuteFailureKind::ConnectionMismatch);
}
