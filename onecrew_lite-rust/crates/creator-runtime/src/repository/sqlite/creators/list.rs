//! Query the full Creator relation before applying a bounded keyset page.
use rusqlite::{Connection, Row, TransactionBehavior, params_from_iter, types::Value};
use serde::{Deserialize, Serialize};

use super::{
    SqliteRepository, enrich_summary_identity, parse_follower_counts, to_sql_conversion_error,
};
use crate::{
    CREATOR_PAGE_LIMITS, CreatorDateRange, CreatorGroupFilter, CreatorGroupId, CreatorGroupRef,
    CreatorId, CreatorListFacets, CreatorListPage, CreatorListSort, CreatorPlatformCount,
    CreatorSortDirection, CreatorSortField, CreatorStage, CreatorStageCount, CreatorSummary,
    ListCreatorsInput, Platform, decode_page_cursor, encode_page_cursor,
};

/// Both list filtering/sorting and record detail use the same outbound-touch definition.
/// The argument is an internal SQL expression; actual Creator IDs remain bound values.
pub(super) fn last_touch_expression(creator: &'static str) -> String {
    format!("(SELECT MAX(value) FROM (
        SELECT MAX(touch.occurred_at) AS value FROM touchpoints touch WHERE touch.creator_id = {creator}
        UNION ALL
        SELECT MAX(email.occurred_at) FROM emails email
          JOIN email_threads thread ON thread.id = email.thread_id
          WHERE thread.creator_id = {creator} AND email.direction = 'outbound'
        UNION ALL
        SELECT MAX(message.sent_at) FROM messages message
          JOIN conversation_contact_routes link ON link.conversation_id = message.conversation_id
          JOIN creator_contact_routes route ON route.id = link.contact_route_id
          WHERE route.creator_id = {creator} AND message.direction = 'outbound' AND message.status = 'sent'))")
}

// Follower fallback is primary account, first-capture platform order, then
// alphabetic platform order, matching the Web's explicit display policy.
fn projection() -> String {
    format!("WITH identity AS (
        SELECT creator.*, relationship.stage, creator_group.name AS group_name,
          COALESCE(creator.pinned_at, 0) AS pin_rank,
          (SELECT account.platform FROM platform_accounts account
             JOIN creator_account_links link ON link.platform_account_id = account.id
             WHERE link.creator_id = creator.id AND link.state != 'rejected'
             ORDER BY account.created_at ASC, account.rowid ASC LIMIT 1) AS primary_platform,
          {} AS last_touch_at
        FROM creators creator
        JOIN creator_relationships relationship ON relationship.creator_id = creator.id
        LEFT JOIN creator_groups creator_group ON creator_group.id = creator.group_id
      ), projected AS (
        SELECT identity.*,
          CAST(COALESCE(
            json_extract(follower_counts_json, '$.' || primary_platform),
            (SELECT json_extract(identity.follower_counts_json, '$.' || account.platform)
               FROM platform_accounts account JOIN creator_account_links link ON link.platform_account_id = account.id
               WHERE link.creator_id = identity.id AND link.state != 'rejected'
                 AND json_extract(identity.follower_counts_json, '$.' || account.platform) IS NOT NULL
               GROUP BY account.platform
               ORDER BY MIN(account.created_at) ASC, MIN(account.rowid) ASC LIMIT 1),
            json_extract(follower_counts_json, '$.instagram'),
            json_extract(follower_counts_json, '$.telegram'),
            json_extract(follower_counts_json, '$.tiktok'),
            json_extract(follower_counts_json, '$.x'),
            json_extract(follower_counts_json, '$.youtube'), -1) AS INTEGER) AS follower_count
        FROM identity
      )", last_touch_expression("creator.id"))
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Facet {
    Stage,
    Platform,
}

fn bind(values: &mut Vec<Value>, value: impl Into<Value>) -> String {
    values.push(value.into());
    format!("?{}", values.len())
}

fn date_clause(
    field: &str,
    range: &Option<CreatorDateRange>,
    parts: &mut Vec<String>,
    values: &mut Vec<Value>,
) {
    if let Some(range) = range {
        if let Some(from) = range.from {
            parts.push(format!("{field} >= {}", bind(values, from as i64)));
        }
        if let Some(to) = range.to_exclusive {
            parts.push(format!("{field} < {}", bind(values, to as i64)));
        }
    }
}

fn filters(input: &ListCreatorsInput, omit: Option<Facet>) -> (String, Vec<Value>) {
    let mut parts = vec!["1 = 1".to_owned()];
    let mut values = Vec::new();
    if !input.include_archived {
        parts.push("creator.archived_at IS NULL".into());
    }
    match &input.group {
        None => {}
        Some(CreatorGroupFilter::Uncategorized) => parts.push("creator.group_id IS NULL".into()),
        Some(CreatorGroupFilter::Group { group_id }) => parts.push(format!(
            "creator.group_id = {}",
            bind(&mut values, group_id.as_str().to_owned())
        )),
    }
    if omit != Some(Facet::Stage) && !input.stages.is_empty() {
        let selected = input
            .stages
            .iter()
            .map(|value| bind(&mut values, value.as_str().to_owned()))
            .collect::<Vec<_>>()
            .join(",");
        parts.push(format!("creator.stage IN ({selected})"));
    }
    if omit != Some(Facet::Platform) && !input.primary_platforms.is_empty() {
        let selected = input
            .primary_platforms
            .iter()
            .map(|value| bind(&mut values, value.as_str().to_owned()))
            .collect::<Vec<_>>()
            .join(",");
        parts.push(format!("creator.primary_platform IN ({selected})"));
    }
    if let Some(query) = &input.query {
        let value = bind(&mut values, query.clone());
        parts.push(format!(
            "(instr(lower(creator.display_name), lower({value})) > 0
            OR EXISTS (SELECT 1 FROM creator_account_links query_link
              JOIN platform_account_observations query_observation ON query_observation.id = (
                SELECT latest.id FROM platform_account_observations latest
                WHERE latest.platform_account_id = query_link.platform_account_id
                ORDER BY latest.observed_at DESC, latest.rowid DESC LIMIT 1)
              WHERE query_link.creator_id = creator.id AND query_link.state != 'rejected'
                AND instr(lower(COALESCE(query_observation.handle, '')), lower({value})) > 0)
            OR EXISTS (SELECT 1 FROM creator_contact_routes query_route
              JOIN contact_locators query_locator ON query_locator.id = query_route.locator_id
              WHERE query_route.creator_id = creator.id AND (
                instr(lower(query_locator.channel), lower({value})) > 0
                OR instr(lower(query_locator.kind), lower({value})) > 0
                OR instr(lower(query_locator.normalized_value), lower({value})) > 0
                OR instr(lower(query_locator.display_value), lower({value})) > 0)))"
        ));
    }
    date_clause(
        "creator.last_touch_at",
        &input.last_touch_range,
        &mut parts,
        &mut values,
    );
    date_clause(
        "creator.updated_at",
        &input.updated_range,
        &mut parts,
        &mut values,
    );
    (parts.join(" AND "), values)
}

