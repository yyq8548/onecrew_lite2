use std::collections::{BTreeMap, btree_map::Entry};

use rusqlite::{Transaction, TransactionBehavior};

use crate::{
    CREATOR_BUNDLE_MAX_ACCOUNTS_PER_CREATOR, CREATOR_BUNDLE_MAX_CREATORS, CreatorTransferAccount,
    CreatorTransferConfidence, CreatorTransferContactRoute, CreatorTransferCreator,
    CreatorTransferLinkState, CreatorTransferPlatform, CreatorTransferProfile,
    CreatorTransferRelationship, CreatorTransferStage, CreatorTransferTargetKind,
};

use super::{
    SqliteRepository,
    creators::{avatar_url_from_public_data, parse_follower_counts, to_sql_conversion_error},
};

pub(super) fn export_creator_bundle(
    repository: &SqliteRepository,
) -> Result<crate::CreatorBundle, String> {
    export_creator_bundle_with(repository, crate::creator_transfer::build_creator_bundle)
}

pub(super) fn export_creator_workbook(
    repository: &SqliteRepository,
) -> Result<crate::CreatorWorkbookExport, String> {
    crate::creator_transfer::build_creator_workbook_export(export_creator_workbook_snapshot(
        repository,
    )?)
}

pub(super) fn export_creator_workbook_snapshot(
    repository: &SqliteRepository,
) -> Result<Vec<(crate::CreatorTransferCreator, crate::CreatorWorkbookDisplay)>, String> {
    let rows = {
        let mut connection = repository.read_connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Deferred)
            .map_err(|error| error.to_string())?;
        let mut rows = load_transfer_rows_batch(&transaction, CREATOR_BUNDLE_MAX_CREATORS)?;
        let last_touches = load_last_touches_batch(&transaction)?;
        for (creator_id, _, display) in &mut rows {
            display.last_touch_at = last_touches.get(creator_id).copied();
        }
        transaction.commit().map_err(|error| error.to_string())?;
        rows
    };
    Ok(rows
        .into_iter()
        .map(|(_, creator, display)| (creator, display))
        .collect())
}

fn export_creator_bundle_with<F>(
    repository: &SqliteRepository,
    builder: F,
) -> Result<crate::CreatorBundle, String>
where
    F: FnOnce(Vec<CreatorTransferCreator>) -> Result<crate::CreatorBundle, String>,
{
    // One read lease covers exactly the SQL: a deferred transaction gives
    // every batch query the same committed snapshot, and the connection is
    // returned before the builder hashes and validates every Creator. The
    // CPU-bound half of the export therefore owns no SQLite capacity.
    let creators = {
        let mut connection = repository.read_connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Deferred)
            .map_err(|error| error.to_string())?;
        let creators = load_transfer_creators_batch(&transaction, CREATOR_BUNDLE_MAX_CREATORS)?;
        transaction.commit().map_err(|error| error.to_string())?;
        creators
    };
    builder(creators)
}

#[cfg(test)]
pub(super) fn export_creator_bundle_with_test_builder<F>(
    repository: &SqliteRepository,
    builder: F,
) -> Result<crate::CreatorBundle, String>
where
    F: FnOnce(Vec<CreatorTransferCreator>) -> Result<crate::CreatorBundle, String>,
{
    export_creator_bundle_with(repository, builder)
}

/// Loads every transferable Creator with a fixed number of set queries
/// (creators+relationships, account capture order, accounts, contact routes,
/// route observation sources) and groups the rows in memory by creator id.
///
/// The per-Creator detail loader ran a dozen statements per Creator, several
/// of them unindexed scans, so the statement count and the lock hold grew with
/// the store; the batch keeps both flat. Every query orders by an explicit
/// key plus a `rowid` tiebreak so equal sort keys cannot reorder between runs.
/// The observation-source tiebreak (`observed_at DESC, rowid DESC`) is not the
/// unspecified tie order the per-row query produced, so `semantic_sha256`
/// values for routes with equal `observed_at` observations may differ from the
/// pre-batch baseline; that one-time difference is accepted.
///
/// Returns an error when more than `limit` Creators exist, when a Creator has
/// no relationship row (the per-row loader failed the same way), or when one
/// Creator links more than the per-Creator account cap.
fn load_transfer_creators_batch(
    transaction: &Transaction<'_>,
    limit: usize,
) -> Result<Vec<CreatorTransferCreator>, String> {
    Ok(load_transfer_rows_batch(transaction, limit)?
        .into_iter()
        .map(|(_, creator, _)| creator)
        .collect())
}

