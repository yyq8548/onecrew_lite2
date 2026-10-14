//! CRM v57 -> v58 (app 0.6.29): additive audited language and match findings.
//! SQL layout is unchanged. New JSON assertions are created only by explicit
//! reviews; existing criteria, evidence, receipts and budgets remain byte-identical.
use rusqlite::Connection;

pub(super) fn migrate(connection: &Connection) -> Result<(), String> {
    // Validate the persisted JSON containers before admitting the new encoding.
    let invalid: bool = connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM discovery_evidence_reviews
         WHERE NOT json_valid(findings_json) OR json_type(findings_json)!='array'
            OR NOT json_valid(material_json) OR json_type(material_json)!='object')",
            [],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())?;
    if invalid {
        return Err("review_encoding_invalid".into());
    }
    Ok(())
}