fn normalize(input: &mut ListCreatorsInput) -> Result<(), String> {
    input.query = input
        .query
        .as_deref()
        .map(|q| q.trim().trim_start_matches('@').trim())
        .filter(|q| !q.is_empty())
        .map(str::to_owned);
    input.stages.sort_unstable();
    input.stages.dedup();
    input.primary_platforms.sort_unstable();
    input.primary_platforms.dedup();
    for range in [&mut input.last_touch_range, &mut input.updated_range] {
        if let Some(bounds) = range {
            if bounds
                .from
                .into_iter()
                .chain(bounds.to_exclusive)
                .any(|value| value > i64::MAX as u64)
                || bounds
                    .from
                    .zip(bounds.to_exclusive)
                    .is_some_and(|(from, to)| from >= to)
            {
                return Err("creator date range is invalid".into());
            }
            if bounds.from.is_none() && bounds.to_exclusive.is_none() {
                *range = None;
            }
        }
    }
    if input.sorting.len() > 6
        || input.sorting.iter().enumerate().any(|(index, value)| {
            input.sorting[..index]
                .iter()
                .any(|prior| prior.field == value.field)
        })
    {
        return Err("creator sorting is invalid".into());
    }
    if input.sorting.is_empty() {
        input.sorting.push(CreatorListSort {
            field: CreatorSortField::UpdatedAt,
            direction: CreatorSortDirection::Desc,
        });
    }
    Ok(())
}

struct SortTerm {
    expression: &'static str,
    direction: CreatorSortDirection,
    text: bool,
}

fn sort_terms(input: &ListCreatorsInput) -> Vec<SortTerm> {
    let mut terms = vec![SortTerm {
        expression: "creator.pin_rank",
        direction: CreatorSortDirection::Desc,
        text: false,
    }];
    for sort in &input.sorting {
        let (expression, text) = match sort.field {
            CreatorSortField::DisplayName => ("lower(creator.display_name)", true),
            CreatorSortField::Followers => ("creator.follower_count", false),
            CreatorSortField::Score => ("COALESCE(creator.score, -1)", false),
            CreatorSortField::LastTouchAt => ("COALESCE(creator.last_touch_at, -1)", false),
            CreatorSortField::UpdatedAt => ("creator.updated_at", false),
            CreatorSortField::CreatedAt => ("creator.created_at", false),
        };
        terms.push(SortTerm {
            expression,
            direction: sort.direction,
            text,
        });
    }
    terms.push(SortTerm {
        expression: "creator.id",
        direction: CreatorSortDirection::Desc,
        text: true,
    });
    terms
}