fn load_transfer_rows_batch(
    transaction: &Transaction<'_>,
    limit: usize,
) -> Result<
    Vec<(
        String,
        CreatorTransferCreator,
        crate::CreatorWorkbookDisplay,
    )>,
    String,
> {
    let heads = load_transfer_heads(transaction, limit)?;
    if heads.len() > limit {
        return Err(format!(
            "creator bundle exceeds the maximum of {limit} Creators"
        ));
    }
    let account_capture_order = load_account_capture_order(transaction)?;
    let mut accounts = load_transfer_accounts(transaction, &account_capture_order)?;
    let mut routes = load_transfer_routes(transaction)?;
    let mut route_sources = load_transfer_route_sources(transaction)?;
    heads
        .into_iter()
        .map(|head| {
            let TransferHead {
                notes,
                creator_id,
                display_name,
                tags,
                score,
                follower_counts,
                pinned,
                stage,
                updated_at,
                group_name,
            } = head;
            let contact_routes = transfer_contact_routes(
                routes.remove(&creator_id).unwrap_or_default(),
                route_sources.remove(&creator_id).unwrap_or_default(),
            )?;
            Ok((
                creator_id.clone(),
                CreatorTransferCreator {
                    notes,
                    semantic_sha256: String::new(),
                    record_key: String::new(),
                    display_name,
                    tags,
                    score,
                    follower_counts,
                    pinned,
                    relationship: CreatorTransferRelationship { stage },
                    accounts: accounts.remove(&creator_id).unwrap_or_default(),
                    contact_routes,
                },
                crate::CreatorWorkbookDisplay {
                    last_touch_at: None,
                    updated_at,
                    group_name,
                },
            ))
        })
        .collect()
}

struct TransferHead {
    notes: Option<String>,
    creator_id: String,
    display_name: String,
    tags: Vec<String>,
    score: Option<u8>,
    follower_counts: BTreeMap<CreatorTransferPlatform, u64>,
    pinned: bool,
    stage: CreatorTransferStage,
    updated_at: u64,
    group_name: Option<String>,
}

/// One row per Creator in id order, up to `limit + 1` rows so the caller can
/// detect the cap without counting the table. The relationship is LEFT
/// JOINed rather than INNER JOINed on purpose: a Creator without a
/// relationship row must surface as an error, not silently vanish from the
/// export.
fn load_transfer_heads(
    transaction: &Transaction<'_>,
    limit: usize,
) -> Result<Vec<TransferHead>, String> {
    let mut statement = transaction
        .prepare(
            "SELECT creator.id, creator.display_name, creator.score,
                    creator.follower_counts_json, creator.pinned_at IS NOT NULL,
                    relationship.stage, creator.updated_at, creator_group.name, creator.tags_json,
                    (SELECT note.value FROM observation_sources source,
                          json_each(source.metadata_json, '$.source_notes') note
                     WHERE source.kind='creator_import'
                       AND note.key=creator.id
                     ORDER BY source.observed_at DESC, source.rowid DESC LIMIT 1)
             FROM creators creator
             LEFT JOIN creator_relationships relationship
               ON relationship.creator_id = creator.id
             LEFT JOIN creator_groups creator_group ON creator_group.id = creator.group_id
             ORDER BY creator.id ASC
             LIMIT ?1",
        )
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map([limit as i64 + 1], |row| {
            let follower_counts: String = row.get(3)?;
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Option<u8>>(2)?,
                parse_follower_counts(&follower_counts).map_err(to_sql_conversion_error)?,
                row.get::<_, bool>(4)?,
                row.get::<_, Option<String>>(5)?,
                row.get::<_, u64>(6)?,
                row.get::<_, Option<String>>(7)?,
                row.get::<_, String>(8)?,
                row.get::<_, Option<String>>(9)?,
            ))
        })
        .map_err(|error| error.to_string())?
        .map(|row| row.map_err(|error| error.to_string()))
        .collect::<Result<Vec<_>, _>>()?;
    rows.into_iter()
        .map(
            |(
                creator_id,
                display_name,
                score,
                follower_counts,
                pinned,
                stage,
                updated_at,
                group_name,
                tags_json,
                notes,
            )| {
                let stage =
                    stage.ok_or_else(|| format!("creator {creator_id} has no relationship row"))?;
                let stage = crate::CreatorStage::parse(&stage)?;
                Ok(TransferHead {
                    notes,
                    tags: serde_json::from_str(&tags_json).map_err(|error| error.to_string())?,
                    creator_id,
                    display_name,
                    score,
                    follower_counts: follower_counts
                        .into_iter()
                        .map(|(platform, count)| (CreatorTransferPlatform::from(platform), count))
                        .collect(),
                    pinned,
                    stage: CreatorTransferStage::from(stage),
                    updated_at,
                    group_name,
                })
            },
        )
        .collect()
}

