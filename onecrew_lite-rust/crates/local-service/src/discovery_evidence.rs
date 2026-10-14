//! Explicit evidence collection reuses the approved Creator claims and read budget.
use crate::ServiceState;
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub(crate) struct CollectEvidenceInput {
    campaign_id: String,
    expected_revision: u32,
    search_hit_id: String,
    profile_ref: String,
    kind: EvidenceKind,
}
#[derive(Deserialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
enum EvidenceKind {
    Transcript,
    Profile,
    Content,
}
#[derive(Serialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub(crate) struct EvidenceCollection {
    campaign_id: String,
    search_hit_id: String,
    status: String,
}

pub(crate) async fn collect(
    state: &ServiceState,
    input: CollectEvidenceInput,
) -> Result<EvidenceCollection, String> {
    if state
        .discovery_execution
        .snapshot(
            &state
                .validate_login()
                .await?
                .status
                .credential_id
                .ok_or("evidence_login_required")?,
        )
        .is_some_and(|s| {
            matches!(
                s.stage,
                crate::discovery_workspace::WorkspaceStage::Searching
                    | crate::discovery_workspace::WorkspaceStage::Verifying
                    | crate::discovery_workspace::WorkspaceStage::Pausing
            )
        })
    {
        return Err("evidence_execution_busy".into());
    }
    let platform = state.platform().await?;
    let campaign = platform
        .creators()?
        .get_discovery_campaign_async(input.campaign_id.clone())
        .await
        .map_err(|_| "evidence_campaign_unavailable")?;
    if campaign.phase != "active"
        || campaign.revision != input.expected_revision
        || !campaign
            .scopes
            .iter()
            .any(|s| s.profile_ref == input.profile_ref)
    {
        return Err("evidence_approval_changed".into());
    }
    let status = match input.kind {
        EvidenceKind::Transcript => platform
            .read_discovery_youtube_transcript_once(
                &input.campaign_id,
                input.expected_revision,
                &input.profile_ref,
                &input.search_hit_id,
            )
            .await
            .map(|r| r.status),
        EvidenceKind::Profile => platform
            .read_discovery_profile_once(
                &input.campaign_id,
                input.expected_revision,
                &input.search_hit_id,
            )
            .await
            .map(|r| r.status),
        EvidenceKind::Content => platform
            .read_discovery_content_once(
                &input.campaign_id,
                input.expected_revision,
                &input.search_hit_id,
            )
            .await
            .map(|r| r.status),
    }
    .map_err(|_| "evidence_collection_unavailable")?;
    Ok(EvidenceCollection {
        campaign_id: input.campaign_id,
        search_hit_id: input.search_hit_id,
        status,
    })
}
