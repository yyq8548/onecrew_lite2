use crate::LocalPlatform;
use onecrew_creator_runtime::{
    DiscoveryContentClaim, DiscoveryContentRead, DiscoveryLanguageClaim, DiscoveryLanguageSample,
};
use onecrew_opencli::{OpenCli, OpenCliError};

enum LanguageReadFailure {
    Provider(OpenCliError),
    SourcesExhausted,
}

impl From<OpenCliError> for LanguageReadFailure {
    fn from(error: OpenCliError) -> Self {
        Self::Provider(error)
    }
}

impl LocalPlatform {
    pub(crate) async fn read_discovery_language(
        &self,
        claim: DiscoveryLanguageClaim,
    ) -> Result<DiscoveryContentRead, String> {
        let result: Result<DiscoveryLanguageSample, LanguageReadFailure> = async {
            let cli = OpenCli::inspect_system().await?;
            if claim.ordinal != 1 || !claim.previous_urls.is_empty() {
                return Err(LanguageReadFailure::SourcesExhausted);
            }
            let source = claim.root_url.clone();
            let id = source
                .rsplit('/')
                .next()
                .ok_or(OpenCliError::InvalidOutput)?;
            let value = if claim.platform == "douyin" {
                let owner = self.creators().map_err(|_| OpenCliError::InvalidOutput)?;
                let saved = owner.get_discovery_content_async(claim.campaign_id.clone(),claim.search_hit_id.clone()).await.map_err(|_|OpenCliError::InvalidOutput)?;
                let joint = owner.get_discovery_source_author_async(claim.campaign_id.clone(),claim.search_hit_id.clone()).await.map_err(|_|OpenCliError::InvalidOutput)?;
                let native=saved.native_content.as_ref().ok_or(OpenCliError::InvalidOutput)?;
                if saved.status!="completed" || saved.stop_code.is_some() || joint.status!="completed" || joint.stop_code.is_some() || joint.duplicate_of.is_some()
                    || saved.claim.expected_revision!=claim.revision || joint.claim.expected_revision!=claim.revision
                    || saved.claim.platform!=claim.platform || saved.claim.profile_ref!=claim.profile_ref || joint.claim.profile_ref!=claim.profile_ref
                    || saved.claim.author_id!=claim.author_id || joint.author_id.as_deref()!=Some(claim.author_id.as_str())
                    || saved.claim.call_number!=joint.claim.call_number || saved.claim.call_number>=claim.call_number
                    || saved.observed_source_url()!=Some(source.as_str()) || joint.claim.source_url!=source {
                    return Err(OpenCliError::InvalidOutput.into());
                }
                let window=cli.douyin_original_comments(&claim.profile_ref,&joint.claim.post_id,&claim.author_id).await?;
                if native["text"].as_str()!=Some(window.text.as_str()) || native["text_truncated"].as_bool()!=Some(window.text_truncated) {
                    return Err(OpenCliError::InvalidOutput.into());
                }
                serde_json::json!({"author_id":claim.author_id,"source_url":source,"text":window.text,
                    "comments":window.comments.into_iter().map(|c|serde_json::json!({"source_url":format!("{source}#comment-{}",c.id),"text":c.text})).collect::<Vec<_>>()})
            } else if claim.platform == "linkedin" {
                let owner = self.creators().map_err(|_| OpenCliError::InvalidOutput)?;
                let body = owner.get_discovery_content_async(claim.campaign_id.clone(), claim.search_hit_id.clone()).await.map_err(|_| OpenCliError::InvalidOutput)?;
                let profile = owner.get_discovery_profile_async(claim.campaign_id.clone(), claim.search_hit_id.clone()).await.map_err(|_| OpenCliError::InvalidOutput)?;
                let native = body.native_content.as_ref().ok_or(OpenCliError::InvalidOutput)?;
                let identity = profile.native_profile.as_ref().ok_or(OpenCliError::InvalidOutput)?;
                let canonical = format!("https://www.linkedin.com/in/{}/", claim.author_handle);
                if body.status != "completed" || body.stop_code.is_some() || profile.status != "completed" || profile.stop_code.is_some()
                    || body.claim.expected_revision != claim.revision || profile.claim.expected_revision != claim.revision
                    || body.claim.platform != claim.platform || profile.claim.platform != claim.platform
                    || body.claim.profile_ref != claim.profile_ref || profile.claim.profile_ref != claim.profile_ref
                    || body.claim.author_id != claim.author_handle || profile.claim.author_id != claim.author_handle
                    || body.claim.source_url != canonical || profile.claim.source_url != canonical
                    || profile.claim.call_number >= body.claim.call_number || body.claim.call_number >= claim.call_number
                    || body.observed_source_url() != Some(source.as_str())
                    || native["account_id"].as_str() != Some(claim.author_id.as_str())
                    || identity["account_id"].as_str() != Some(claim.author_id.as_str()) {
                    return Err(OpenCliError::InvalidOutput.into());
                }
                let text = native["text"].as_str().ok_or(OpenCliError::InvalidOutput)?;
                let comments = cli.linkedin_original_comments(&claim.profile_ref, &claim.author_handle, &claim.author_id, &source).await?;
                serde_json::json!({"author_id":claim.author_id,"source_url":source,"text":text,
                    "comments":comments.into_iter().map(|c| serde_json::json!({"source_url":format!("{source}#comment-{}",c.id),"text":c.text})).collect::<Vec<_>>()})
            } else if claim.platform == "hackernews" {
                let saved = self.creators().map_err(|_| OpenCliError::InvalidOutput)?
                    .get_discovery_content_async(claim.campaign_id.clone(), claim.search_hit_id.clone()).await.map_err(|_| OpenCliError::InvalidOutput)?;
                if saved.status != "completed" || saved.stop_code.is_some()
                    || saved.claim.expected_revision != claim.revision || saved.claim.platform != claim.platform
                    || saved.claim.profile_ref != claim.profile_ref || saved.claim.author_id != claim.author_id
                    || saved.claim.call_number >= claim.call_number || saved.observed_source_url() != Some(source.as_str()) {
                    return Err(OpenCliError::InvalidOutput.into());
                }
                let native = saved.native_content.as_ref().ok_or(OpenCliError::InvalidOutput)?;
                let item_id = native["item_id"].as_u64().ok_or(OpenCliError::InvalidOutput)?;
                let item = cli.hackernews_original_comments(&claim.profile_ref, item_id).await?;
                if item.author_id != claim.author_id || item.source_url != source
                    || native["text"].as_str() != Some(item.text.as_str())
                    || native["comment_ids"] != serde_json::to_value(&item.comment_ids).map_err(|_| OpenCliError::InvalidOutput)? {
                    return Err(OpenCliError::InvalidOutput.into());
                }
                serde_json::json!({"author_id":item.author_id,"source_url":item.source_url,"text":item.text,
                    "comments":item.comments.into_iter().map(|comment| serde_json::json!({"source_url":comment.source_url,"text":comment.text})).collect::<Vec<_>>()})
            } else if claim.platform == "zhihu" {
                let saved = self.creators().map_err(|_| OpenCliError::InvalidOutput)?
                    .get_discovery_content_async(claim.campaign_id.clone(), claim.search_hit_id.clone()).await.map_err(|_| OpenCliError::InvalidOutput)?;
                if saved.claim.expected_revision != claim.revision || saved.claim.platform != claim.platform
                    || saved.claim.profile_ref != claim.profile_ref || saved.claim.author_id != claim.author_id
                    || saved.claim.call_number >= claim.call_number || saved.observed_source_url() != Some(source.as_str()) {
                    return Err(OpenCliError::InvalidOutput.into());
                }
                let text = saved.native_content.as_ref().and_then(|v| v["text"].as_str()).ok_or(OpenCliError::InvalidOutput)?;
                let comments = cli.zhihu_original_comments(&claim.profile_ref, &source).await?;
                serde_json::json!({"author_id":claim.author_id,"source_url":source,"text":text,
                    "comments":comments.into_iter().map(|c| serde_json::json!({"source_url":format!("{}#comment-{}",source,c.id),"text":c.text})).collect::<Vec<_>>()})
            } else if claim.platform == "bluesky" {
                let saved=self.creators().map_err(|_|OpenCliError::InvalidOutput)?
                    .get_discovery_content_async(claim.campaign_id.clone(),claim.search_hit_id.clone()).await.map_err(|_|OpenCliError::InvalidOutput)?;
                if saved.claim.expected_revision!=claim.revision || saved.claim.platform!=claim.platform
                    || saved.claim.profile_ref!=claim.profile_ref || saved.claim.author_id!=claim.author_id
                    || saved.claim.call_number>=claim.call_number || saved.observed_source_url()!=Some(source.as_str()) {
                    return Err(OpenCliError::InvalidOutput.into());
                }
                let native=saved.native_content.as_ref().ok_or(OpenCliError::InvalidOutput)?;
                let uri=native["post_uri"].as_str().ok_or(OpenCliError::InvalidOutput)?;
                let evidence=cli.bluesky_original_comments(&claim.profile_ref,uri).await?;
                if evidence.author_id!=claim.author_id || evidence.source_url!=source || evidence.text!=native["text"].as_str().ok_or(OpenCliError::InvalidOutput)? {
                    return Err(OpenCliError::InvalidOutput.into());
                }
                serde_json::json!({"author_id":claim.author_id,"source_url":source,"text":evidence.text,
                    "comments":evidence.comments.into_iter().map(|c|serde_json::json!({"source_url":format!("{source}#comment-{}",c.post_uri),"text":c.text})).collect::<Vec<_>>()})
            } else if claim.platform == "instagram" {
                // Reuse already saved native caption/identity. This reservation
                // performs only the one bounded comments request, not a hidden
                // second feed/profile read.
                let saved = self.creators().map_err(|_| OpenCliError::InvalidOutput)?.get_discovery_content_async(claim.campaign_id.clone(), claim.search_hit_id.clone())
                    .await.map_err(|_| OpenCliError::InvalidOutput)?;
                if saved.claim.author_id != claim.author_id
                    || saved.claim.expected_revision != claim.revision
                    || saved.claim.platform != claim.platform
                    || saved.claim.profile_ref != claim.profile_ref
                    || saved.claim.call_number >= claim.call_number
                    || saved.observed_source_url() != Some(source.as_str()) {
                    return Err(OpenCliError::InvalidOutput.into());
                }
                let text = saved.native_content.as_ref().and_then(|v| v["text"].as_str()).ok_or(OpenCliError::InvalidOutput)?;
                let evidence = cli.instagram_comment_language_evidence(&claim.profile_ref, &claim.author_handle, &source).await?;
                serde_json::json!({"author_id":claim.author_id,"source_url":source,"text":text,"comments":evidence["comments"]})
            } else if claim.platform == "weibo" {
                // Use the saved numeric API ID, not its base62 share code.
                let saved=self.creators().map_err(|_|OpenCliError::InvalidOutput)?
                    .get_discovery_content_async(claim.campaign_id.clone(),claim.search_hit_id.clone()).await.map_err(|_|OpenCliError::InvalidOutput)?;
                if saved.claim.expected_revision!=claim.revision || saved.claim.platform!=claim.platform
                    || saved.claim.profile_ref!=claim.profile_ref || saved.claim.author_id!=claim.author_id
                    || saved.claim.call_number>=claim.call_number || saved.observed_source_url()!=Some(source.as_str()) {
                    return Err(OpenCliError::InvalidOutput.into());
                }
                let native=saved.native_content.as_ref().ok_or(OpenCliError::InvalidOutput)?;
                let post_id=native["post_id"].as_str().ok_or(OpenCliError::InvalidOutput)?;
                let text=native["text"].as_str().ok_or(OpenCliError::InvalidOutput)?;
                let comments=cli.weibo_original_comments(&claim.profile_ref,post_id).await?;
                // Provenance anchors identify comments within their observed
                // parent source, without claiming separately fetched pages.
                serde_json::json!({"author_id":claim.author_id,"source_url":source,"text":text,
                    "comments":comments.into_iter().map(|c|serde_json::json!({"source_url":format!("{source}#comment-{}",c.id),"text":c.text})).collect::<Vec<_>>()})
            } else if claim.platform == "facebook" {
                serde_json::to_value(
                    cli.facebook_language_sample(&claim.profile_ref, &claim.author_id, &source)
                        .await?,
                )
                .map_err(|_| OpenCliError::InvalidOutput)?
            } else if claim.platform == "reddit" {
                cli.reddit_language_sample(
                    &claim.profile_ref,
                    &claim.author_handle,
                    &claim.author_id,
                    &source,
                )
                .await?
            } else if claim.platform == "tiktok" {
                cli.tiktok_language_sample(
                    &claim.profile_ref,
                    &claim.author_handle,
                    &claim.author_id,
                    &source,
                )
                .await?
            } else if claim.platform == "bilibili" {
                cli.bilibili_language_sample(&claim.profile_ref, &claim.author_id, &source)
                    .await?
            } else if matches!(claim.platform.as_str(), "google" | "yahoo") {
                let owner = self.creators().map_err(|_| OpenCliError::InvalidOutput)?;
                let saved = owner.get_discovery_web_source_async(claim.campaign_id.clone(), claim.search_hit_id.clone())
                    .await.map_err(|_| OpenCliError::InvalidOutput)?;
                if saved.status != "completed" || saved.stop_code.is_some() || !saved.is_primary_source
                    || saved.claim.expected_revision != claim.revision || saved.claim.origin_platform != claim.platform
                    || saved.claim.profile_ref != claim.profile_ref || !matches!(saved.claim.target_platform.as_str(),"youtube"|"twitter"|"douyin")
                    || saved.native_author_id.as_deref() != Some(claim.author_id.as_str())
                    || saved.candidate_id.as_deref() != Some(claim.candidate_id.as_str())
                    || saved.claim.call_number >= claim.call_number
                    || source != if matches!(saved.claim.target_platform.as_str(),"twitter"|"douyin") {saved.claim.source_url.clone()} else {format!("https://www.youtube.com/watch?v={}",saved.claim.video_id)}
                {
                    return Err(OpenCliError::InvalidOutput.into());
                }
                if saved.claim.target_platform=="douyin" {
                    if claim.author_handle != claim.author_id { return Err(OpenCliError::InvalidOutput.into()); }
                    let window=cli.douyin_original_comments(&claim.profile_ref,&saved.claim.video_id,&claim.author_id).await?;
                    let original=saved.native_material.as_ref().ok_or(OpenCliError::InvalidOutput)?;
                    if original["content"]["text"].as_str()!=Some(window.text.as_str())
                        || original["content"]["text_truncated"].as_bool()!=Some(window.text_truncated) {
                        return Err(OpenCliError::InvalidOutput.into());
                    }
                    serde_json::json!({"author_id":claim.author_id,"source_url":source,"text":window.text,
                        "comments":window.comments.into_iter().map(|c|serde_json::json!({"source_url":format!("{source}#comment-{}",c.id),"text":c.text})).collect::<Vec<_>>()})
                } else if saved.claim.target_platform=="twitter" {
                    let (handle,post)=onecrew_creator_runtime::web_source_twitter_target(&source).ok_or(OpenCliError::InvalidOutput)?;
                    if handle != claim.author_handle { return Err(OpenCliError::InvalidOutput.into()); }
                    cli.twitter_language_sample(&claim.profile_ref,&handle,&post,&claim.author_id).await?
                } else { cli.youtube_language_sample(&claim.profile_ref, &claim.author_id, &saved.claim.video_id).await? }
            } else if claim.platform == "youtube" {
                cli.youtube_language_sample(&claim.profile_ref, &claim.author_id, &source)
                    .await?
            } else {
                cli.twitter_language_sample(
                    &claim.profile_ref,
                    &claim.author_handle,
                    id,
                    &claim.author_id,
                )
                .await?
            };
            let mut sample = serde_json::from_value::<DiscoveryLanguageSample>(value)
                .map_err(|_| OpenCliError::InvalidOutput)?;
            // Preserve observed order: later replies cannot replace an unusable
            // member of the first five or provide additional language votes.
            sample.comments.truncate(5);
            if !sample.valid_for(&claim) {
                return Err(OpenCliError::InvalidOutput.into());
            }
            Ok(sample)
        }
        .await;
        let (sample, stop) = match result {
            Ok(sample) => (Some(sample), None),
            Err(LanguageReadFailure::SourcesExhausted) => {
                (None, Some("language_sources_exhausted".into()))
            }
            Err(LanguageReadFailure::Provider(error)) => (
                None,
                Some(crate::opencli::provider_stop_code(&error).into()),
            ),
        };
        // A supplement has its own receipt. Returning the first content read
        // would conceal this attempt's failure and repeat its old call number.
        let receipt = DiscoveryContentRead {
            claim: DiscoveryContentClaim {
                campaign_id: claim.campaign_id.clone(),
                expected_revision: claim.revision,
                search_hit_id: claim.search_hit_id.clone(),
                platform: claim.platform.clone(),
                profile_ref: claim.profile_ref.clone(),
                author_id: claim.author_id.clone(),
                source_url: sample
                    .as_ref()
                    .map(|s| s.source_url.clone())
                    .unwrap_or_else(|| claim.root_url.clone()),
                call_number: claim.call_number,
            },
            status: if sample.is_some() {
                "completed"
            } else {
                "failed"
            }
            .into(),
            native_content: None,
            language_sample: sample.clone(),
            stop_code: stop.clone(),
            captured_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_err(|e| e.to_string())?
                .as_secs() as i64,
        };
        let creators = self.creators()?;
        creators
            .finish_discovery_language_async(claim, sample, stop)
            .await
            .map_err(|e| e.to_string())?;
        Ok(receipt)
    }
}