/// Computes the Creator-list `last_touch_at` projection in three set queries.
/// This stays in the same deferred snapshot as the portable rows and never
/// turns the human export into a per-Creator detail read.
fn load_last_touches_batch(transaction: &Transaction<'_>) -> Result<BTreeMap<String, u64>, String> {
    let mut result = BTreeMap::new();
    merge_last_touch_rows(
        transaction,
        "SELECT creator_id, MAX(occurred_at) FROM touchpoints GROUP BY creator_id",
        &mut result,
    )?;
    merge_last_touch_rows(
        transaction,
        "SELECT thread.creator_id, MAX(email.occurred_at)
           FROM emails email
           JOIN email_threads thread ON thread.id = email.thread_id
          WHERE email.direction = 'outbound'
            AND thread.creator_id IS NOT NULL
          GROUP BY thread.creator_id",
        &mut result,
    )?;
    merge_last_touch_rows(
        transaction,
        "SELECT route.creator_id, MAX(message.sent_at)
           FROM messages message
           JOIN conversation_contact_routes link
             ON link.conversation_id = message.conversation_id
           JOIN creator_contact_routes route ON route.id = link.contact_route_id
          WHERE message.direction = 'outbound' AND message.status = 'sent'
          GROUP BY route.creator_id",
        &mut result,
    )?;
    Ok(result)
}

fn merge_last_touch_rows(
    transaction: &Transaction<'_>,
    sql: &str,
    output: &mut BTreeMap<String, u64>,
) -> Result<(), String> {
    let mut statement = transaction
        .prepare(sql)
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, Option<u64>>(1)?))
        })
        .map_err(|error| error.to_string())?;
    for row in rows {
        let (creator_id, timestamp) = row.map_err(|error| error.to_string())?;
        if let Some(timestamp) = timestamp {
            output
                .entry(creator_id)
                .and_modify(|current| *current = (*current).max(timestamp))
                .or_insert(timestamp);
        }
    }
    Ok(())
}

fn load_account_capture_order(
    connection: &rusqlite::Connection,
) -> Result<BTreeMap<String, u32>, String> {
    let mut statement = connection
        .prepare(
            "SELECT DISTINCT account.id, account.created_at, account.rowid
             FROM platform_accounts account
             JOIN creator_account_links link ON link.platform_account_id = account.id
             ORDER BY account.created_at ASC, account.rowid ASC",
        )
        .map_err(|error| error.to_string())?;
    statement
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(|error| error.to_string())?
        .enumerate()
        .map(|(index, row)| {
            Ok((
                row.map_err(|error| error.to_string())?,
                u32::try_from(index)
                    .map_err(|_| "Creator account capture order cannot be represented")?,
            ))
        })
        .collect()
}

