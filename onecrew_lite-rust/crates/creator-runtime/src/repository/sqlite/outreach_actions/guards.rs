//! Deletion must not orphan the stable attribution of an unfinished effect.
//! Malformed new envelopes fail closed instead of becoming an unscoped claim.

use crate::{AccountRefId, CreatorContactRouteId, CreatorId};
use rusqlite::{Connection, params};

pub(in super::super) fn creator_pending(
    connection: &Connection,
    id: &CreatorId,
) -> Result<bool, String> {
    pending(connection, "$.state.identity.creator_id", id.as_str())
}
pub(in super::super) fn operator_pending(
    connection: &Connection,
    id: &AccountRefId,
) -> Result<bool, String> {
    pending(
        connection,
        "$.state.identity.operator_account_ref_id",
        id.as_str(),
    )
}
pub(in super::super) fn route_pending_count(
    connection: &Connection,
    id: &CreatorContactRouteId,
) -> Result<u64, String> {
    connection.query_row(
        "SELECT COUNT(*) FROM idempotency_records WHERE action IN ('relationship_update_v1','content_reply_send_v1','direct_message_send_v1')
         AND state IN ('in_progress','remote_started','remote_accepted','remote_verified')
         AND CASE WHEN json_valid(result_json) AND json_extract(result_json,'$.version') = 1
                       AND json_type(result_json,'$.state.identity.creator_id') = 'text'
                       AND json_type(result_json,'$.state.identity.operator_account_ref_id') = 'text'
                       AND length(trim(json_extract(result_json,'$.state.identity.creator_id'))) > 0
                       AND length(trim(json_extract(result_json,'$.state.identity.operator_account_ref_id'))) > 0
                       AND json_extract(result_json,'$.state.fingerprint') = request_fingerprint
                       AND request_fingerprint GLOB 'sha256:v1:*'
                  THEN contact_route_id IS NULL AND json_extract(result_json,'$.state.identity.contact_route_id') = ?1
                  ELSE 1 END",
        [id.as_str()], |row| row.get(0),
    ).map_err(|error| error.to_string())
}
fn pending(connection: &Connection, path: &str, id: &str) -> Result<bool, String> {
    connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM idempotency_records WHERE action IN ('relationship_update_v1','content_reply_send_v1','direct_message_send_v1')
         AND state IN ('in_progress','remote_started','remote_accepted','remote_verified')
         AND CASE WHEN json_valid(result_json) AND json_extract(result_json,'$.version') = 1
                       AND json_type(result_json,'$.state.identity.creator_id') = 'text'
                       AND json_type(result_json,'$.state.identity.operator_account_ref_id') = 'text'
                       AND length(trim(json_extract(result_json,'$.state.identity.creator_id'))) > 0
                       AND length(trim(json_extract(result_json,'$.state.identity.operator_account_ref_id'))) > 0
                       AND json_extract(result_json,'$.state.fingerprint') = request_fingerprint
                       AND request_fingerprint GLOB 'sha256:v1:*'
                  THEN json_extract(result_json, ?2) = ?1 ELSE 1 END)",
        params![id, path], |row| row.get(0),
    ).map_err(|error| error.to_string())
}
