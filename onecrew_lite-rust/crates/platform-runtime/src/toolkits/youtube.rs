//! YouTube toolkit functions over the `onecrew_youtube` facade, read through a
//! stored service credential.
//!
//! A read validates its input with YouTube's own checks, then delegates once
//! to the YouTube credential read: a bound search is checked before
//! admission, the credential is admitted and opened on the blocking pool, the
//! search is checked again, YouTube is called, and the read's credential
//! health is committed on the blocking pool. A failed YouTube call crosses as
//! YouTube's evidence only once its health outcome is committed, so it is
//! reported after that local commit. Peer-facing reads return platform-native
//! data; runtime orchestration keeps the typed YouTube models.

use std::{future::Future, sync::Arc, time::Instant};

pub use onecrew_youtube::{
    PUBLISHED_AFTER_FORMAT, SEARCH_DEFAULT_COUNT, SEARCH_MAX_COUNT, SEARCH_MIN_COUNT,
    YoutubeReadDepth, YoutubeSearchOrder, accepts_published_after, accepts_search_count,
    accepts_search_query,
};
use onecrew_youtube::{
    YoutubeChannelSummary, YoutubeCreator, YoutubePlatform, YoutubePlatformError,
    YoutubeSearchInput, YoutubeSearchKind, accepts_channel_id,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::{
    PlatformNativeData, PlatformReadError,
    read::{BoundSearch, platform_native_data},
};
use crate::{
    BoundSearchAuthorization, LocalPlatform, PlatformConnectionResolutionError,
    PlatformConnectionSelector,
    credentials::services::{
        CredentialServices, YoutubeCredentialRead, YoutubeCredentialReadError, blocking_segment,
    },
};

/// Inputs for YouTube channel search, including its native optional criteria.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChannelsSearchInput {
    pub credential_id: String,
    pub query: String,
    pub count: u8,
    /// How YouTube ranks the page. Named on every search, like `count`, so
    /// one confirmed plan scope has one spelling.
    pub order: YoutubeSearchOrder,
    /// Optional publication bound in [`PUBLISHED_AFTER_FORMAT`].
    pub published_after: Option<String>,
    pub cursor: Option<String>,
    pub region_code: Option<String>,
    pub language: Option<String>,
}

/// Inputs for YouTube video search. A page can contain multiple videos by
/// the same channel, each retaining its provider identity and position.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VideosSearchInput {
    pub credential_id: String,
    pub query: String,
    pub count: u8,
    /// How YouTube ranks the page. With [`YoutubeSearchOrder::Date`] the
    /// page's own `matched.published_at` is recent-activity evidence, so a
    /// caller need not read each channel to learn whether it still posts.
    pub order: YoutubeSearchOrder,
    /// Optional publication bound in [`PUBLISHED_AFTER_FORMAT`].
    pub published_after: Option<String>,
    pub cursor: Option<String>,
    pub region_code: Option<String>,
    pub language: Option<String>,
}

/// Reads a channel search page without channel-get enrichment, as
/// platform-native data.
///
/// # Errors
///
/// Returns [`PlatformReadError`] for an empty query, a count outside
/// [`SEARCH_MIN_COUNT`]`..=`[`SEARCH_MAX_COUNT`], a publication bound outside
/// [`PUBLISHED_AFTER_FORMAT`], a refused search authorization, an unadmitted
/// or unavailable credential, a YouTube failure after its health commit, or
/// an unknown health commit outcome.
pub async fn channels_search(
    platform: &LocalPlatform,
    input: ChannelsSearchInput,
    authorization: BoundSearchAuthorization,
) -> Result<PlatformNativeData, PlatformReadError> {
    validate_search_input(&input.query, input.count, input.published_after.as_deref())?;
    let input = &input;
    let page = credential_read(
        platform,
        &input.credential_id,
        Some(BoundSearch::new(&authorization, input)),
        |youtube| async move {
            youtube
                .search(YoutubeSearchInput {
                    query: input.query.clone(),
                    kind: YoutubeSearchKind::Channel,
                    max_results: input.count,
                    order: input.order,
                    published_after: input.published_after.clone(),
                    region_code: input.region_code.clone(),
                    language: input.language.clone(),
                    page_token: input.cursor.clone(),
                })
                .await
        },
    )
    .await?;
    platform_native_data(&page)
}