/// Every linked account for every Creator in one pass, grouped by creator id.
/// The per-Creator account cap is enforced while streaming rows, so an
/// oversized Creator fails after its cap+1th row without materializing the
/// rest of its links.
fn load_transfer_accounts(
    transaction: &Transaction<'_>,
    account_capture_order: &BTreeMap<String, u32>,
) -> Result<BTreeMap<String, Vec<CreatorTransferAccount>>, String> {
    let mut statement = transaction
        .prepare(
            "SELECT link.creator_id, account.id, account.platform, account.target_kind,
                    account.external_id, link.state, link.confidence,
                    observation.handle, observation.display_name, observation.profile_url,
                    observation.public_json, source.kind
             FROM creator_account_links link
             JOIN platform_accounts account ON account.id = link.platform_account_id
             LEFT JOIN platform_account_observations observation ON observation.rowid = (
               SELECT latest.rowid FROM platform_account_observations latest
               WHERE latest.platform_account_id = account.id
               ORDER BY latest.observed_at DESC, latest.rowid DESC
               LIMIT 1
             )
             LEFT JOIN observation_sources source ON source.id = observation.source_id
             ORDER BY link.creator_id ASC, account.created_at ASC, account.rowid ASC,
                      link.id ASC",
        )
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map([], |row| {
            let platform = crate::Platform::parse(&row.get::<_, String>(2)?)
                .map_err(to_sql_conversion_error)?;
            let handle = row.get::<_, Option<String>>(7)?;
            let display_name = row.get::<_, Option<String>>(8)?;
            let profile_url = row.get::<_, Option<String>>(9)?;
            let public_data = row
                .get::<_, Option<String>>(10)?
                .map(|value| {
                    serde_json::from_str::<serde_json::Value>(&value)
                        .map_err(|error| to_sql_conversion_error(error.to_string()))
                })
                .transpose()?;
            let avatar_url = public_data
                .as_ref()
                .and_then(|value| avatar_url_from_public_data(platform, value));
            let source_kind = row
                .get::<_, Option<String>>(11)?
                .map(|value| crate::SourceKind::parse(&value).map_err(to_sql_conversion_error))
                .transpose()?
                .unwrap_or(crate::SourceKind::InitialPlatformAccount);
            let latest_profile = (handle.is_some()
                || display_name.is_some()
                || profile_url.is_some()
                || avatar_url.is_some())
            .then_some(CreatorTransferProfile {
                handle,
                display_name,
                profile_url,
                avatar_url,
            });
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                CreatorTransferAccount {
                    capture_order: 0,
                    platform: CreatorTransferPlatform::from(platform),
                    target_kind: CreatorTransferTargetKind::from(
                        crate::TargetKind::parse(&row.get::<_, String>(3)?)
                            .map_err(to_sql_conversion_error)?,
                    ),
                    external_id: row.get::<_, String>(4)?,
                    link_state: CreatorTransferLinkState::from(
                        crate::LinkState::parse(&row.get::<_, String>(5)?)
                            .map_err(to_sql_conversion_error)?,
                    ),
                    confidence: CreatorTransferConfidence::from(
                        crate::Confidence::parse(&row.get::<_, String>(6)?)
                            .map_err(to_sql_conversion_error)?,
                    ),
                    source_kind,
                    latest_profile,
                },
            ))
        })
        .map_err(|error| error.to_string())?;
    let mut grouped = BTreeMap::<String, Vec<CreatorTransferAccount>>::new();
    for row in rows {
        let (creator_id, account_id, mut account) = row.map_err(|error| error.to_string())?;
        account.capture_order = account_capture_order
            .get(&account_id)
            .copied()
            .ok_or_else(|| "Creator account capture order is missing".to_string())?;
        let accounts = match grouped.entry(creator_id) {
            Entry::Occupied(occupied)
                if occupied.get().len() >= CREATOR_BUNDLE_MAX_ACCOUNTS_PER_CREATOR =>
            {
                return Err(format!(
                    "creator {} exceeds the maximum of {CREATOR_BUNDLE_MAX_ACCOUNTS_PER_CREATOR} transferable accounts",
                    occupied.key()
                ));
            }
            entry => entry.or_default(),
        };
        accounts.push(account);
    }
    Ok(grouped)
}

struct TransferRouteRow {
    route_id: String,
    channel: crate::ContactChannel,
    locator_kind: crate::LocatorKind,
    normalized_value: String,
    display_value: String,
    confidence: CreatorTransferConfidence,
}

