use super::{SqliteRepository, discovery_contacts};
use crate::{DiscoveryContactLookup, DiscoveryPublicPageInput};
use rusqlite::{Connection, TransactionBehavior, params};

fn page_input(input: &DiscoveryContactLookup) -> DiscoveryPublicPageInput {
    DiscoveryPublicPageInput {
        campaign_id: input.campaign_id.clone(),
        expected_revision: input.expected_revision,
        candidate_id: input.candidate_id.clone(),
        profile_ref: input.profile_ref.clone(),
        requested_url: String::new(),
    }
}

fn next_on(
    connection: &Connection,
    campaign: &str,
    excluded_platforms: &[String],
    candidate: Option<&str>,
) -> Result<Option<DiscoveryContactLookup>, String> {
    let mut stmt = connection.prepare("SELECT c.id,c.criteria_revision,c.source_profile_ref FROM discovery_candidates c
      JOIN discovery_campaigns d ON d.id=c.discovery_campaign_id
      JOIN discovery_campaign_revisions a ON a.discovery_campaign_id=d.id AND a.revision=d.current_revision
      JOIN discovery_platform_runs p ON p.discovery_campaign_id=d.id AND p.platform=c.source_platform AND p.profile_ref=c.source_profile_ref
      WHERE d.id=?1 AND d.phase='active' AND a.approved_at IS NOT NULL AND c.criteria_revision=d.current_revision
       AND NOT EXISTS(SELECT 1 FROM json_each(?2) excluded WHERE excluded.value IN (c.platform,c.source_platform))
       AND (?3 IS NULL OR c.id=?3)
       AND c.verdict='qualified' AND p.status='completed' AND (d.max_calls IS NULL OR d.used_calls<d.max_calls)
       AND NOT EXISTS(SELECT 1 FROM discovery_platform_runs r WHERE r.discovery_campaign_id=d.id AND r.status='searching')
      ORDER BY c.id").map_err(|e|e.to_string())?;
    let rows = stmt
        .query_map(
            params![
                campaign,
                serde_json::to_string(excluded_platforms).map_err(|e| e.to_string())?,
                candidate
            ],
            |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, u32>(1)?,
                    r.get::<_, String>(2)?,
                ))
            },
        )
        .map_err(|e| e.to_string())?;
    for row in rows {
        let (candidate_id, expected_revision, profile_ref) = row.map_err(|e| e.to_string())?;
        let mut input = DiscoveryContactLookup {
            campaign_id: campaign.into(),
            expected_revision,
            candidate_id,
            profile_ref,
            method: String::new(),
            native_author_id: String::new(),
            author_label: String::new(),
            username: None,
            query: String::new(),
            call_number: 0,
            status: "planned".into(),
            links: vec![],
            stop_code: None,
        };
        let Some(material) = discovery_contacts::native_material(connection, &page_input(&input))?
        else {
            continue;
        };
        // Keep known official page reads available, but do not expand through
        // Maigret/search once the author has published an alternative account.
        // Unconfirmed third-party links are not sufficient to stop discovery.
        if discovery_contacts::social_accounts(
            connection,
            campaign,
            expected_revision,
            &input.candidate_id,
            &material.native_author_id,
        )?
        .iter()
        .any(|contact| contact.attribution == crate::DiscoveryContactAttribution::AuthorPublished)
        {
            continue;
        }
        let native_bluesky_username = if material.hit.platform == "bluesky" {
            let receipt = super::discovery_profiles::load_profile_row(
                connection,
                campaign,
                &material.hit.id,
            )?;
            receipt
                .native_profile
                .as_ref()
                .and_then(|v| v["handle"].as_str())
                .filter(|handle| {
                    crate::discovery_bluesky::valid_handle(handle) && handle.len() <= 64
                })
                .map(str::to_owned)
        } else {
            None
        };
        let linked_username = material
            .native_profile
            .as_ref()
            .and_then(|p| p.bio.as_deref())
            .into_iter()
            .chain(material.video_description.as_deref())
            .chain(
                material
                    .native_content
                    .as_ref()
                    .and_then(|r| r.native_content.as_ref())
                    .and_then(|c| {
                        c[if material.hit.platform == "bilibili" {
                            "description"
                        } else {
                            "text"
                        }]
                        .as_str()
                    }),
            )
            .find_map(crate::contact_navigation::linked_username);
        input.native_author_id = material.native_author_id;
        input.author_label = material.hit.author_label;
        input.username = native_bluesky_username
            .or_else(|| {
                material
                    .native_profile
                    .as_ref()
                    .map(|p| p.profile_url.as_str())
                    .or(material.hit.author_url.as_deref())
                    .and_then(crate::discovery_contact_lookup::exact_username)
            })
            .or(linked_username);
        // Search terms come from the saved author, never model-proposed email addresses.
        let name = input
            .username
            .as_deref()
            .unwrap_or(&input.author_label)
            .trim();
        if name.is_empty() || name.len() > 200 || name.contains(['\n', '\r', '\0', '"']) {
            continue;
        }
        let criteria_json: String = connection.query_row("SELECT criteria_json FROM discovery_campaign_revisions WHERE discovery_campaign_id=?1 AND revision=?2", params![campaign, expected_revision], |r| r.get(0)).map_err(|e| e.to_string())?;
        let criteria: serde_json::Value =
            serde_json::from_str(&criteria_json).map_err(|e| e.to_string())?;
        let terms = criteria["contact_search_terms"]
            .as_array()
            .map(|values| {
                values
                    .iter()
                    .filter_map(|v| v.as_str().map(str::to_owned))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        input.query = crate::contact_navigation::search_query(
            name,
            criteria["language"].as_str().unwrap_or(""),
            &terms,
        );
        // Actual email or Telegram satisfies the formal recommendation policy.
        // Hidden email and associated accounts retain their separate routes.
        if !discovery_contacts::list(
            connection,
            campaign,
            expected_revision,
            &input.candidate_id,
            &input.native_author_id,
        )?
        .is_empty()
        {
            continue;
        }
        for method in ["maigret", "search"] {
            if method == "maigret" && input.username.is_none() {
                continue;
            }
            let saved:bool=connection.query_row("SELECT EXISTS(SELECT 1 FROM discovery_contact_lookups WHERE candidate_id=?1 AND criteria_revision=?2 AND method=?3)",params![input.candidate_id,expected_revision,method],|r|r.get(0)).map_err(|e|e.to_string())?;
            if saved {
                continue;
            }
            if method == "maigret" {
                // A failed username lookup belongs to that candidate. Only a
                // shared dependency failure, or an actual Maigret challenge,
                // prevents independent username lookups in this campaign.
                // Challenges on linked websites are already isolated by host
                // in the public-page owner; they do not disable Maigret.
                let blocked:bool=connection.query_row("SELECT EXISTS(SELECT 1 FROM discovery_contact_lookups WHERE discovery_campaign_id=?1 AND method='maigret' AND json_extract(receipt_json,'$.status')='failed'
                  AND json_extract(receipt_json,'$.stop_code') IN ('opencli_unavailable','opencli_version_changed','opencli_daemon_unavailable','opencli_extension_unavailable','opencli_command_unavailable','opencli_risk_control','opencli_auth_required','opencli_profile_unavailable'))",[campaign],|r|r.get(0)).map_err(|e|e.to_string())?;
                if blocked {
                    continue;
                }
            }
            if method == "search" {
                let google_ready:bool=connection.query_row("SELECT NOT EXISTS(SELECT 1 FROM discovery_platform_runs WHERE discovery_campaign_id=?1 AND platform='google' AND profile_ref=?2 AND status IN ('unauthorized','risk_control_stop','technical_stop')) AND NOT EXISTS(SELECT 1 FROM discovery_contact_lookups WHERE discovery_campaign_id=?1 AND method='search' AND json_extract(receipt_json,'$.stop_code') IN ('opencli_risk_control','opencli_auth_required'))",params![campaign,input.profile_ref],|r|r.get(0)).map_err(|e|e.to_string())?;
                if !google_ready {
                    continue;
                }
            }
            input.method = method.into();
            return Ok(Some(input));
        }
    }
    Ok(None)
}
pub(super) fn next(
    repository: &SqliteRepository,
    campaign: &str,
    excluded_platforms: &[String],
) -> Result<Option<DiscoveryContactLookup>, String> {
    let connection = repository.read_connection()?;
    next_on(&connection, campaign, excluded_platforms, None)
}
pub(super) fn claim(
    repository: &SqliteRepository,
    input: DiscoveryContactLookup,
) -> Result<DiscoveryContactLookup, String> {
    let mut connection = repository.lock()?;
    let tx = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|e| e.to_string())?;
    let mut planned = next_on(&tx, &input.campaign_id, &[], Some(&input.candidate_id))?
        .ok_or("contact_lookup_not_eligible")?;
    if planned != input {
        return Err("contact_lookup_plan_changed".into());
    }
    let used: u32 = tx
        .query_row(
            "SELECT used_calls FROM discovery_campaigns WHERE id=?1",
            [&input.campaign_id],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())?;
    planned.call_number = used
        .checked_add(1)
        .ok_or("contact_lookup_budget_overflow")?;
    planned.status = "reading".into();
    tx.execute(
        "UPDATE discovery_campaigns SET used_calls=used_calls+1 WHERE id=?1",
        [&input.campaign_id],
    )
    .map_err(|e| e.to_string())?;
    tx.execute(
        "INSERT INTO discovery_contact_lookups VALUES(?1,?2,?3,?4,?5)",
        params![
            input.candidate_id,
            input.campaign_id,
            input.expected_revision,
            input.method,
            serde_json::to_string(&planned).map_err(|e| e.to_string())?
        ],
    )
    .map_err(|e| e.to_string())?;
    let active = format!("contact:{}:{}", input.candidate_id, planned.call_number);
    let changed=tx.execute("UPDATE discovery_platform_runs SET status='searching',active_query=?3 WHERE discovery_campaign_id=?1 AND profile_ref=?2 AND platform=(SELECT source_platform FROM discovery_candidates WHERE id=?4)",params![input.campaign_id,input.profile_ref,active,input.candidate_id]).map_err(|e|e.to_string())?;
    if changed != 1 {
        return Err("contact_lookup_source_run_changed".into());
    }
    tx.commit().map_err(|e| e.to_string())?;
    Ok(planned)
}
pub(super) fn finish(
    repository: &SqliteRepository,
    mut input: DiscoveryContactLookup,
) -> Result<DiscoveryContactLookup, String> {
    if input.links.len() > 5
        || input.links.iter().any(|s| {
            s.len() > 2048
                || !url::Url::parse(s).is_ok_and(|u| {
                    u.scheme() == "https"
                        && u.host_str().is_some()
                        && u.username().is_empty()
                        && u.password().is_none()
                        && u.port().is_none()
                })
        })
        || !matches!(input.status.as_str(), "completed" | "failed")
        || (input.status == "failed" && (!input.links.is_empty() || input.stop_code.is_none()))
        || (input.status == "completed" && input.stop_code.is_some())
    {
        return Err("contact_lookup_invalid_result".into());
    }
    let mut connection = repository.lock()?;
    let tx = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|e| e.to_string())?;
    let raw:String=tx.query_row("SELECT receipt_json FROM discovery_contact_lookups WHERE candidate_id=?1 AND criteria_revision=?2 AND method=?3",params![input.candidate_id,input.expected_revision,input.method],|r|r.get(0)).map_err(|e|e.to_string())?;
    let mut saved: DiscoveryContactLookup =
        serde_json::from_str(&raw).map_err(|e| e.to_string())?;
    if saved.status != "reading" {
        return if saved == input {
            Ok(saved)
        } else {
            Err("contact_lookup_already_finished".into())
        };
    }
    saved.status = input.status.clone();
    saved.links = input.links.clone();
    saved.stop_code = input.stop_code.clone();
    if saved != input {
        return Err("contact_lookup_claim_changed".into());
    }
    let approved:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM discovery_campaigns WHERE id=?1 AND current_revision=?2 AND phase='active')",params![input.campaign_id,input.expected_revision],|r|r.get(0)).map_err(|e|e.to_string())?;
    if !approved || discovery_contacts::native_material(&tx, &page_input(&input))?.is_none() {
        input.status = "failed".into();
        input.links.clear();
        input.stop_code = Some("contact_lookup_stale".into());
    }
    tx.execute("UPDATE discovery_contact_lookups SET receipt_json=?4 WHERE candidate_id=?1 AND criteria_revision=?2 AND method=?3",params![input.candidate_id,input.expected_revision,input.method,serde_json::to_string(&input).map_err(|e|e.to_string())?]).map_err(|e|e.to_string())?;
    // Candidate enrichment errors are local receipts. They do not stop other platform reads.
    tx.execute("UPDATE discovery_platform_runs SET status='completed',active_query=NULL WHERE discovery_campaign_id=?1 AND profile_ref=?2 AND active_query=?3",params![input.campaign_id,input.profile_ref,format!("contact:{}:{}",input.candidate_id,input.call_number)]).map_err(|e|e.to_string())?;
    if input.method == "search"
        && input
            .stop_code
            .as_deref()
            .is_some_and(|c| matches!(c, "opencli_risk_control" | "opencli_auth_required"))
    {
        tx.execute("UPDATE discovery_platform_runs SET status=?4,stop_code=?3 WHERE discovery_campaign_id=?1 AND platform='google' AND profile_ref=?2",params![input.campaign_id,input.profile_ref,input.stop_code,if input.stop_code.as_deref()==Some("opencli_risk_control") {"risk_control_stop"} else {"unauthorized"}]).map_err(|e|e.to_string())?;
    }
    tx.commit().map_err(|e| e.to_string())?;
    Ok(input)
}

