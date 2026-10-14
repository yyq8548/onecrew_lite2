use super::*;
use onecrew_local_runtime::ToolkitId;

#[test]
fn connection_list_projection_is_toolkit_tagged_bounded_and_secret_free() {
    use crate::operation_registry::descriptor::{
        ConnectionListInput, ConnectionListResult, MAX_CONNECTIONS_PER_TOOLKIT,
    };
    use onecrew_local_runtime::{
        LocalConnectionAvailability, PlatformConnectionInventory, XConnectionSummary,
    };

    let inventory = PlatformConnectionInventory {
        x: (0..=MAX_CONNECTIONS_PER_TOOLKIT)
            .map(|index| XConnectionSummary {
                operator_account_id: format!("x-{index:02}"),
                username: format!("user-{index:02}"),
                label: format!("label-{index:02}"),
                availability: LocalConnectionAvailability::Available,
            })
            .collect(),
        instagram: Vec::new(),
        telegram: Vec::new(),
        youtube: Vec::new(),
        mailboxes: Vec::new(),
    };
    let result = ConnectionListResult::project(
        inventory,
        &ConnectionListInput {
            toolkits: vec![ToolkitId::X],
        },
    );
    assert_eq!(result.x.len(), MAX_CONNECTIONS_PER_TOOLKIT);
    assert!(result.truncated);
    assert!(result.instagram.is_empty());
    assert!(matches!(
        result.x[0].connection,
        ConnectionRef::OperatorAccount {
            toolkit: OperatorToolkitId::X,
            ..
        }
    ));
    let encoded = serde_json::to_vec(&result).unwrap();
    assert!(encoded.len() < 16 * 1024);
    let text = String::from_utf8(encoded).unwrap();
    for forbidden in ["api_key", "secret", "token", "password"] {
        assert!(
            !text.contains(forbidden),
            "connection list leaked {forbidden}"
        );
    }
}
