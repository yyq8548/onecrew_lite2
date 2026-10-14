use onecrew_local_domain::{ServiceCredential, ServiceProvider};
use onecrew_local_vault::LocalVault;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::{LocalConnectionAvailability, shared};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct YoutubeCredentialSummary {
    pub credential_id: String,
    pub label: String,
    pub availability: LocalConnectionAvailability,
}

pub(super) fn summaries(
    vault: &LocalVault,
    credentials: Vec<ServiceCredential>,
) -> Vec<YoutubeCredentialSummary> {
    let mut summaries = credentials
        .into_iter()
        .filter(|credential| credential.provider == ServiceProvider::Youtube)
        .filter_map(|credential| {
            let credential_id = credential.id.to_string();
            (!credential_id.trim().is_empty()).then(|| YoutubeCredentialSummary {
                credential_id,
                label: credential.label.clone(),
                availability: shared::local_availability(
                    has_youtube_shape(&credential)
                        && shared::service_credential_is_locally_available(vault, &credential),
                ),
            })
        })
        .collect::<Vec<_>>();
    summaries.sort_by(|left, right| {
        left.label
            .cmp(&right.label)
            .then_with(|| left.credential_id.cmp(&right.credential_id))
    });
    summaries
}

pub(super) fn has_youtube_shape(credential: &ServiceCredential) -> bool {
    credential
        .metadata
        .get("api")
        .and_then(serde_json::Value::as_str)
        == Some("youtube_data_api_v3")
}
