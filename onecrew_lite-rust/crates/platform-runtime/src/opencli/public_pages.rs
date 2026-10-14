use super::*;
use onecrew_creator_runtime::{
    DiscoveryPublicPageInput, DiscoveryPublicPageRead, FinishDiscoveryPublicPageInput,
};

fn facebook_native_contact_author<'a>(
    saved: &'a onecrew_creator_runtime::DiscoveryProfileRead,
    claim: &onecrew_creator_runtime::DiscoveryPublicPageClaim,
    base: &str,
) -> Result<&'a str, OpenCliError> {
    let native = saved
        .native_profile
        .as_ref()
        .ok_or(OpenCliError::InvalidOutput)?;
    if saved.status!="completed" || saved.stop_code.is_some()
        || saved.claim.campaign_id!=claim.input.campaign_id || saved.claim.search_hit_id!=claim.search_hit_id
        || saved.claim.expected_revision!=claim.input.expected_revision || saved.claim.profile_ref!=claim.input.profile_ref
        || saved.claim.platform!="facebook" || saved.claim.call_number>=claim.call_number
        || native["adapter_version"]!="2.0.0" || native["observed_profile_url"].as_str()!=Some(base)
        || native["id"].as_str()!=Some(saved.claim.author_id.as_str())
        // The source is the saved vanity URL; author_url is its separately
        // verified numeric canonical identity and must not replace that source.
        || saved.claim.source_url!=claim.source_url
        || !native["contact_links"].as_array().is_some_and(|links|links.iter().any(|link|link.as_str()==Some(&claim.input.requested_url)))
    {
        return Err(OpenCliError::InvalidOutput);
    }
    Ok(&saved.claim.author_id)
}

impl LocalPlatform {
    /// Google lookup remains available; automatic contact lookup scheduling is disabled.
    pub async fn read_discovery_contact_lookup_once(
        &self,
        input: onecrew_creator_runtime::DiscoveryContactLookup,
    ) -> Result<onecrew_creator_runtime::DiscoveryContactLookup, OpenCliDiscoverySearchError> {
        if input.method != "search" {
            return Err(OpenCliDiscoverySearchError::Campaign(
                "contact_lookup_retired".into(),
            ));
        }
        let creators = self
            .creators()
            .map_err(OpenCliDiscoverySearchError::Campaign)?;
        // The transactional owner repeats score, conditions, scope, budget and no-contact checks before any provider process.
        let mut claim = creators
            .claim_discovery_contact_lookup_async(input)
            .await
            .map_err(|e| OpenCliDiscoverySearchError::Campaign(e.to_string()))?;
        let started = std::time::Instant::now();
        let mut raw_results = 0usize;
        let mut kept_results = 0usize;
        let result = {
            match OpenCli::inspect_system().await {
                Ok(cli) if cli.inventory().version == VERIFIED_OPENCLI_VERSION => cli
                    .creator_search(&claim.profile_ref, "google", &claim.query, 5)
                    .await
                    .map(|mut hits| {
                        raw_results = hits.len();
                        hits.sort_by_key(|hit| {
                            onecrew_creator_runtime::contact_search_lead_priority(
                                &claim.author_label,
                                claim.username.as_deref(),
                                &hit.source_url,
                                &hit.title,
                            )
                        });
                        let links: Vec<String> = hits
                            .into_iter()
                            .filter(|hit| {
                                onecrew_creator_runtime::contact_search_lead_matches(
                                    &claim.author_label,
                                    claim.username.as_deref(),
                                    &hit.source_url,
                                    &hit.title,
                                )
                            })
                            .map(|hit| hit.source_url)
                            .filter(|url| onecrew_opencli::validate_public_web_url(url).is_ok())
                            .collect();
                        kept_results = links.len();
                        links
                    }),
                Ok(_) => Err(OpenCliError::VersionChanged),
                Err(error) => Err(error),
            }
        };
        match result {
            Ok(links) => {
                claim.status = "completed".into();
                claim.links = links;
            }
            Err(error) => {
                claim.status = "failed".into();
                claim.stop_code = Some(provider_stop_code(&error).into());
            }
        }
        super::contact_audit::record(
            &self.data_dir,
            &claim.campaign_id,
            serde_json::json!({
                "kind":"lookup", "candidate_id":claim.candidate_id, "call_number":claim.call_number,
                "method":claim.method, "elapsed_ms":started.elapsed().as_millis() as u64,
                "raw_results":raw_results, "kept_links":kept_results, "provider_status":claim.status, "stop_code":claim.stop_code
            }),
        );
        creators
            .finish_discovery_contact_lookup_async(claim)
            .await
            .map_err(|e| OpenCliDiscoverySearchError::Campaign(e.to_string()))
    }