/// Reads a video search page with every matching video retained, as
/// platform-native data.
///
/// # Errors
///
/// Returns [`PlatformReadError`] for an empty query, a count outside
/// [`SEARCH_MIN_COUNT`]`..=`[`SEARCH_MAX_COUNT`], a publication bound outside
/// [`PUBLISHED_AFTER_FORMAT`], a refused search authorization, an unadmitted
/// or unavailable credential, a YouTube failure after its health commit, or
/// an unknown health commit outcome.
pub async fn videos_search(
    platform: &LocalPlatform,
    input: VideosSearchInput,
    authorization: BoundSearchAuthorization,
) -> Result<PlatformNativeData, PlatformReadError> {
    validate_search_input(&input.query, input.count, input.published_after.as_deref())?;
    let input = &input;
    let page = credential_read(
        platform,
        &input.credential_id,
        Some(BoundSearch::new(&authorization, input)),
        |youtube| async move {
            youtube
                .search(YoutubeSearchInput {
                    query: input.query.clone(),
                    kind: YoutubeSearchKind::Video,
                    max_results: input.count,
                    order: input.order,
                    published_after: input.published_after.clone(),
                    region_code: input.region_code.clone(),
                    language: input.language.clone(),
                    page_token: input.cursor.clone(),
                })
                .await
        },
    )
    .await?;
    platform_native_data(&page)
}

/// One YouTube channel by its stable channel id as platform-native data: the
/// channel, its About-page links, recent videos and public comments as the
/// client returns them.
///
/// # Errors
///
/// Returns [`PlatformReadError`] when the channel id is not a stable UC
/// identifier, the credential is not admitted or cannot be opened, YouTube
/// fails the read, the health outcome cannot be committed, or the channel is
/// not a JSON object.
pub async fn channel(
    platform: &LocalPlatform,
    credential_id: &str,
    channel_id: &str,
    depth: YoutubeReadDepth,
) -> Result<PlatformNativeData, PlatformReadError> {
    platform_native_data(&channel_creator(platform, credential_id, channel_id, depth).await?)
}

/// One YouTube channel by its stable channel id as the typed YouTube model,
/// for runtime orchestration: Creator save reads.
///
/// # Errors
///
/// Returns [`PlatformReadError`] when the channel id is not a stable UC
/// identifier, the credential is not admitted or cannot be opened, YouTube
/// fails the read, or the health outcome cannot be committed.
pub(crate) async fn channel_creator(
    platform: &LocalPlatform,
    credential_id: &str,
    channel_id: &str,
    depth: YoutubeReadDepth,
) -> Result<YoutubeCreator, PlatformReadError> {
    let channel_id = channel_id.trim();
    if !accepts_channel_id(channel_id) {
        return Err(PlatformReadError::InputInvalid(
            "YouTube channel id must be a stable 24-character UC identifier",
        ));
    }
    let depth_label = match depth {
        YoutubeReadDepth::Light => "light",
        YoutubeReadDepth::Full => "full",
    };
    credential_read_with_depth(
        platform,
        credential_id,
        depth_label,
        CredentialServices::youtube_platform_for_read,
        None,
        |youtube| async move { youtube.creator_by_channel_id(channel_id, depth).await },
    )
    .await
}

/// Resolve a public handle to the provider's minimal channel summary, for
/// outreach target resolution. Credential admission and health recording are
/// the same as other toolkit reads.
///
/// # Errors
///
/// Returns [`PlatformReadError`] for an empty handle, an unadmitted or
/// unavailable credential, a failed YouTube lookup, or an unknown health
/// commit outcome.
pub(crate) async fn resolve_handle(
    platform: &LocalPlatform,
    credential_id: &str,
    handle: &str,
) -> Result<YoutubeChannelSummary, PlatformReadError> {
    let handle = handle.trim();
    if handle.is_empty() {
        return Err(PlatformReadError::InputInvalid(
            "YouTube public handle is required",
        ));
    }
    credential_read_with_depth(
        platform,
        credential_id,
        "handle",
        CredentialServices::youtube_platform_for_read,
        None,
        |youtube| async move { youtube.resolve_handle(handle).await },
    )
    .await
}

