//! The platform-independent parts of the shared read orchestration.
use onecrew_x::{TimelinePage, User};
use serde_json::Value;

use super::{PlatformReadError, platform_native_data};
use crate::{PlatformConnectionResolutionError, SearchPlanApprovalError};

#[test]
fn platform_native_data_is_the_serde_serialization_of_the_platform_model() {
    let user = User {
        id: "9007199254740995".into(),
        name: "Creator".into(),
        username: "creator".into(),
        followers: 42,
        can_dm: Some(true),
        ..Default::default()
    };
    let page = TimelinePage {
        items: Vec::new(),
        next_cursor: Some("provider+/cursor==".into()),
    };
    assert_eq!(
        Value::Object(platform_native_data(&user).unwrap()),
        serde_json::to_value(&user).unwrap()
    );
    assert_eq!(
        Value::Object(platform_native_data(&page).unwrap()),
        serde_json::to_value(&page).unwrap()
    );
}

#[test]
fn a_model_that_is_not_a_json_object_is_reported_as_such() {
    struct Unserializable;
    impl serde::Serialize for Unserializable {
        fn serialize<S: serde::Serializer>(&self, _serializer: S) -> Result<S::Ok, S::Error> {
            Err(serde::ser::Error::custom("provider body SENTINEL"))
        }
    }
    for failure in [
        platform_native_data(&vec![User::default()]).unwrap_err(),
        platform_native_data(&"text").unwrap_err(),
        platform_native_data(&Value::Null).unwrap_err(),
        platform_native_data(&Unserializable).unwrap_err(),
    ] {
        assert_eq!(failure, PlatformReadError::NativeDataNotObject);
        assert!(!format!("{failure:?}{failure}").contains("SENTINEL"));
    }
}

#[test]
fn local_outcomes_are_explained_without_naming_a_platform() {
    let mut local = vec![
        PlatformReadError::SessionUnavailable,
        PlatformReadError::CredentialUnavailable,
        PlatformReadError::CredentialHealthCommitUnknown,
        PlatformReadError::Interrupted,
        PlatformReadError::NativeDataNotObject,
    ];
    local.extend(SearchPlanApprovalError::ALL.map(PlatformReadError::SearchPlanRefused));
    local.extend(
        [
            PlatformConnectionResolutionError::InvalidIdentifier,
            PlatformConnectionResolutionError::NotFoundOrMismatch,
            PlatformConnectionResolutionError::Unavailable,
            PlatformConnectionResolutionError::Internal,
        ]
        .map(PlatformReadError::Connection),
    );
    for error in local {
        let message = error.safe_message();
        assert_eq!(error.to_string(), message);
        for platform in ["X ", "Instagram", "Telegram", "YouTube"] {
            assert!(!message.contains(platform), "{error:?}: {message}");
        }
    }
    assert_eq!(
        PlatformReadError::InputInvalid("X handle is required").safe_message(),
        "X handle is required"
    );
    assert_eq!(
        PlatformReadError::Connection(PlatformConnectionResolutionError::Unavailable)
            .safe_message(),
        "connection is unavailable"
    );
}
