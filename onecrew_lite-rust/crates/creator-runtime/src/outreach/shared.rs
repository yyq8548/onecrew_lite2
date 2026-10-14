use onecrew_dm_contracts::{DmTarget, DmTargetKind};

use crate::{
    AccountRef, ContactChannel, CreatorContactRouteId, CreatorContactRouteRecord, CreatorId,
    CreatorRuntime, LinkState, Platform, ResolvedTarget, TargetKind,
};

pub(super) fn contact_route_outreach_context(
    runtime: &CreatorRuntime,
    creator_id: &CreatorId,
    contact_route_id: &CreatorContactRouteId,
    resolved_target_id: &crate::ResolvedTargetId,
    operator_account_ref_id: &crate::AccountRefId,
    require_operable: bool,
) -> Result<
    (
        crate::CreatorRecord,
        CreatorContactRouteRecord,
        AccountRef,
        ResolvedTarget,
    ),
    String,
> {
    let creator = if require_operable {
        runtime.require_active_creator(creator_id)?
    } else {
        runtime.repository.get_creator(creator_id)?
    };
    let (creator, operator, target) = creator_target_context(
        runtime,
        creator,
        resolved_target_id,
        operator_account_ref_id,
        require_operable,
    )?;
    let route = creator
        .contact_routes
        .iter()
        .find(|route| route.route.id == *contact_route_id)
        .cloned()
        .ok_or_else(|| "contact route does not belong to the Creator".to_string())?;
    let resolution = runtime
        .repository
        .active_locator_resolution(&route.locator.id, &operator.id)?
        .ok_or_else(|| {
            "contact route has no active resolution for the selected operator".to_string()
        })?;
    if resolution.target.id != target.id {
        return Err("contact route does not resolve to the selected target".into());
    }
    Ok((creator, route, operator, target))
}

fn creator_target_context(
    runtime: &CreatorRuntime,
    creator: crate::CreatorRecord,
    resolved_target_id: &crate::ResolvedTargetId,
    operator_account_ref_id: &crate::AccountRefId,
    require_operable: bool,
) -> Result<(crate::CreatorRecord, AccountRef, ResolvedTarget), String> {
    let operator = if require_operable {
        runtime
            .repository
            .require_operable_account_ref(operator_account_ref_id)?
    } else {
        runtime
            .repository
            .get_account_ref(operator_account_ref_id)?
    };
    let target = runtime.repository.get_resolved_target(resolved_target_id)?;
    if target.account_ref_id != operator.id || target.platform != operator.platform {
        return Err("resolved target is outside the selected operator account context".into());
    }
    let platform_account_id = target.platform_account_id.as_ref().ok_or_else(|| {
        "resolved target is not attached to a stable platform account".to_string()
    })?;
    let owned = creator.account_links.iter().any(|link| {
        link.platform_account_id == *platform_account_id && link.state == LinkState::Confirmed
    });
    if !owned {
        return Err("resolved target is not confirmed for this Creator".into());
    }
    Ok((creator, operator, target))
}

pub(super) fn target_handle(target: &ResolvedTarget) -> Result<String, String> {
    target
        .metadata
        .get("handle")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| "resolved target has no verified handle for this action".into())
}

pub(super) fn dm_target(target: &ResolvedTarget) -> Result<DmTarget, String> {
    Ok(DmTarget {
        handle: target_handle(target)?,
        external_id: target.external_target_id.clone(),
        kind: dm_target_kind(target.target_kind),
    })
}

const fn dm_target_kind(kind: TargetKind) -> DmTargetKind {
    match kind {
        TargetKind::User => DmTargetKind::User,
        TargetKind::Bot => DmTargetKind::Bot,
        TargetKind::Channel => DmTargetKind::Channel,
        TargetKind::Supergroup => DmTargetKind::Supergroup,
        TargetKind::Group => DmTargetKind::Group,
        TargetKind::Creator => DmTargetKind::Creator,
        TargetKind::Business => DmTargetKind::Business,
    }
}

pub(super) fn message_channel(platform: Platform) -> Result<ContactChannel, String> {
    match platform {
        Platform::X => Ok(ContactChannel::X),
        Platform::Instagram => Ok(ContactChannel::Instagram),
        Platform::Telegram => Ok(ContactChannel::Telegram),
        Platform::YouTube => Err("YouTube does not support direct messages".into()),
        Platform::TikTok => Err("TikTok does not support direct messages".into()),
    }
}

pub(super) fn validate_idempotency_key(value: &str) -> Result<(), String> {
    if value.trim().is_empty() {
        Err("idempotency_key is required".into())
    } else {
        Ok(())
    }
}