#[derive(Serialize, Deserialize)]
#[serde(untagged)]
enum CursorValue {
    Number(i64),
    Text(String),
}

fn cursor_clause(
    cursor: &str,
    context: &str,
    terms: &[SortTerm],
    values: &mut Vec<Value>,
) -> Result<String, String> {
    let invalid = || "page cursor is not valid for this query".to_string();
    let position: Vec<CursorValue> =
        serde_json::from_str(&decode_page_cursor(cursor, context)?).map_err(|_| invalid())?;
    if position.len() != terms.len() {
        return Err(invalid());
    }
    let mut bindings = Vec::new();
    for (value, term) in position.into_iter().zip(terms) {
        let value = match (value, term.text) {
            (CursorValue::Number(value), false) => Value::Integer(value),
            (CursorValue::Text(value), true) => Value::Text(value),
            _ => return Err(invalid()),
        };
        bindings.push(bind(values, value));
    }
    let mut alternatives = Vec::new();
    for (index, term) in terms.iter().enumerate() {
        let mut equality = (0..index)
            .map(|prior| format!("{} = {}", terms[prior].expression, bindings[prior]))
            .collect::<Vec<_>>();
        let comparison = match term.direction {
            CreatorSortDirection::Asc => ">",
            CreatorSortDirection::Desc => "<",
        };
        equality.push(format!(
            "{} {comparison} {}",
            term.expression, bindings[index]
        ));
        alternatives.push(format!("({})", equality.join(" AND ")));
    }
    Ok(format!("({})", alternatives.join(" OR ")))
}

fn facets(
    connection: &Connection,
    projection: &str,
    input: &ListCreatorsInput,
) -> Result<CreatorListFacets, String> {
    let (predicate, values) = filters(input, None);
    let total = connection
        .query_row(
            &format!("{projection} SELECT COUNT(*) FROM projected creator WHERE {predicate}"),
            params_from_iter(&values),
            |row| row.get(0),
        )
        .map_err(|e| e.to_string())?;
    let counts = |column: &str, facet| -> Result<Vec<(String, u64)>, String> {
        let (predicate, values) = filters(input, Some(facet));
        let mut statement = connection.prepare(&format!("{projection} SELECT {column}, COUNT(*) FROM projected creator WHERE {predicate} AND {column} IS NOT NULL GROUP BY {column} ORDER BY {column}")).map_err(|e| e.to_string())?;
        statement
            .query_map(params_from_iter(&values), |row| {
                Ok((row.get(0)?, row.get(1)?))
            })
            .map_err(|e| e.to_string())?
            .map(|row| row.map_err(|e| e.to_string()))
            .collect()
    };
    let stages = counts("creator.stage", Facet::Stage)?
        .into_iter()
        .map(|(value, count)| {
            Ok(CreatorStageCount {
                value: CreatorStage::parse(&value)?,
                count,
            })
        })
        .collect::<Result<_, String>>()?;
    let primary_platforms = counts("creator.primary_platform", Facet::Platform)?
        .into_iter()
        .map(|(value, count)| {
            Ok(CreatorPlatformCount {
                value: Platform::parse(&value)?,
                count,
            })
        })
        .collect::<Result<_, String>>()?;
    Ok(CreatorListFacets {
        total,
        stages,
        primary_platforms,
    })
}

fn summary(row: &Row<'_>) -> rusqlite::Result<CreatorSummary> {
    let stage: String = row.get("stage")?;
    let followers: String = row.get("follower_counts_json")?;
    Ok(CreatorSummary {
        tags: serde_json::from_str(&row.get::<_, String>("tags_json")?)
            .map_err(|error| to_sql_conversion_error(error.to_string()))?,
        id: CreatorId::new(row.get::<_, String>("id")?),
        display_name: row.get("display_name")?,
        stage: CreatorStage::parse(&stage).map_err(to_sql_conversion_error)?,
        group: row
            .get::<_, Option<String>>("group_id")?
            .zip(row.get::<_, Option<String>>("group_name")?)
            .map(|(id, name)| CreatorGroupRef {
                id: CreatorGroupId::new(id),
                name,
            }),
        primary_platform: None,
        primary_handle: None,
        avatar_url: None,
        platforms: Vec::new(),
        channels: Vec::new(),
        has_manual_origin: row.get("has_manual_origin")?,
        score: row.get("score")?,
        assessment: row.get("assessment")?,
        follower_counts: parse_follower_counts(&followers).map_err(to_sql_conversion_error)?,
        last_touch_at: row.get("last_touch_at")?,
        pinned_at: row.get("pinned_at")?,
        platform_account_count: row.get("platform_account_count")?,
        contact_route_count: row.get("contact_route_count")?,
        archived_at: row.get("archived_at")?,
        created_at: row.get("created_at")?,
        updated_at: row.get("updated_at")?,
    })
}