/// One YouTube read through a stored credential after its input was
/// validated: a bound search is checked before admission, the credential is
/// admitted and opened on the blocking pool, the search is checked again, and
/// `read` runs with the opened facade. The outcome is committed to the
/// credential's health on the blocking pool before this returns.
///
/// # Errors
///
/// Returns every [`PlatformReadError`] local outcome of these steps,
/// [`PlatformReadError::Platform`] after its local commit when `read` fails,
/// and [`PlatformReadError::CredentialHealthCommitUnknown`] when the health
/// outcome cannot be committed.
async fn credential_read<T, Fut>(
    platform: &LocalPlatform,
    credential_id: &str,
    search: Option<BoundSearch<'_>>,
    read: impl FnOnce(YoutubePlatform) -> Fut,
) -> Result<T, PlatformReadError>
where
    T: Send + 'static,
    Fut: Future<Output = Result<T, YoutubePlatformError>>,
{
    credential_read_with_depth(
        platform,
        credential_id,
        "unspecified",
        CredentialServices::youtube_platform_for_read,
        search,
        read,
    )
    .await
}

async fn credential_read_with_depth<T, Fut>(
    platform: &LocalPlatform,
    credential_id: &str,
    read_depth: &'static str,
    open: OpenCredential,
    search: Option<BoundSearch<'_>>,
    read: impl FnOnce(YoutubePlatform) -> Fut,
) -> Result<T, PlatformReadError>
where
    T: Send + 'static,
    Fut: Future<Output = Result<T, YoutubePlatformError>>,
{
    credential_read_opened_by_with_depth(platform, credential_id, read_depth, open, search, read)
        .await
}

/// Opens an admitted YouTube credential for a read on the blocking pool.
type OpenCredential =
    fn(&CredentialServices, &str) -> Result<YoutubeCredentialRead, YoutubeCredentialReadError>;

/// [`credential_read`] with the step that opens the admitted credential
/// supplied, so a test can observe when the credential opens.
#[cfg(test)]
async fn credential_read_opened_by<T, Fut>(
    platform: &LocalPlatform,
    credential_id: &str,
    open: OpenCredential,
    search: Option<BoundSearch<'_>>,
    read: impl FnOnce(YoutubePlatform) -> Fut,
) -> Result<T, PlatformReadError>
where
    T: Send + 'static,
    Fut: Future<Output = Result<T, YoutubePlatformError>>,
{
    credential_read_opened_by_with_depth(platform, credential_id, "unspecified", open, search, read)
        .await
}

async fn credential_read_opened_by_with_depth<T, Fut>(
    platform: &LocalPlatform,
    credential_id: &str,
    read_depth: &'static str,
    open: OpenCredential,
    search: Option<BoundSearch<'_>>,
    read: impl FnOnce(YoutubePlatform) -> Fut,
) -> Result<T, PlatformReadError>
where
    T: Send + 'static,
    Fut: Future<Output = Result<T, YoutubePlatformError>>,
{
    if let Some(search) = &search {
        search.validate()?;
    }
    let admission_started = Instant::now();
    let opened = match admitted_credential(platform, credential_id, open).await {
        Ok(opened) => opened,
        Err(error) => {
            log::debug!(
                target: "onecrew.youtube",
                "youtube provider event=read phase=credential_admission read_depth={} elapsed_ms={} outcome=failure",
                read_depth,
                admission_started.elapsed().as_millis(),
            );
            return Err(error);
        }
    };
    log::debug!(
        target: "onecrew.youtube",
        "youtube provider event=read phase=credential_admission read_depth={} elapsed_ms={} outcome=success",
        read_depth,
        admission_started.elapsed().as_millis(),
    );
    let flight = platform.credential_services.begin_youtube_read();
    if let Some(search) = &search {
        search.validate()?;
    }
    let read_started = Instant::now();
    let outcome = read(opened.platform).await;
    log::debug!(
        target: "onecrew.youtube",
        "youtube provider event=read phase=provider_read read_depth={} elapsed_ms={} outcome={}",
        read_depth,
        read_started.elapsed().as_millis(),
        if outcome.is_ok() { "success" } else { "failure" },
    );
    let services = Arc::clone(&platform.credential_services);
    let health_started = Instant::now();
    blocking_segment(
        PlatformReadError::CredentialHealthCommitUnknown,
        move || {
            let _flight = flight;
            commit_health(
                &services,
                opened.id,
                &opened.revision,
                opened.observation,
                outcome,
            )
        },
    )
    .await
    .inspect(|_| {
        log::debug!(
            target: "onecrew.youtube",
            "youtube provider event=read phase=health_commit read_depth={} elapsed_ms={} outcome=success",
            read_depth,
            health_started.elapsed().as_millis(),
        );
    })
    .inspect_err(|_| {
        log::debug!(
            target: "onecrew.youtube",
            "youtube provider event=read phase=health_commit read_depth={} elapsed_ms={} outcome=failure",
            read_depth,
            health_started.elapsed().as_millis(),
        );
    })
}