    /// Read a link only after the Creator owner resolves its observed source and
    /// commits a budget reservation. Page content alone is not qualification.
    pub async fn read_discovery_public_page_once(
        &self,
        input: DiscoveryPublicPageInput,
    ) -> Result<DiscoveryPublicPageRead, OpenCliDiscoverySearchError> {
        onecrew_opencli::validate_public_web_url(&input.requested_url)
            .map_err(OpenCliDiscoverySearchError::Dependency)?;
        let cli = OpenCli::inspect_system()
            .await
            .map_err(OpenCliDiscoverySearchError::Dependency)?;
        let creators = self
            .creators()
            .map_err(OpenCliDiscoverySearchError::Campaign)?;
        let campaign = creators
            .get_discovery_campaign_async(input.campaign_id.clone())
            .await
            .map_err(|e| OpenCliDiscoverySearchError::Campaign(e.to_string()))?;
        if cli.inventory().version != VERIFIED_OPENCLI_VERSION
            || campaign.opencli_version != cli.inventory().version
        {
            return Err(OpenCliDiscoverySearchError::Dependency(
                OpenCliError::VersionChanged,
            ));
        }
        let claim = creators
            .claim_discovery_public_page_async(input)
            .await
            .map_err(|e| OpenCliDiscoverySearchError::Campaign(e.to_string()))?;
        let started = std::time::Instant::now();
        let mut hidden_email_available = false;
        let result = if let Some(channel_id) = claim
            .input
            .requested_url
            .strip_prefix("https://www.youtube.com/channel/")
            .filter(|id| !id.contains('/') && !id.contains('?') && !id.contains('#'))
        {
            cli.youtube_channel_contact(&claim.input.profile_ref, channel_id)
                .await
                .and_then(|channel| {
                    let page =
                        channel_contact_page(&channel, channel_id, &claim.input.requested_url)?;
                    hidden_email_available = channel.hidden_email_available;
                    Ok(page)
                })
        } else if let Some(base) =
            onecrew_opencli::facebook_native_contact_profile(&claim.input.requested_url)
        {
            async {
                let saved = creators
                    .get_discovery_profile_async(
                        claim.input.campaign_id.clone(),
                        claim.search_hit_id.clone(),
                    )
                    .await
                    .map_err(|_| OpenCliError::InvalidOutput)?;
                let author = facebook_native_contact_author(&saved, &claim, base)?;
                cli.facebook_native_contact_page(
                    &claim.input.profile_ref,
                    author,
                    base,
                    &claim.input.requested_url,
                )
                .await
            }
            .await
        } else if let Some(author_id) =
            onecrew_opencli::facebook_contact_author(&claim.input.requested_url)
        {
            cli.facebook_contact_page(&claim.input.profile_ref, author_id)
                .await
        } else if let Some(handle) =
            onecrew_opencli::linkedin_contact_handle(&claim.input.requested_url)
        {
            async {
                let saved = creators
                    .get_discovery_profile_async(
                        claim.input.campaign_id.clone(),
                        claim.search_hit_id.clone(),
                    )
                    .await
                    .map_err(|_| OpenCliError::InvalidOutput)?;
                if saved.status != "completed"
                    || saved.stop_code.is_some()
                    || saved.claim.expected_revision != claim.input.expected_revision
                    || saved.claim.profile_ref != claim.input.profile_ref
                    || saved.claim.platform != "linkedin"
                    || saved.claim.author_id != handle
                    || saved.claim.author_url != claim.source_url
                    || saved.claim.call_number >= claim.call_number
                {
                    return Err(OpenCliError::InvalidOutput);
                }
                let native = saved
                    .native_profile
                    .as_ref()
                    .ok_or(OpenCliError::InvalidOutput)?;
                if !native["contact_links"].as_array().is_some_and(|links| {
                    links
                        .iter()
                        .any(|link| link.as_str() == Some(&claim.input.requested_url))
                }) {
                    return Err(OpenCliError::InvalidOutput);
                }
                let account = native["account_id"]
                    .as_str()
                    .ok_or(OpenCliError::InvalidOutput)?;
                cli.linkedin_contact_page(&claim.input.profile_ref, handle, account)
                    .await
            }
            .await
        } else {
            cli.read_public_web(&claim.input.profile_ref, &claim.input.requested_url)
                .await
        };
        let completion = match result {
            Ok(page) => FinishDiscoveryPublicPageInput {
                hidden_email_available,
                claim,
                document_url: Some(page.document_url),
                title: page.title,
                markdown_excerpt: page.markdown_excerpt,
                truncated: page.truncated,
                stop_code: None,
            },
            Err(error) => FinishDiscoveryPublicPageInput {
                hidden_email_available: false,
                claim,
                document_url: None,
                title: String::new(),
                markdown_excerpt: String::new(),
                truncated: false,
                stop_code: Some(provider_stop_code(&error).into()),
            },
        };
        super::contact_audit::record(
            &self.data_dir,
            &completion.claim.input.campaign_id,
            serde_json::json!({
                "kind":"page", "candidate_id":completion.claim.input.candidate_id, "call_number":completion.claim.call_number,
                "elapsed_ms":started.elapsed().as_millis() as u64, "provider_status":if completion.stop_code.is_none() {"completed"} else {"failed"},
                "stop_code":completion.stop_code
            }),
        );
        creators
            .finish_discovery_public_page_async(completion)
            .await
            .map_err(|e| OpenCliDiscoverySearchError::Campaign(e.to_string()))
    }
}

