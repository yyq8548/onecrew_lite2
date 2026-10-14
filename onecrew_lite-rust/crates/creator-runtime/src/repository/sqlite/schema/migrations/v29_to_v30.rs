use rusqlite::Connection;

// v29 -> v30 (Desktop app 0.5.13) makes the already-validated one-owner
// contact-locator rule a native SQLite uniqueness invariant. Existing v29
// validation rejects shared locator owners before this additive edge runs, so
// every business row and stable id is preserved and the migration impact is
// empty.
pub(super) fn migrate(connection: &Connection) -> Result<(), String> {
    connection
        .execute(
            "CREATE UNIQUE INDEX creator_contact_routes_one_per_locator
               ON creator_contact_routes(locator_id)",
            [],
        )
        .map_err(|error| error.to_string())?;
    Ok(())
}
