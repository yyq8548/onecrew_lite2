use rusqlite::{OptionalExtension, Transaction, params};

use crate::VaultError;

// v2 -> v3 (Desktop app 0.5.11): the provider codec expands while every
// existing row and the frozen v2 table definition remain unchanged.
pub(in crate::store) fn apply_v2_to_v3(_transaction: &Transaction<'_>) -> Result<(), VaultError> {
    Ok(())
}

// v3 -> v4 (Desktop app 0.5.15): old Resend service credentials become the
// canonical typed Email secret kind. UUIDs, encrypted values, metadata and
// health fields remain byte-for-byte unchanged.
pub(in crate::store) fn apply_v3_to_v4(transaction: &Transaction<'_>) -> Result<(), VaultError> {
    let conflict = transaction
        .query_row(
            "SELECT legacy.label
             FROM service_credentials legacy
             JOIN service_credentials typed
               ON typed.provider = 'email_resend_api_key'
              AND typed.label = legacy.label
             WHERE legacy.provider = 'resend'
             ORDER BY legacy.label
             LIMIT 1",
            [],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(super::database_error)?;
    if conflict.is_some() {
        return Err(VaultError::Serialization(
            "Resend credential identity is ambiguous during typed Email upgrade".into(),
        ));
    }

    let mut statement = transaction
        .prepare(
            "SELECT metadata_json FROM service_credentials
             WHERE provider = 'resend' ORDER BY id",
        )
        .map_err(super::database_error)?;
    let metadata = statement
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(super::database_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(super::database_error)?;
    drop(statement);
    for raw in metadata {
        let value = serde_json::from_str(&raw)
            .map_err(|error| VaultError::Serialization(error.to_string()))?;
        super::super::email_secrets::validate_metadata(value)?;
    }

    transaction
        .execute(
            "UPDATE service_credentials
             SET provider = 'email_resend_api_key'
             WHERE provider = ?1",
            params!["resend"],
        )
        .map(|_| ())
        .map_err(super::database_error)
}

pub(in crate::store) fn apply_forward_migrations(
    transaction: &Transaction<'_>,
    from_version: i64,
    to_version: i64,
) -> Result<(), VaultError> {
    let mut version = from_version;
    while version < to_version {
        match version {
            2 => apply_v2_to_v3(transaction)?,
            3 => apply_v3_to_v4(transaction)?,
            _ => {
                return Err(VaultError::Database(format!(
                    "no local-vault migration registered from version {version}"
                )));
            }
        }
        version += 1;
    }
    if version != to_version {
        return Err(VaultError::Database(format!(
            "local-vault migration registry did not reach requested version {to_version}"
        )));
    }
    Ok(())
}
