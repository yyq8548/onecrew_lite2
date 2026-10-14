//! Read-only saved-community provenance. Provider peer credentials stay with
//! the platform session; this context contains only stable public identity.

use crate::*;

#[derive(Debug, Clone)]
pub struct TelegramCommunityRouteContext {
    pub external_id: String,
    pub target_kind: CommunityTargetKind,
    pub handle: Option<String>,
    pub source: CommunityAdministratorSource,
}

impl CreatorRuntime {
    pub async fn telegram_community_route_context(
        &self,
        source_creator_id: CreatorId,
        source_contact_route_id: CreatorContactRouteId,
        operator_account_ref_id: AccountRefId,
    ) -> Result<TelegramCommunityRouteContext, CreatorRepositoryCallError<String>> {
        let runtime = self.clone();
        self.repository_read_segment(move || {
            let creator = runtime.require_active_creator(&source_creator_id)?;
            let operator = runtime
                .repository
                .require_operable_account_ref(&operator_account_ref_id)?;
            if operator.platform != Platform::Telegram {
                return Err("community administrators require a Telegram operator account".into());
            }
            let route = creator
                .contact_routes
                .into_iter()
                .find(|route| route.route.id == source_contact_route_id)
                .ok_or("contact route does not belong to the source Creator")?;
            if route.locator.channel != ContactChannel::Telegram {
                return Err("source route must be a Telegram contact route".into());
            }
            let active = runtime
                .repository
                .active_locator_resolution(&route.locator.id, &operator_account_ref_id)?
                .ok_or("resolve the Telegram contact target before preparing administrators")?;
            if active.target.platform != Platform::Telegram {
                return Err("resolved source is not Telegram".into());
            }
            let target_kind = CommunityTargetKind::try_from(active.target.target_kind)?;
            let external_id = active.target.external_target_id.trim().to_owned();
            if external_id.is_empty() {
                return Err("resolved Telegram community has no stable target ID".into());
            }
            let handle = active
                .target
                .metadata
                .get("handle")
                .and_then(serde_json::Value::as_str)
                .map(str::trim)
                .filter(|handle| !handle.is_empty())
                .map(str::to_owned);
            Ok(TelegramCommunityRouteContext {
                external_id,
                target_kind,
                handle,
                source: CommunityAdministratorSource::CreatorContactRoute {
                    source_creator_id,
                    source_contact_route_id: Some(source_contact_route_id),
                    operator_account_ref_id,
                    source_resolved_target_id: active.target.id,
                },
            })
        })
        .await
    }
}