fn channel_contact_page(
    channel: &onecrew_opencli::YoutubeChannelContact,
    channel_id: &str,
    requested_url: &str,
) -> Result<onecrew_opencli::PublicWebRead, OpenCliError> {
    if channel.channel_id != channel_id {
        return Err(OpenCliError::InvalidOutput);
    }
    // The adapter already validates the exact source and native author. Missing
    // deferred about data leaves partial biography evidence, not malformed output.
    Ok(onecrew_opencli::PublicWebRead {
        requested_url: requested_url.into(),
        document_url: requested_url.into(),
        title: channel.title.clone(),
        truncated: true,
        markdown_excerpt: format!("{}\n{}", channel.title, channel.description),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn facebook_native_contact_dispatch_binds_saved_vanity_source_and_rejects_scope_drift() {
        use onecrew_creator_runtime::{
            DiscoveryProfileClaim, DiscoveryProfileRead, DiscoveryPublicPageClaim,
        };
        let base = "https://www.facebook.com/LinusTech";
        let requested = format!("{base}/directory_contact_info");
        let saved = DiscoveryProfileRead {
            claim: DiscoveryProfileClaim {
                campaign_id: "qa".into(),
                expected_revision: 1,
                search_hit_id: "hit".into(),
                platform: "facebook".into(),
                profile_ref: "browser".into(),
                author_id: "100044572203205".into(),
                author_url: "https://www.facebook.com/profile.php?id=100044572203205".into(),
                source_url: base.into(),
                call_number: 3,
            },
            status: "completed".into(),
            stop_code: None,
            captured_at: 1,
            native_profile: Some(
                serde_json::json!({"adapter_version":"2.0.0","id":"100044572203205","observed_profile_url":base,"contact_links":[requested]}),
            ),
        };
        let claim = DiscoveryPublicPageClaim {
            input: DiscoveryPublicPageInput {
                campaign_id: "qa".into(),
                expected_revision: 1,
                profile_ref: "browser".into(),
                candidate_id: "candidate".into(),
                requested_url: requested,
            },
            search_hit_id: "hit".into(),
            source_url: base.into(),
            call_number: 7,
        };
        assert_ne!(saved.claim.author_url, claim.source_url);
        assert_eq!(
            facebook_native_contact_author(&saved, &claim, base).unwrap(),
            "100044572203205"
        );
        for changed in 0..7 {
            let mut bad = claim.clone();
            match changed {
                0 => bad.source_url = saved.claim.author_url.clone(),
                1 => bad.input.profile_ref = "other".into(),
                2 => bad.input.expected_revision = 2,
                3 => bad.input.campaign_id = "other".into(),
                4 => bad.search_hit_id = "other".into(),
                5 => bad.call_number = 3,
                _ => bad.input.requested_url = format!("{base}/directory_links"),
            }
            assert!(facebook_native_contact_author(&saved, &bad, base).is_err());
        }
        assert!(
            facebook_native_contact_author(&saved, &claim, "https://www.facebook.com/Other")
                .is_err()
        );
    }
    #[test]
    fn missing_about_panel_preserves_partial_bio_without_inventing_email() {
        let id = "UCabcdefghijklmnopqrstuv";
        let url = format!("https://www.youtube.com/channel/{id}");
        let mut channel = onecrew_opencli::YoutubeChannelContact {
            adapter_version: "1.0.0".into(),
            channel_id: id.into(),
            source_url: url.clone(),
            title: "Creator".into(),
            description: "Keyboard reviews".into(),
            hidden_email_available: false,
            about_data_available: false,
        };
        let page = channel_contact_page(&channel, id, &url).unwrap();
        assert!(page.truncated);
        assert_eq!(page.markdown_excerpt, "Creator\nKeyboard reviews");
        assert!(!channel.hidden_email_available);
        channel.channel_id = "UCotherabcdefghijklmnopq".into();
        assert!(matches!(
            channel_contact_page(&channel, id, &url),
            Err(OpenCliError::InvalidOutput)
        ));
    }
    use onecrew_creator_runtime::{
        CreateDiscoveryCampaignInput, DiscoveryPlatformScope, ListDiscoveryCandidatesInput,
        ListDiscoverySearchHitsInput,
    };

    #[tokio::test]
    async fn contact_lookup_rejects_unscored_candidate_before_any_provider_process() {
        let directory = tempfile::tempdir().unwrap();
        let platform = LocalPlatform::open(directory.path()).unwrap();
        let result = platform
            .read_discovery_contact_lookup_once(onecrew_creator_runtime::DiscoveryContactLookup {
                campaign_id: "not-approved".into(),
                expected_revision: 1,
                candidate_id: "not-scored".into(),
                profile_ref: "dedicated".into(),
                method: "maigret".into(),
                native_author_id: "author".into(),
                author_label: "Creator".into(),
                username: Some("creator".into()),
                query: "creator email".into(),
                call_number: 0,
                status: "planned".into(),
                links: vec![],
                stop_code: None,
            })
            .await;
        assert!(matches!(
            result,
            Err(OpenCliDiscoverySearchError::Campaign(_))
        ));
    }

    #[tokio::test]
    async fn public_page_runtime_rejects_unsafe_target_before_dependency_or_budget() {
        let directory = tempfile::tempdir().unwrap();
        let platform = LocalPlatform::open(directory.path()).unwrap();
        let result = platform
            .read_discovery_public_page_once(DiscoveryPublicPageInput {
                campaign_id: "not-created".into(),
                expected_revision: 1,
                profile_ref: "dedicated".into(),
                candidate_id: "not-created".into(),
                requested_url: "https://127.0.0.1/".into(),
            })
            .await;
        assert!(matches!(
            result,
            Err(OpenCliDiscoverySearchError::Dependency(
                OpenCliError::InvalidInput("public_url")
            ))
        ));
    }

    #[tokio::test]
    #[ignore = "live bounded Campaign requires dedicated OPENCLI_TEST_PROFILE and an observed complete tool URL"]
    async fn public_page_campaign_reads_only_a_real_observed_tool_link_and_reopens_receipt() {
        let profile = std::env::var("OPENCLI_TEST_PROFILE").expect("explicit dedicated profile");
        let directory = tempfile::tempdir().unwrap();
        let platform = LocalPlatform::open(directory.path()).unwrap();
        let owner = platform.creators().unwrap();
        let id = "public-page-live";
        owner.create_discovery_campaign(CreateDiscoveryCampaignInput {
            id:id.into(),brief:"Source-bound AI tool link evidence probe; not qualification acceptance".into(),
            opencli_version:VERIFIED_OPENCLI_VERSION.into(),
            criteria:json!({"min_followers":10000,"requires_ai_tool_demo":true,"requires_public_tool_url":true}),
            scopes:vec![DiscoveryPlatformScope {platform:"youtube".into(),profile_ref:profile.clone(),keywords:vec!["Descript tutorial".into()]}],
            target_count:30,max_calls: Some(6),
        }).unwrap();
        owner.approve_discovery_campaign(id, 1).unwrap();
        platform
            .search_discovery_once(id, 1, "youtube", &profile, 3)
            .await
            .unwrap();
        let hits = owner
            .list_discovery_search_hits(ListDiscoverySearchHitsInput {
                campaign_id: id.into(),
                after_id: None,
                limit: 3,
            })
            .unwrap();
        let mut selected = None;
        println!("source_bound_public_page search_hits={}", hits.len());
        for hit in hits {
            platform
                .verify_discovery_youtube_hit_once(id, 1, &profile, &hit.id)
                .await
                .unwrap();
            let counts = owner.count_discovery_candidates(id).unwrap();
            println!(
                "metadata_read used_calls={} pending={} rejected={} qualified={}",
                owner.get_discovery_campaign(id).unwrap().used_calls,
                counts.pending,
                counts.rejected,
                counts.qualified
            );
            let candidates = owner
                .list_discovery_candidates(ListDiscoveryCandidatesInput {
                    campaign_id: id.into(),
                    after_id: None,
                    limit: 50,
                })
                .unwrap();
            for candidate in candidates.into_iter().filter(|c| c.verdict == "pending") {
                let Some(links) = candidate
                    .evidence
                    .get("public_links")
                    .and_then(serde_json::Value::as_array)
                else {
                    continue;
                };
                if let Some(link) =
                    links
                        .iter()
                        .filter_map(serde_json::Value::as_str)
                        .find(|link| {
                            onecrew_opencli::validate_public_web_url(link).is_ok()
                                && url::Url::parse(link).is_ok_and(|url| {
                                    matches!(
                                        url.host_str(),
                                        Some("descript.com" | "www.descript.com")
                                    )
                                })
                        })
                {
                    selected = Some((candidate.id.clone(), link.to_owned()));
                    break;
                }
            }
            if selected.is_some() {
                break;
            }
        }
        let (candidate_id, requested_url) = selected.expect(
            "no complete observed Descript tool link on a pending candidate; never guess one",
        );
        let before = owner.get_discovery_campaign(id).unwrap().used_calls;
        let read = platform
            .read_discovery_public_page_once(DiscoveryPublicPageInput {
                campaign_id: id.into(),
                expected_revision: 1,
                profile_ref: profile,
                candidate_id: candidate_id.clone(),
                requested_url: requested_url.clone(),
            })
            .await
            .unwrap();
        assert_eq!(
            read.status, "completed",
            "live tool page must actually return content"
        );
        assert!(read.source_excerpt.contains(&requested_url));
        assert!(!read.markdown_excerpt.trim().is_empty());
        assert_eq!(
            owner.get_discovery_campaign(id).unwrap().used_calls,
            before + 1
        );
        assert_eq!(owner.count_discovery_candidates(id).unwrap().qualified, 0);
        drop(owner);
        drop(platform);
        let reopened = LocalPlatform::open(directory.path()).unwrap();
        assert_eq!(
            reopened
                .creators()
                .unwrap()
                .get_discovery_public_page(id, &candidate_id, &requested_url)
                .unwrap(),
            read
        );
        println!(
            "source_bound_public_page used_calls={} qualified=0 excerpt_bytes={} truncated={}",
            before + 1,
            read.markdown_excerpt.len(),
            read.truncated
        );
    }
}