/// Connection admission, then the credential opened on the blocking pool.
async fn admitted_credential(
    platform: &LocalPlatform,
    credential_id: &str,
    open: OpenCredential,
) -> Result<YoutubeCredentialRead, PlatformReadError> {
    platform
        .resolve_platform_connection(PlatformConnectionSelector::YoutubeCredential {
            credential_id: credential_id.to_owned(),
        })
        .await
        .map_err(PlatformReadError::Connection)?;
    let services = Arc::clone(&platform.credential_services);
    let credential_id = credential_id.to_owned();
    blocking_segment(PlatformReadError::Interrupted, move || {
        open(&services, &credential_id).map_err(credential_read_error)
    })
    .await
}

/// Records the read's health outcome on the credential and returns YouTube's
/// answer. A YouTube failure is returned only once its health is committed,
/// as YouTube's evidence after that local commit; a mark that cannot be
/// committed leaves the outcome unknown.
fn commit_health<T>(
    services: &CredentialServices,
    id: Uuid,
    revision: &str,
    observation: u64,
    outcome: Result<T, YoutubePlatformError>,
) -> Result<T, PlatformReadError> {
    match outcome {
        Ok(result) => {
            let commit = services
                .mark_youtube_read_success_if_revision(id, revision, observation)
                .map_err(|_| PlatformReadError::CredentialHealthCommitUnknown)?;
            log::debug!(
                target: "onecrew.youtube",
                "youtube provider event=health_commit disposition={} provider_outcome=success",
                health_commit_disposition(&commit),
            );
            // A stale success is still a valid provider read, but it must not
            // be presented as a health transition after newer credential state
            // has won the revision race.
            match commit {
                onecrew_local_vault::ServiceCredentialHealthCommit::Applied { .. }
                | onecrew_local_vault::ServiceCredentialHealthCommit::Superseded => Ok(result),
            }
        }
        Err(error) => {
            let commit = services
                .mark_youtube_read_failure_if_revision(id, revision, observation, &error)
                .map_err(|_| PlatformReadError::CredentialHealthCommitUnknown)?;
            log::debug!(
                target: "onecrew.youtube",
                "youtube provider event=health_commit disposition={} provider_outcome=failure",
                health_commit_disposition(&commit),
            );
            Err(PlatformReadError::Platform {
                evidence: error.evidence(),
                after_local_commit: matches!(
                    commit,
                    onecrew_local_vault::ServiceCredentialHealthCommit::Applied { .. }
                ),
            })
        }
    }
}

fn health_commit_disposition(
    commit: &onecrew_local_vault::ServiceCredentialHealthCommit,
) -> &'static str {
    match commit {
        onecrew_local_vault::ServiceCredentialHealthCommit::Applied { .. } => "applied",
        onecrew_local_vault::ServiceCredentialHealthCommit::Superseded => "superseded",
    }
}

fn validate_search_input(
    query: &str,
    count: u8,
    published_after: Option<&str>,
) -> Result<(), PlatformReadError> {
    if !accepts_search_query(query) {
        return Err(PlatformReadError::InputInvalid(
            "YouTube search query is required",
        ));
    }
    if !accepts_search_count(count) {
        return Err(PlatformReadError::InputInvalid(
            "YouTube search count must be between 1 and 50",
        ));
    }
    if published_after.is_some_and(|value| !accepts_published_after(value)) {
        return Err(PlatformReadError::InputInvalid(
            "YouTube published_after must be an RFC 3339 UTC instant, YYYY-MM-DDTHH:MM:SSZ",
        ));
    }
    Ok(())
}

/// A credential the vault row, API shape, recorded health or API client
/// refuses cannot be opened for the read.
fn credential_read_error(error: YoutubeCredentialReadError) -> PlatformReadError {
    match error {
        YoutubeCredentialReadError::InvalidCredentialId => {
            PlatformReadError::Connection(PlatformConnectionResolutionError::InvalidIdentifier)
        }
        YoutubeCredentialReadError::CredentialUnavailable
        | YoutubeCredentialReadError::Infrastructure => PlatformReadError::CredentialUnavailable,
    }
}

#[cfg(test)]
mod tests;