pub(super) fn list_creators(
    repository: &SqliteRepository,
    mut input: ListCreatorsInput,
) -> Result<CreatorListPage, String> {
    normalize(&mut input)?;
    let limit = CREATOR_PAGE_LIMITS.effective(input.limit) as usize;
    // JSON avoids ambiguous delimiters; only query semantics bind the cursor.
    let mut context_input = input.clone();
    context_input.limit = None;
    context_input.cursor = None;
    context_input.include_facets = false;
    let context = format!(
        "creator_list_v2|{}",
        serde_json::to_string(&context_input).map_err(|e| e.to_string())?
    );
    let terms = sort_terms(&input);
    let (mut predicate, mut values) = filters(&input, None);
    if let Some(cursor) = &input.cursor {
        predicate.push_str(" AND ");
        predicate.push_str(&cursor_clause(cursor, &context, &terms, &mut values)?);
    }
    let mut connection = repository.read_connection()?;
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Deferred)
        .map_err(|e| e.to_string())?;
    if let Some(CreatorGroupFilter::Group { group_id }) = &input.group {
        let exists: bool = transaction
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM creator_groups WHERE id = ?1)",
                [group_id.as_str()],
                |row| row.get(0),
            )
            .map_err(|e| e.to_string())?;
        if !exists {
            return Err("creator group not found".into());
        }
    }
    let projection = projection();
    let facets = if input.include_facets {
        Some(facets(&transaction, &projection, &input)?)
    } else {
        None
    };
    let order = terms
        .iter()
        .map(|term| {
            format!(
                "{} {}",
                term.expression,
                match term.direction {
                    CreatorSortDirection::Asc => "ASC",
                    CreatorSortDirection::Desc => "DESC",
                }
            )
        })
        .collect::<Vec<_>>()
        .join(", ");
    let positions = terms
        .iter()
        .enumerate()
        .map(|(index, term)| format!("{} AS cursor_{index}", term.expression))
        .collect::<Vec<_>>()
        .join(", ");
    let limit_parameter = bind(&mut values, (limit + 1) as i64);
    let sql = format!("{projection} SELECT creator.*,
        (SELECT COUNT(*) FROM creator_account_links link WHERE link.creator_id = creator.id AND link.state != 'rejected') AS platform_account_count,
        (SELECT COUNT(*) FROM creator_contact_routes route WHERE route.creator_id = creator.id) AS contact_route_count,
        EXISTS(SELECT 1 FROM creator_origins origin WHERE origin.creator_id = creator.id AND origin.kind = 'manual_entry') AS has_manual_origin,
        {positions} FROM projected creator WHERE {predicate} ORDER BY {order} LIMIT {limit_parameter}");
    let mut statement = transaction.prepare(&sql).map_err(|e| e.to_string())?;
    let mut rows = statement
        .query_map(params_from_iter(&values), |row| {
            let position = terms
                .iter()
                .enumerate()
                .map(|(index, term)| {
                    let column = format!("cursor_{index}");
                    if term.text {
                        row.get(column.as_str()).map(CursorValue::Text)
                    } else {
                        row.get(column.as_str()).map(CursorValue::Number)
                    }
                })
                .collect::<rusqlite::Result<Vec<_>>>()?;
            Ok((summary(row)?, position))
        })
        .map_err(|e| e.to_string())?
        .map(|row| row.map_err(|e| e.to_string()))
        .collect::<Result<Vec<_>, _>>()?;
    drop(statement);
    let has_more = rows.len() > limit;
    rows.truncate(limit);
    let next_cursor = if has_more {
        rows.last()
            .map(|(_, position)| {
                serde_json::to_string(position)
                    .map(|position| encode_page_cursor(&context, &position))
            })
            .transpose()
            .map_err(|e| e.to_string())?
    } else {
        None
    };
    let mut items = rows
        .into_iter()
        .map(|(summary, _)| summary)
        .collect::<Vec<_>>();
    for row in &mut items {
        enrich_summary_identity(&transaction, row)?;
    }
    transaction.commit().map_err(|e| e.to_string())?;
    Ok(CreatorListPage {
        items,
        next_cursor,
        facets,
    })
}
