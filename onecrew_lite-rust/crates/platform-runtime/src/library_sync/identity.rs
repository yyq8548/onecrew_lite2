//! Library wire projection of the Creator owner's versioned identity material.
use onecrew_creator_runtime::DiscoveryLibraryPublication;
use reqwest::Method;
use serde_json::{Value, json};

use super::{
    client::{LibraryClient, query},
    uncertain_write,
};

pub(super) fn payload(
    table: &str,
    publication: &DiscoveryLibraryPublication,
) -> Result<Value, String> {
    let sources = &publication.material.sources;
    let identity = &publication.material.identity;
    if table.is_empty()
        || sources.source_revision == 0
        || publication.identity_revision == 0
        || sources.campaign_id != identity.campaign_id
        || sources.criteria_revision != identity.criteria_revision
    {
        return Err("library_request_invalid".into());
    }
    let body = json!({
        "table_id": table, "campaign_id": sources.campaign_id, "projection_format": 1,
        "criteria_revision": sources.criteria_revision,
        "source_revision": sources.source_revision, "source_hash": sources.snapshot_hash,
        "identity_revision": publication.identity_revision, "identity_hash": identity.snapshot_hash,
        "source_rows": sources.rows.iter().map(|row| json!({
            "search_hit_id": row.hit.id, "candidate_id": row.candidate_id,
            "native_author_id": row.native_author_id, "evidence_version": row.evidence_version,
        })).collect::<Vec<_>>(),
        "groups": identity.groups.iter().map(|group| json!({
            "group_id": group.group_id, "has_same_author_review": group.has_same_author_review,
            "decision_ids": group.decision_ids,
            "members": group.members.iter().map(|member| json!({
                "candidate_id": member.candidate_id, "platform": member.platform,
                "native_author_id": member.native_author_id, "display_name": member.display_name,
                "profile_url": member.profile_url,
                "source_hit_ids": member.sources.iter().map(|row| &row.hit.id).collect::<Vec<_>>(),
            })).collect::<Vec<_>>(),
        })).collect::<Vec<_>>(),
    });
    if serde_json::to_vec(&body)
        .map_err(|_| "library_request_invalid")?
        .len()
        > 16 * 1024 * 1024
    {
        return Err("library_capacity_exceeded".into());
    }
    Ok(body)
}

impl LibraryClient {
    pub(super) async fn publish_identity(
        &self,
        table: &str,
        publication: &DiscoveryLibraryPublication,
    ) -> Result<(), String> {
        let body = payload(table, publication)?;
        let expected = json!({"state":"current", "projection":body});
        let result = self
            .request::<Value>(Method::POST, "/_local/discovery/identity", Some(body))
            .await;
        match result {
            Ok(receipt) if receipt != expected => return Err("library_readback_incomplete".into()),
            Err(error) if !uncertain_write(&error) => return Err(error),
            _ => {}
        }
        // Reconcile one uncertain response; do not repeat the POST. A changed or
        // missing projection never counts as a successful handoff.
        let readback: Value = self
            .request(
                Method::GET,
                &format!(
                    "/_local/discovery/identity?table_id={}&campaign_id={}",
                    query(table),
                    query(&publication.material.sources.campaign_id),
                ),
                None,
            )
            .await?;
        if readback != expected {
            return Err("library_readback_incomplete".into());
        }
        Ok(())
    }
}
