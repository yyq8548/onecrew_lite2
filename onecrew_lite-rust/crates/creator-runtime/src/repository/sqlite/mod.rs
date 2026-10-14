mod account_lifecycle;
mod contact_confirmations;
mod contact_route_deletion;
mod contacts;
mod content_interactions;
mod conversations;
mod creator_import;
mod creator_save;
mod creator_transfer;
mod creators;
mod discovery;
mod discovery_query_rounds;
mod discovery_contact_lookup;
mod discovery_contacts;
mod discovery_content;
mod discovery_douyin;
mod discovery_facebook;
mod discovery_facebook_identity;
mod discovery_identity;
mod discovery_identity_groups;
mod discovery_identity_review;
mod discovery_language;
mod discovery_library;
mod discovery_manual_approval;
mod discovery_profiles;
mod discovery_public_pages;
mod discovery_read_health;
mod discovery_review;
mod discovery_transcripts;
mod discovery_web_sources;
mod emails;
mod gmail_bulk;
mod idempotency;
mod identity;
mod mail_state;
mod outreach_actions;
mod relationships;
pub(crate) mod reply_triage;
mod schema;
pub use schema::CURRENT_SCHEMA_VERSION;
mod targets;

use std::{
    path::Path,
    sync::{Arc, Mutex, MutexGuard},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

#[cfg(any(test, feature = "test-support"))]
use std::sync::atomic::{AtomicU64, Ordering};

use r2d2::PooledConnection;
use r2d2_sqlite::SqliteConnectionManager;
use rusqlite::{Connection, OpenFlags};

use crate::{CreatorDatabaseStatus, CreatorDatabaseUpgradeError, CreatorDatabaseUpgradeResult};

const SQLITE_BUSY_TIMEOUT: Duration = Duration::from_secs(5);
pub(crate) const SQLITE_READ_POOL_SIZE: u32 = 2;
const SQLITE_READ_POOL_WAIT: Duration = Duration::from_millis(250);

type SqliteReadConnection = PooledConnection<SqliteConnectionManager>;

#[derive(Clone)]
pub(crate) struct SqliteRepository {
    connection: Arc<Mutex<Connection>>,
    read_pool: r2d2::Pool<SqliteConnectionManager>,
    /// Test-only evidence of successful acquisitions of the exclusive writer
    /// connection owner. This counts leases, not SQLite transactions, commits,
    /// changed rows, or any other database effect.
    #[cfg(any(test, feature = "test-support"))]
    writer_lease_acquisition_counter: Arc<AtomicU64>,
}

impl SqliteRepository {
    pub(crate) fn open(path: impl AsRef<Path>) -> Result<Self, String> {
        if let Some(parent) = path.as_ref().parent() {
            std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        let path = path.as_ref();
        let connection = Connection::open(path).map_err(|error| error.to_string())?;
        connection
            .busy_timeout(SQLITE_BUSY_TIMEOUT)
            .map_err(|error| error.to_string())?;
        schema::initialize(&connection)?;
        // Read-only connections are created only after inspect/upgrade has
        // left the database at the exact current schema. r2d2 owns checkout;
        // SQL executes after checkout and never under the pool's management
        // lock.
        let manager = SqliteConnectionManager::file(path)
            .with_flags(
                OpenFlags::SQLITE_OPEN_READ_ONLY
                    | OpenFlags::SQLITE_OPEN_NO_MUTEX
                    | OpenFlags::SQLITE_OPEN_URI,
            )
            .with_init(|connection| {
                connection.busy_timeout(SQLITE_BUSY_TIMEOUT)?;
                connection.execute_batch("PRAGMA foreign_keys=ON; PRAGMA query_only=ON;")
            });
        let read_pool = r2d2::Pool::builder()
            .max_size(SQLITE_READ_POOL_SIZE)
            .min_idle(Some(SQLITE_READ_POOL_SIZE))
            .connection_timeout(SQLITE_READ_POOL_WAIT)
            .build(manager)
            .map_err(|error| format!("Creator read pool is unavailable: {error}"))?;
        Ok(Self {
            connection: Arc::new(Mutex::new(connection)),
            read_pool,
            #[cfg(any(test, feature = "test-support"))]
            writer_lease_acquisition_counter: Arc::new(AtomicU64::new(0)),
        })
    }

    fn lock(&self) -> Result<MutexGuard<'_, Connection>, String> {
        let owner = self
            .connection
            .lock()
            .map_err(|_| "Creator repository lock poisoned".to_string())?;
        #[cfg(any(test, feature = "test-support"))]
        self.writer_lease_acquisition_counter
            .fetch_add(1, Ordering::Relaxed);
        Ok(owner)
    }

    fn read_connection(&self) -> Result<SqliteReadConnection, String> {
        self.read_pool
            .get()
            .map_err(|error| format!("Creator read capacity is unavailable: {error}"))
    }

    #[cfg(test)]
    pub(crate) fn strip_reddit_reference(connection: &Connection) -> Result<(), String> {
        schema::strip_reddit_reference(connection)
    }

    #[cfg(test)]
    fn read_pool(&self) -> &r2d2::Pool<SqliteConnectionManager> {
        &self.read_pool
    }

    #[cfg(feature = "test-support")]
    pub(crate) fn writer_lease_acquisition_counter(&self) -> Arc<AtomicU64> {
        Arc::clone(&self.writer_lease_acquisition_counter)
    }
}

pub(super) fn inspect_database(path: &Path) -> CreatorDatabaseStatus {
    schema::inspect(path)
}

pub(super) fn upgrade_database(
    path: &Path,
) -> Result<CreatorDatabaseUpgradeResult, CreatorDatabaseUpgradeError> {
    schema::upgrade(path)
}

#[cfg(feature = "test-support")]
pub(super) fn downgrade_database_to_v27_for_test(path: &Path) -> Result<(), String> {
    let connection = Connection::open(path).map_err(|error| error.to_string())?;
    schema::downgrade_reference_to_v27(&connection)
}

#[cfg(feature = "test-support")]
pub(super) fn downgrade_database_to_v28_for_test(path: &Path) -> Result<(), String> {
    let connection = Connection::open(path).map_err(|error| error.to_string())?;
    schema::downgrade_reference_to_v28(&connection)
}

#[cfg(feature = "test-support")]
pub(super) fn downgrade_database_to_v29_for_test(path: &Path) -> Result<(), String> {
    let connection = Connection::open(path).map_err(|error| error.to_string())?;
    schema::downgrade_reference_to_v29(&connection)
}

#[cfg(feature = "test-support")]
pub(super) fn downgrade_database_to_v31_for_test(path: &Path) -> Result<(), String> {
    let connection = Connection::open(path).map_err(|error| error.to_string())?;
    schema::downgrade_reference_to_v31(&connection)
}

#[cfg(feature = "test-support")]
pub(super) fn downgrade_database_to_v35_for_test(path: &Path) -> Result<(), String> {
    let connection = Connection::open(path).map_err(|error| error.to_string())?;
    schema::downgrade_reference_to_v35(&connection)
}

/// Structure-only predecessor fixture reconstruction; never a production downgrade.
#[cfg(feature = "test-support")]
pub(super) fn downgrade_database_to_v36_for_test(path: &Path) -> Result<(), String> {
    let connection = Connection::open(path).map_err(|error| error.to_string())?;
    schema::downgrade_reference_to_v36(&connection)
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn json(value: &impl serde::Serialize) -> Result<String, String> {
    serde_json::to_string(value).map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests;

use crate::{Page, decode_page_cursor, encode_page_cursor};

/// Decode a `"{updated_at}|{id}"` keyset position from an opaque cursor
/// bound to `context`.
/// Decode a cursor into its ordered numeric segments plus the trailing id.
///
/// `numeric_arity` is how many segments the caller's ORDER BY carries before
/// the id. It is checked rather than assumed: a cursor minted under a
/// different sort key decodes into the wrong number of parts, and silently
/// accepting it would page through a list using a position that means
/// something else — rows repeat or vanish at the boundary. A mismatch is the
/// same typed rejection as a corrupt cursor, which sends the caller back to
/// the first page.
pub(super) fn decode_keyset_position(
    cursor: &str,
    context: &str,
    numeric_arity: usize,
) -> Result<(Vec<u64>, String), String> {
    let invalid = || "page cursor is not valid for this query".to_string();
    let position = decode_page_cursor(cursor, context)?;
    let mut parts = position.split('|').collect::<Vec<_>>();
    if parts.len() != numeric_arity + 1 {
        return Err(invalid());
    }
    let id = parts.pop().ok_or_else(invalid)?.to_string();
    let numbers = parts
        .into_iter()
        .map(|part| part.parse::<u64>().map_err(|_| invalid()))
        .collect::<Result<Vec<_>, _>>()?;
    Ok((numbers, id))
}

/// The single-numeric-segment case, which is every list ordered by one
/// timestamp and an id tiebreak. Sugar over `decode_keyset_position`, not a
/// second decoder: the arity check still runs, so a cursor minted under a
/// wider sort key is rejected here too.
pub(super) fn decode_keyset_single(cursor: &str, context: &str) -> Result<(u64, String), String> {
    let (numbers, id) = decode_keyset_position(cursor, context, 1)?;
    Ok((numbers[0], id))
}

/// Turn up-to-`limit + 1` fetched rows into one bounded page, encoding the
/// continuation from the last returned row's keyset position.
pub(super) fn keyset_page<T>(
    mut rows: Vec<T>,
    limit: usize,
    context: &str,
    position: impl Fn(&T) -> (Vec<u64>, &str),
) -> Page<T> {
    let has_more = rows.len() > limit;
    rows.truncate(limit);
    let next_cursor = if has_more {
        rows.last().map(|last| {
            let (numbers, id) = position(last);
            let mut payload = String::new();
            for number in numbers {
                payload.push_str(&number.to_string());
                payload.push('|');
            }
            payload.push_str(id);
            encode_page_cursor(context, &payload)
        })
    } else {
        None
    };
    Page::new(rows, next_cursor)
}
