//! Derive operation stops from current-revision receipts; never rewrite evidence.
use rusqlite::{Connection, params};
use std::collections::{BTreeMap, BTreeSet};

/// Only a classified parser failure may accumulate across authors. Generic
/// command errors, timeouts and missing individual accounts are not proof.
fn stopped(rows: impl IntoIterator<Item = (String, Option<String>)>) -> bool {
    let mut authors = BTreeSet::new();
    let mut previous = None;
    for (author, code) in rows {
        let Some(code) = code else { break };
        if matches!(
            code.as_str(),
            "opencli_risk_control" | "opencli_auth_required"
        ) {
            return true;
        }
        if !matches!(
            code.as_str(),
            "opencli_invalid_output" | "opencli_output_too_large"
        ) {
            break;
        }
        if previous.as_ref().is_some_and(|last| last != &code) {
            break;
        }
        previous = Some(code);
        if !author.is_empty() {
            authors.insert(author);
        }
        if authors.len() >= 3 {
            return true;
        }
    }
    false
}

pub(super) fn stops(
    db: &Connection,
    campaign: &str,
    kind: &str,
) -> Result<Vec<crate::DiscoveryReadStop>, String> {
    let table = match kind {
        "profile" => "discovery_profile_reads",
        "content" => "discovery_content_reads",
        _ => return Err("unsupported_read_health_kind".into()),
    };
    let mut query = db
        .prepare(&format!(
            "SELECT p.platform,p.profile_ref,p.author_id,p.stop_code FROM {table} p
         JOIN discovery_campaigns c ON c.id=p.discovery_campaign_id
         WHERE c.id=?1 AND p.expected_revision=c.current_revision AND p.status!='reading'
         ORDER BY p.call_number DESC"
        ))
        .map_err(|e| e.to_string())?;
    let rows = query
        .query_map([campaign], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, Option<String>>(3)?,
            ))
        })
        .map_err(|e| e.to_string())?;
    let mut scopes = BTreeMap::<(String, String), Vec<(String, Option<String>)>>::new();
    for row in rows {
        let (platform, profile, author, code) = row.map_err(|e| e.to_string())?;
        scopes
            .entry((platform, profile))
            .or_default()
            .push((author, code));
    }
    let mut result = Vec::new();
    for ((platform, profile), rows) in scopes {
        let code = rows.first().and_then(|r| r.1.clone()).unwrap_or_default();
        if stopped(rows) {
            result.push(crate::DiscoveryReadStop {
                platform,
                profile_ref: profile,
                operation: kind.into(),
                code,
            });
        }
    }
    Ok(result)
}

pub(super) fn excluded_hits(
    db: &Connection,
    campaign: &str,
    kind: &str,
) -> Result<Vec<String>, String> {
    let mut hits = Vec::new();
    let mut query = db.prepare("SELECT id FROM discovery_search_hits WHERE discovery_campaign_id=?1 AND platform=?2 AND profile_ref=?3").map_err(|e|e.to_string())?;
    for stop in stops(db, campaign, kind)? {
        hits.extend(
            query
                .query_map(params![campaign, stop.platform, stop.profile_ref], |r| {
                    r.get::<_, String>(0)
                })
                .map_err(|e| e.to_string())?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| e.to_string())?,
        );
    }
    Ok(hits)
}

