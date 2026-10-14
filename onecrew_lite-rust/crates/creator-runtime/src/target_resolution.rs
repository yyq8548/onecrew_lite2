//! Creator target policy and durable identity association. Provider reads are
//! composed by platform-runtime; this owner never writes profile observations.

use crate::{
    AccountRef, CommitContactTargetResolutionInput, CommitContactTargetResolutionResult,
    ContactAction, ContactChannel, ContactTargetResolutionResult, CreatorContactRouteRecord,
    CreatorId, IdentityConflict, LinkState, LocatorKind, Platform, PlatformResolvedTarget,
    PlatformTargetResolution, ResolveContactTargetInput, ResolvedLocatorTarget, TargetKind,
    TargetResolutionStatus,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CreatorTargetResolutionError {
    ContextUnavailable,
    InvalidEvidence,
    Storage,
    CommitUnknown,
}
impl std::fmt::Display for CreatorTargetResolutionError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::ContextUnavailable => "contact target context is unavailable",
            Self::InvalidEvidence => "contact target evidence does not match the request",
            Self::Storage => "contact target storage is unavailable",
            Self::CommitUnknown => "contact target commit outcome is unknown",
        })
    }
}
impl std::error::Error for CreatorTargetResolutionError {}

/// The exact route, locator and operator loaded before a provider read. Only
/// this owner can construct it; finish consumes it and rechecks the stored IDs.
#[derive(Debug)]
pub struct CreatorContactTargetReadContext {
    creator_id: CreatorId,
    route: CreatorContactRouteRecord,
    operator_account: AccountRef,
}
impl CreatorContactTargetReadContext {
    pub fn route(&self) -> &CreatorContactRouteRecord {
        &self.route
    }
    pub fn operator_account(&self) -> &AccountRef {
        &self.operator_account
    }
}

#[derive(Debug)]
pub enum CreatorContactTargetPreparation {
    Ready(Box<ContactTargetResolutionResult>),
    Read(CreatorContactTargetReadContext),
    /// Cached stable identity still needs an ownership transaction. No new
    /// provider read is needed and prepare itself has performed no write.
    Commit {
        context: CreatorContactTargetReadContext,
        evidence: PlatformTargetResolution,
    },
}

impl crate::CreatorRuntime {
    pub async fn prepare_contact_target_resolution(
        &self,
        input: ResolveContactTargetInput,
    ) -> Result<CreatorContactTargetPreparation, CreatorTargetResolutionError> {
        let runtime = self.clone();
        let (creator, route, operator_account, active) = self
            .repository_read_segment(move || {
                let creator = runtime
                    .require_active_creator(&input.creator_id)
                    .map_err(|_| CreatorTargetResolutionError::ContextUnavailable)?;
                let route = creator
                    .contact_routes
                    .iter()
                    .find(|record| record.route.id == input.contact_route_id)
                    .cloned()
                    .ok_or(CreatorTargetResolutionError::ContextUnavailable)?;
                let operator = runtime
                    .repository
                    .require_operable_account_ref(&input.operator_account_ref_id)
                    .map_err(|_| CreatorTargetResolutionError::ContextUnavailable)?;
                let active = if input.refresh {
                    None
                } else {
                    runtime
                        .repository
                        .active_locator_resolution(&route.locator.id, &operator.id)
                        .map_err(|_| CreatorTargetResolutionError::Storage)?
                };
                Ok((creator, route, operator, active))
            })
            .await
            .map_err(read_error)?;
        if let Some(result) = non_resolvable_route(&creator.creator.id, &route, &operator_account) {
            return Ok(CreatorContactTargetPreparation::Ready(Box::new(result)));
        }
        let platform = route_platform(route.locator.channel)
            .ok_or(CreatorTargetResolutionError::InvalidEvidence)?;
        if operator_account.platform != platform {
            return Err(CreatorTargetResolutionError::ContextUnavailable);
        }
        let context = CreatorContactTargetReadContext {
            creator_id: creator.creator.id,
            route,
            operator_account,
        };
        if let Some(active) = active {
            let target = PlatformResolvedTarget {
                platform: active.target.platform,
                target_kind: active.target.target_kind,
                external_target_id: active.target.external_target_id.clone(),
                handle: metadata_string(&active.target.metadata, "handle"),
                display_name: metadata_string(&active.target.metadata, "display_name"),
                canonical_url: metadata_string(&active.target.metadata, "canonical_url"),
            };
            validate_evidence(&context, &target)?;
            let already_owned =
                active
                    .target
                    .platform_account_id
                    .as_ref()
                    .is_some_and(|account_id| {
                        creator.account_links.iter().any(|link| {
                            &link.platform_account_id == account_id
                                && link.state == LinkState::Confirmed
                        })
                    });
            if !attach_as_creator_identity(target.platform, target.target_kind) || already_owned {
                return Ok(CreatorContactTargetPreparation::Ready(Box::new(
                    resolved_result(
                        context.creator_id,
                        context.route,
                        context.operator_account,
                        active,
                    ),
                )));
            }
            return Ok(CreatorContactTargetPreparation::Commit {
                context,
                evidence: PlatformTargetResolution::Resolved { target },
            });
        }
        Ok(CreatorContactTargetPreparation::Read(context))
    }