pub(super) fn links(
    connection: &Connection,
    candidate: &str,
    revision: u32,
) -> Result<Vec<(String, String)>, String> {
    let mut stmt=connection.prepare("SELECT receipt_json FROM discovery_contact_lookups WHERE candidate_id=?1 AND criteria_revision=?2 ORDER BY method").map_err(|e|e.to_string())?;
    let rows = stmt
        .query_map(params![candidate, revision], |r| r.get::<_, String>(0))
        .map_err(|e| e.to_string())?;
    let mut links = vec![];
    for row in rows {
        let receipt: DiscoveryContactLookup =
            serde_json::from_str(&row.map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
        if receipt.status == "completed" && receipt.stop_code.is_none() {
            for link in receipt.links {
                links.push((link, receipt.query.clone()));
            }
        }
    }
    Ok(links)
}

pub(super) fn blocked_host(
    connection: &Connection,
    campaign: &str,
    url: &str,
) -> Result<bool, String> {
    let Ok(target) = url::Url::parse(url) else {
        return Ok(true);
    };
    let mut stmt=connection.prepare("SELECT r.requested_url FROM discovery_public_page_reads r JOIN discovery_candidates c ON c.id=r.candidate_id
      WHERE c.discovery_campaign_id=?1 AND r.status='failed' AND r.stop_code IN ('opencli_risk_control','opencli_auth_required')").map_err(|e|e.to_string())?;
    let rows = stmt
        .query_map([campaign], |r| r.get::<_, String>(0))
        .map_err(|e| e.to_string())?;
    for row in rows {
        if url::Url::parse(&row.map_err(|e| e.to_string())?)
            .is_ok_and(|saved| saved.host_str() == target.host_str())
        {
            return Ok(true);
        }
    }
    Ok(false)
}
