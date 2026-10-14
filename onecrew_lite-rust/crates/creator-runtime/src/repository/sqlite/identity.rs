use std::collections::BTreeSet;

use rusqlite::{Connection, params};

use crate::{CreatorId, IdentityResolution, PlatformAccountCandidate};

pub(super) fn resolve_creator_identity(
    connection: &Connection,
    candidate: &PlatformAccountCandidate,
) -> Result<IdentityResolution, String> {
    if let (Some(external_id), Some(target_kind)) =
        (candidate.external_id.as_deref(), candidate.target_kind)
    {
        let external_id = external_id.trim();
        if !external_id.is_empty() {
            let exact = creator_ids(
                connection,
                "SELECT link.creator_id
                 FROM platform_accounts account
                 JOIN creator_account_links link ON link.platform_account_id = account.id
                 WHERE account.platform = ?1 AND account.target_kind = ?2
                   AND account.external_id = ?3 AND link.state = 'confirmed'",
                params![
                    candidate.platform.as_str(),
                    target_kind.as_str(),
                    external_id
                ],
            )?;
            return match exact.as_slice() {
                [] => resolve_handle_identity(connection, candidate),
                [creator_id] => Ok(IdentityResolution::ExactExistingCreator {
                    creator_id: creator_id.clone(),
                }),
                _ => Ok(IdentityResolution::ConflictingMatches { creator_ids: exact }),
            };
        }
    }
    resolve_handle_identity(connection, candidate)
}

fn resolve_handle_identity(
    connection: &Connection,
    candidate: &PlatformAccountCandidate,
) -> Result<IdentityResolution, String> {
    let Some(handle) = candidate
        .handle
        .as_deref()
        .map(normalize_handle)
        .filter(|value| !value.is_empty())
    else {
        return Ok(IdentityResolution::New);
    };
    let Some(target_kind) = candidate.target_kind else {
        return Ok(IdentityResolution::New);
    };
    let possible = creator_ids(
        connection,
        "SELECT link.creator_id
         FROM platform_account_observations observation
         JOIN platform_accounts account ON account.id = observation.platform_account_id
         JOIN creator_account_links link ON link.platform_account_id = account.id
         WHERE account.platform = ?1 AND account.target_kind = ?2
           AND lower(ltrim(observation.handle, '@')) = ?3
           AND link.state != 'rejected'",
        params![candidate.platform.as_str(), target_kind.as_str(), handle],
    )?;
    match possible.as_slice() {
        [] => Ok(IdentityResolution::New),
        _ if possible.len() == 1 => Ok(IdentityResolution::PossibleMatch {
            creator_ids: possible,
        }),
        _ => Ok(IdentityResolution::ConflictingMatches {
            creator_ids: possible,
        }),
    }
}

fn creator_ids<P>(connection: &Connection, sql: &str, params: P) -> Result<Vec<CreatorId>, String>
where
    P: rusqlite::Params,
{
    let mut statement = connection.prepare(sql).map_err(|error| error.to_string())?;
    let ids = statement
        .query_map(params, |row| row.get::<_, String>(0))
        .map_err(|error| error.to_string())?
        .map(|row| row.map_err(|error| error.to_string()))
        .collect::<Result<BTreeSet<_>, _>>()?;
    Ok(ids.into_iter().map(CreatorId::new).collect())
}

fn normalize_handle(value: &str) -> String {
    value.trim().trim_start_matches('@').to_lowercase()
}