    pub async fn finish_contact_target_resolution(
        &self,
        context: CreatorContactTargetReadContext,
        evidence: PlatformTargetResolution,
    ) -> Result<ContactTargetResolutionResult, CreatorTargetResolutionError> {
        match evidence {
            PlatformTargetResolution::Resolved { target } => {
                validate_evidence(&context, &target)?;
                let runtime = self.clone();
                self.repository_write_segment(move || {
                    let result = runtime.repository.commit_contact_target_resolution(CommitContactTargetResolutionInput {
                        creator_id: context.creator_id.clone(), contact_route_id: context.route.route.id.clone(),
                        locator_id: context.route.locator.id.clone(), account_ref_id: context.operator_account.id.clone(),
                        platform: target.platform, target_kind: target.target_kind, external_target_id: target.external_target_id,
                        attach_to_creator: attach_as_creator_identity(target.platform, target.target_kind),
                        metadata: serde_json::json!({"handle":target.handle,"display_name":target.display_name,"canonical_url":target.canonical_url}),
                    })?;
                    Ok(project_commit(context, result))
                }).await.map_err(write_error)
            }
            PlatformTargetResolution::RequiresOperatorAuth { reason } => Ok(status_result(
                context.creator_id,
                context.route,
                context.operator_account,
                TargetResolutionStatus::RequiresOperatorAuth,
                None,
                Some(reason),
            )),
            PlatformTargetResolution::BrowserOnly {
                canonical_url,
                reason,
            } => Ok(status_result(
                context.creator_id,
                context.route,
                context.operator_account,
                TargetResolutionStatus::BrowserOnly,
                canonical_url,
                Some(reason),
            )),
            PlatformTargetResolution::Unsupported { reason } => Ok(status_result(
                context.creator_id,
                context.route,
                context.operator_account,
                TargetResolutionStatus::Unsupported,
                None,
                Some(reason),
            )),
            PlatformTargetResolution::NotFound { reason } => Ok(status_result(
                context.creator_id,
                context.route,
                context.operator_account,
                TargetResolutionStatus::NotFound,
                None,
                Some(reason),
            )),
        }
    }
}