/// Every contact route with its locator, grouped by creator id in first-save
/// order (`created_at`, then `rowid`), which is the `save_order` the bundle
/// exports.
fn load_transfer_routes(
    transaction: &Transaction<'_>,
) -> Result<BTreeMap<String, Vec<TransferRouteRow>>, String> {
    let mut statement = transaction
        .prepare(
            "SELECT route.creator_id, route.id, route.confidence,
                    locator.channel, locator.kind, locator.normalized_value,
                    locator.display_value
             FROM creator_contact_routes route
             JOIN contact_locators locator ON locator.id = route.locator_id
             ORDER BY route.creator_id ASC, route.created_at ASC, route.rowid ASC",
        )
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                TransferRouteRow {
                    route_id: row.get(1)?,
                    confidence: CreatorTransferConfidence::from(
                        crate::Confidence::parse(&row.get::<_, String>(2)?)
                            .map_err(to_sql_conversion_error)?,
                    ),
                    channel: crate::ContactChannel::parse(&row.get::<_, String>(3)?)
                        .map_err(to_sql_conversion_error)?,
                    locator_kind: crate::LocatorKind::parse(&row.get::<_, String>(4)?)
                        .map_err(to_sql_conversion_error)?,
                    normalized_value: row.get(5)?,
                    display_value: row.get(6)?,
                },
            ))
        })
        .map_err(|error| error.to_string())?;
    let mut grouped = BTreeMap::<String, Vec<TransferRouteRow>>::new();
    for row in rows {
        let (creator_id, route) = row.map_err(|error| error.to_string())?;
        grouped.entry(creator_id).or_default().push(route);
    }
    Ok(grouped)
}

/// Observation source kinds per route, newest observation first, grouped as
/// creator id -> route id -> kinds. A route sees the observations attributed
/// to its Creator directly or through any of that Creator's linked accounts,
/// exactly the set the per-route loader used.
fn load_transfer_route_sources(
    transaction: &Transaction<'_>,
) -> Result<BTreeMap<String, BTreeMap<String, Vec<crate::SourceKind>>>, String> {
    let mut statement = transaction
        .prepare(
            "SELECT route.creator_id, route.id, source.kind
             FROM creator_contact_routes route
             JOIN locator_observations observation
               ON observation.locator_id = route.locator_id
             JOIN observation_sources source ON source.id = observation.source_id
             WHERE observation.creator_id = route.creator_id
                OR observation.platform_account_id IN (
                  SELECT link.platform_account_id FROM creator_account_links link
                  WHERE link.creator_id = route.creator_id
                )
             ORDER BY route.creator_id ASC, route.id ASC,
                      observation.observed_at DESC, observation.rowid DESC",
        )
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                crate::SourceKind::parse(&row.get::<_, String>(2)?)
                    .map_err(to_sql_conversion_error)?,
            ))
        })
        .map_err(|error| error.to_string())?;
    let mut grouped = BTreeMap::<String, BTreeMap<String, Vec<crate::SourceKind>>>::new();
    for row in rows {
        let (creator_id, route_id, kind) = row.map_err(|error| error.to_string())?;
        grouped
            .entry(creator_id)
            .or_default()
            .entry(route_id)
            .or_default()
            .push(kind);
    }
    Ok(grouped)
}

fn transfer_contact_routes(
    routes: Vec<TransferRouteRow>,
    mut sources: BTreeMap<String, Vec<crate::SourceKind>>,
) -> Result<Vec<CreatorTransferContactRoute>, String> {
    routes
        .into_iter()
        .enumerate()
        .map(|(index, route)| {
            Ok(CreatorTransferContactRoute {
                save_order: u32::try_from(index)
                    .map_err(|_| "Creator contact route order cannot be represented")?,
                channel: route.channel,
                locator_kind: route.locator_kind,
                normalized_value: route.normalized_value,
                display_value: route.display_value,
                confidence: route.confidence,
                source_kinds: sources.remove(&route.route_id).unwrap_or_default(),
            })
        })
        .collect()
}

#[cfg(test)]
pub(super) fn insert_creators_for_cap_test(
    repository: &SqliteRepository,
    count: usize,
) -> Result<(), String> {
    let mut connection = repository.lock()?;
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|error| error.to_string())?;
    for index in 0..count {
        let id = format!("cap-creator-{index:05}");
        transaction
            .execute(
                "INSERT INTO creators
                   (id, display_name, notes, created_at, updated_at, follower_counts_json)
                 VALUES (?1, ?2, 'excluded note', 1, 1, '{}')",
                rusqlite::params![id, format!("Creator {index}")],
            )
            .map_err(|error| error.to_string())?;
        transaction
            .execute(
                "INSERT INTO creator_relationships
                   (creator_id, stage, last_touch_at, created_at, updated_at)
                 VALUES (?1, 'prospect', NULL, 1, 1)",
                [id],
            )
            .map_err(|error| error.to_string())?;
    }
    transaction.commit().map_err(|error| error.to_string())
}