pub(super) fn check(db: &Connection, campaign: &str, kind: &str, hit: &str) -> Result<(), String> {
    if excluded_hits(db, campaign, kind)?
        .iter()
        .any(|id| id == hit)
    {
        return Err("discovery_read_operation_stopped".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::*;
    #[test]
    fn parser_stop_survives_reopen_and_rejected_claim_does_not_spend_budget() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("health.sqlite");
        let owner = CreatorRuntime::open(&path, CreatorGateways::default()).unwrap();
        owner.create_discovery_campaign(CreateDiscoveryCampaignInput {
            id: "health".into(), brief: "Shoe creators".into(), opencli_version: "1.8.8".into(),
            criteria: serde_json::json!({"min_followers":10000,"category":"鞋子","region":"","language":"","requires_ai_tool_demo":false,"requires_public_tool_url":false}),
            scopes: vec![DiscoveryPlatformScope { platform:"bilibili".into(),profile_ref:"dedicated".into(),keywords:vec!["鞋子".into()] }],
            target_count:30,max_calls:Some(100),
        }).unwrap();
        owner.approve_discovery_campaign("health", 1).unwrap();
        let claim = owner
            .claim_discovery_search("health", 1, "bilibili", "dedicated")
            .unwrap();
        owner
            .finish_discovery_search(FinishDiscoverySearchInput {
                claim,
                status: "completed".into(),
                stop_code: None,
                search_hits: (1..=4)
                    .map(|n| DiscoverySearchHitInput {
                        source_url: format!("https://www.bilibili.com/video/BV1shoe0000{n}"),
                        title: "鞋子测评".into(),
                        author_label: format!("Runner{n}"),
                        author_id: Some(format!("16831029{n}")),
                        author_url: Some(format!("https://space.bilibili.com/16831029{n}")),
                    })
                    .collect(),
            })
            .unwrap();
        let hits = owner
            .list_discovery_search_hits(ListDiscoverySearchHitsInput {
                campaign_id: "health".into(),
                after_id: None,
                limit: 10,
            })
            .unwrap();
        for hit in hits.iter().take(3) {
            let claim = owner.claim_discovery_profile("health", 1, &hit.id).unwrap();
            owner
                .finish_discovery_profile(FinishDiscoveryProfileInput {
                    claim,
                    native_profile: None,
                    stop_code: Some("opencli_invalid_output".into()),
                })
                .unwrap();
        }
        drop(owner);
        let repository = super::super::SqliteRepository::open(path.join("crm.db")).unwrap();
        let db = repository.read_connection().unwrap();
        assert_eq!(stops(&db, "health", "profile").unwrap().len(), 1);
        assert!(stops(&db, "health", "content").unwrap().is_empty());
        assert!(stops(&db, "other-campaign", "profile").unwrap().is_empty());
        assert!(
            super::super::discovery_profiles::next_profile_hit(&repository, "health", &[])
                .unwrap()
                .is_none()
        );
        let before: u32 = db
            .query_row(
                "SELECT used_calls FROM discovery_campaigns WHERE id='health'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert!(
            super::super::discovery_profiles::claim_profile_read(
                &repository,
                "health",
                1,
                &hits[3].id
            )
            .is_err()
        );
        let after: u32 = db
            .query_row(
                "SELECT used_calls FROM discovery_campaigns WHERE id='health'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(before, after);
    }
    fn rows(values: &[(&str, Option<&str>)]) -> Vec<(String, Option<String>)> {
        values
            .iter()
            .map(|(a, c)| (a.to_string(), c.map(str::to_owned)))
            .collect()
    }
    #[test]
    fn repeated_parser_failures_require_three_distinct_authors() {
        let code = Some("opencli_invalid_output");
        assert!(stopped(rows(&[("c", code), ("b", code), ("a", code)])));
        assert!(!stopped(rows(&[("a", code), ("a", code), ("a", code)])));
        assert!(!stopped(rows(&[("c", code), ("b", None), ("a", code)])));
        assert!(!stopped(rows(&[("", code), ("", code), ("", code)])));
    }
    #[test]
    fn generic_failures_do_not_stop_platform_and_auth_does() {
        assert!(!stopped(rows(&[
            ("c", Some("opencli_command_failed")),
            ("b", Some("opencli_command_failed")),
            ("a", Some("opencli_command_failed"))
        ])));
        assert!(stopped(rows(&[("a", Some("opencli_auth_required"))])));
        assert!(stopped(rows(&[("a", Some("opencli_risk_control"))])));
    }
}