fn validate_evidence(
    context: &CreatorContactTargetReadContext,
    target: &PlatformResolvedTarget,
) -> Result<(), CreatorTargetResolutionError> {
    if target.platform != context.operator_account.platform
        || Some(target.platform) != route_platform(context.route.locator.channel)
        || target.external_target_id.trim().is_empty()
        || target.external_target_id.trim() != target.external_target_id
        || !valid_target_kind(target.platform, target.target_kind)
    {
        return Err(CreatorTargetResolutionError::InvalidEvidence);
    }
    Ok(())
}
fn project_commit(
    context: CreatorContactTargetReadContext,
    result: CommitContactTargetResolutionResult,
) -> ContactTargetResolutionResult {
    match result {
        CommitContactTargetResolutionResult::Active { value } => resolved_result(
            context.creator_id,
            context.route,
            context.operator_account,
            value,
        ),
        CommitContactTargetResolutionResult::LocatorConflict {
            conflict, active, ..
        } => conflict_result(
            context.creator_id,
            context.route,
            context.operator_account,
            Some(conflict),
            format!(
                "locator now resolves to a different stable target; active target remains {}",
                active.target.external_target_id
            ),
        ),
        CommitContactTargetResolutionResult::CreatorConflict {
            conflict,
            creator_ids,
            ..
        } => conflict_result(
            context.creator_id,
            context.route,
            context.operator_account,
            Some(conflict),
            format!(
                "stable platform target is already confirmed for Creator(s): {}",
                creator_ids
                    .iter()
                    .map(|id| id.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        ),
    }
}
fn metadata_string(metadata: &serde_json::Value, key: &str) -> Option<String> {
    metadata
        .get(key)
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .map(str::to_owned)
}
fn read_error(
    error: crate::CreatorRepositoryCallError<CreatorTargetResolutionError>,
) -> CreatorTargetResolutionError {
    match error {
        crate::CreatorRepositoryCallError::Owner(error) => error,
        crate::CreatorRepositoryCallError::Dispatch(_) => CreatorTargetResolutionError::Storage,
    }
}
fn write_error(
    error: crate::CreatorRepositoryCallError<CreatorTargetResolutionError>,
) -> CreatorTargetResolutionError {
    match error {
        crate::CreatorRepositoryCallError::Owner(error) => error,
        crate::CreatorRepositoryCallError::Dispatch(error) if error.known_no_effect() => {
            CreatorTargetResolutionError::Storage
        }
        crate::CreatorRepositoryCallError::Dispatch(_) => {
            CreatorTargetResolutionError::CommitUnknown
        }
    }
}

fn route_platform(channel: ContactChannel) -> Option<Platform> {
    match channel {
        ContactChannel::X => Some(Platform::X),
        ContactChannel::Instagram => Some(Platform::Instagram),
        ContactChannel::Telegram => Some(Platform::Telegram),
        ContactChannel::YouTube => Some(Platform::YouTube),
        ContactChannel::TikTok => None,
        ContactChannel::Email | ContactChannel::Website | ContactChannel::LinkHub | ContactChannel::Wechat => None,
    }
}

fn attach_as_creator_identity(platform: Platform, kind: TargetKind) -> bool {
    matches!(
        (platform, kind),
        (
            Platform::X | Platform::Instagram,
            TargetKind::User | TargetKind::Creator | TargetKind::Business
        ) | (Platform::Telegram, TargetKind::User)
            | (Platform::YouTube, TargetKind::Channel)
    )
}

fn valid_target_kind(platform: Platform, kind: TargetKind) -> bool {
    matches!(
        (platform, kind),
        (Platform::X, TargetKind::User)
            | (
                Platform::Instagram,
                TargetKind::User | TargetKind::Creator | TargetKind::Business
            )
            | (
                Platform::Telegram,
                TargetKind::User
                    | TargetKind::Bot
                    | TargetKind::Channel
                    | TargetKind::Supergroup
                    | TargetKind::Group
            )
            | (Platform::YouTube, TargetKind::Channel)
    )
}

fn actions(platform: Platform, kind: TargetKind) -> Vec<ContactAction> {
    match (platform, kind) {
        (
            Platform::X | Platform::Instagram,
            TargetKind::User | TargetKind::Creator | TargetKind::Business,
        ) => vec![
            ContactAction::OpenProfile,
            ContactAction::ReadPublic,
            ContactAction::Follow,
            ContactAction::DirectMessage,
        ],
        (Platform::Telegram, TargetKind::User) => {
            vec![ContactAction::OpenProfile, ContactAction::DirectMessage]
        }
        (Platform::Telegram, TargetKind::Bot) => vec![ContactAction::OpenProfile],
        (Platform::Telegram, TargetKind::Channel) => vec![
            ContactAction::OpenCommunity,
            ContactAction::ReadPublic,
            ContactAction::ExtractAdministrators,
        ],
        (Platform::Telegram, TargetKind::Supergroup) => vec![
            ContactAction::OpenCommunity,
            ContactAction::ExtractAdministrators,
        ],
        (Platform::Telegram, TargetKind::Group) => vec![ContactAction::OpenCommunity],
        (Platform::YouTube, TargetKind::Channel) => {
            vec![ContactAction::OpenProfile, ContactAction::ReadPublic]
        }
        _ => Vec::new(),
    }
}

fn non_resolvable_route(
    creator_id: &crate::CreatorId,
    route: &CreatorContactRouteRecord,
    operator_account: &AccountRef,
) -> Option<ContactTargetResolutionResult> {
    match route.locator.channel {
        ContactChannel::Website | ContactChannel::LinkHub => Some(status_result(
            creator_id.clone(),
            route.clone(),
            operator_account.clone(),
            TargetResolutionStatus::BrowserOnly,
            (route.locator.kind == LocatorKind::Url)
                .then(|| route.locator.normalized_value.clone()),
            Some("public web locators are opened in the browser and do not create targets".into()),
        )),
        ContactChannel::Wechat => Some(status_result(
            creator_id.clone(), route.clone(), operator_account.clone(),
            TargetResolutionStatus::Unsupported, None,
            Some("WeChat handles are retained for manual contact; automated messaging is unavailable".into()),
        )),
        ContactChannel::Email => Some(status_result(
            creator_id.clone(),
            route.clone(),
            operator_account.clone(),
            TargetResolutionStatus::Unsupported,
            None,
            Some("email target preparation belongs to the mailbox phase".into()),
        )),
        _ => None,
    }
}

fn resolved_result(
    creator_id: crate::CreatorId,
    route: CreatorContactRouteRecord,
    operator_account: AccountRef,
    resolved: ResolvedLocatorTarget,
) -> ContactTargetResolutionResult {
    let allowed_actions = actions(resolved.target.platform, resolved.target.target_kind);
    let browser_url = resolved
        .target
        .metadata
        .get("canonical_url")
        .and_then(serde_json::Value::as_str)
        .map(ToOwned::to_owned);
    ContactTargetResolutionResult {
        creator_id,
        route,
        operator_account,
        status: TargetResolutionStatus::Resolved,
        resolved: Some(resolved),
        conflict: None,
        allowed_actions,
        browser_url,
        reason: None,
    }
}

fn status_result(
    creator_id: crate::CreatorId,
    route: CreatorContactRouteRecord,
    operator_account: AccountRef,
    status: TargetResolutionStatus,
    browser_url: Option<String>,
    reason: Option<String>,
) -> ContactTargetResolutionResult {
    ContactTargetResolutionResult {
        creator_id,
        route,
        operator_account,
        status,
        resolved: None,
        conflict: None,
        allowed_actions: if status == TargetResolutionStatus::BrowserOnly {
            vec![ContactAction::OpenProfile]
        } else {
            Vec::new()
        },
        browser_url,
        reason,
    }
}

fn conflict_result(
    creator_id: crate::CreatorId,
    route: CreatorContactRouteRecord,
    operator_account: AccountRef,
    conflict: Option<IdentityConflict>,
    reason: String,
) -> ContactTargetResolutionResult {
    ContactTargetResolutionResult {
        creator_id,
        route,
        operator_account,
        status: TargetResolutionStatus::Conflict,
        resolved: None,
        conflict,
        allowed_actions: Vec::new(),
        browser_url: None,
        reason: Some(reason),
    }
}

#[cfg(test)]
mod tests;
