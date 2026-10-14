//! Snapshot reads of stable attribution. New-effect admission is checked again
//! under the claim transaction; archive/disable do not erase recorded context.

use super::super::SqliteRepository;
use crate::*;
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};

pub(in super::super) fn route_context(
    repository: &SqliteRepository,
    creator_id: &CreatorId,
    route_id: &CreatorContactRouteId,
    target_id: &ResolvedTargetId,
    operator_id: &AccountRefId,
) -> Result<CreatorRouteOutreachContext, CreatorOutreachError> {
    let mut connection = repository
        .read_connection()
        .map_err(|_| CreatorOutreachError::Storage)?;
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Deferred)
        .map_err(|_| CreatorOutreachError::Storage)?;
    let context = route_in(&transaction, creator_id, route_id, target_id, operator_id)?;
    transaction
        .commit()
        .map_err(|_| CreatorOutreachError::Storage)?;
    Ok(context)
}

fn route_in(
    connection: &Connection,
    creator_id: &CreatorId,
    route_id: &CreatorContactRouteId,
    target_id: &ResolvedTargetId,
    operator_id: &AccountRefId,
) -> Result<CreatorRouteOutreachContext, CreatorOutreachError> {
    let creator = super::super::creators::load_creator_record(connection, creator_id)
        .map_err(|_| CreatorOutreachError::ContextUnavailable)?;
    let route = creator
        .contact_routes
        .iter()
        .find(|route| route.route.id == *route_id)
        .ok_or(CreatorOutreachError::ScopeMismatch)?;
    let operator = super::super::targets::load_account_ref(connection, operator_id)
        .map_err(|_| CreatorOutreachError::ContextUnavailable)?;
    let target = super::super::targets::load_resolved_target(connection, target_id)
        .map_err(|_| CreatorOutreachError::Storage)?
        .ok_or(CreatorOutreachError::ContextUnavailable)?;
    let account_id = target
        .platform_account_id
        .clone()
        .ok_or(CreatorOutreachError::ScopeMismatch)?;
    if target.account_ref_id != operator.id
        || target.platform != operator.platform
        || !creator
            .account_links
            .iter()
            .any(|link| link.platform_account_id == account_id)
    {
        return Err(CreatorOutreachError::ScopeMismatch);
    }
    let attributed = connection.query_row("SELECT EXISTS(SELECT 1 FROM locator_resolutions WHERE locator_id = ?1 AND account_ref_id = ?2 AND resolved_target_id = ?3)", params![route.locator.id.as_str(), operator_id.as_str(), target_id.as_str()], |row| row.get::<_, bool>(0)).map_err(|_| CreatorOutreachError::Storage)?;
    if !attributed {
        return Err(CreatorOutreachError::ScopeMismatch);
    }
    Ok(CreatorRouteOutreachContext {
        creator_id: creator_id.clone(),
        creator_display_name: creator.creator.display_name,
        contact_route_id: route_id.clone(),
        operator_account: operator,
        resolved_target_id: target_id.clone(),
        target: CreatorOutreachTarget {
            platform: target.platform,
            target_kind: target.target_kind,
            external_target_id: target.external_target_id,
            platform_account_id: account_id,
            handle: target
                .metadata
                .get("handle")
                .and_then(serde_json::Value::as_str)
                .map(str::trim)
                .filter(|handle| !handle.is_empty())
                .map(str::to_owned),
        },
        conversation: None,
    })
}

pub(in super::super) fn conversation_context(
    repository: &SqliteRepository,
    conversation_id: &ConversationId,
) -> Result<CreatorRouteOutreachContext, CreatorOutreachError> {
    let mut connection = repository
        .read_connection()
        .map_err(|_| CreatorOutreachError::Storage)?;
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Deferred)
        .map_err(|_| CreatorOutreachError::Storage)?;
    let scope = transaction.query_row(
        "SELECT route.creator_id, route.id, conversation.resolved_target_id, conversation.account_ref_id, conversation.provider_thread_id, conversation.channel
         FROM conversations conversation JOIN creator_contact_routes route ON route.id = (
           SELECT link.contact_route_id FROM conversation_contact_routes link WHERE link.conversation_id = conversation.id ORDER BY link.created_at ASC, link.contact_route_id ASC LIMIT 1)
         WHERE conversation.id = ?1", [conversation_id.as_str()], |row| Ok((row.get::<_, String>(0)?,row.get::<_, String>(1)?,row.get::<_, String>(2)?,row.get::<_, String>(3)?,row.get::<_, String>(4)?,row.get::<_, String>(5)?)),
    ).optional().map_err(|_| CreatorOutreachError::Storage)?.ok_or(CreatorOutreachError::ContextUnavailable)?;
    let mut context = route_in(
        &transaction,
        &CreatorId::new(scope.0),
        &CreatorContactRouteId::new(scope.1),
        &ResolvedTargetId::new(scope.2),
        &AccountRefId::new(scope.3),
    )?;
    if scope.4.trim().is_empty() || scope.5 != context.target.platform.as_str() {
        return Err(CreatorOutreachError::ScopeMismatch);
    }
    context.conversation = Some(CreatorOutreachConversation {
        conversation_id: conversation_id.clone(),
        provider_thread_id: scope.4,
    });
    transaction
        .commit()
        .map_err(|_| CreatorOutreachError::Storage)?;
    Ok(context)
}

pub(in super::super) fn content_context(
    repository: &SqliteRepository,
    input: PrepareContentReplyInput,
) -> Result<CreatorContentOutreachContext, CreatorOutreachError> {
    let mut connection = repository
        .read_connection()
        .map_err(|_| CreatorOutreachError::Storage)?;
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Deferred)
        .map_err(|_| CreatorOutreachError::Storage)?;
    let creator = super::super::creators::load_creator_record(&transaction, &input.creator_id)
        .map_err(|_| CreatorOutreachError::ContextUnavailable)?;
    let operator =
        super::super::targets::load_account_ref(&transaction, &input.operator_account_ref_id)
            .map_err(|_| CreatorOutreachError::ContextUnavailable)?;
    let account = creator
        .platform_accounts
        .iter()
        .find(|account| account.id == input.target_platform_account_id)
        .ok_or(CreatorOutreachError::ScopeMismatch)?;
    if account.platform != operator.platform
        || account.platform != Platform::X
        || account.target_kind != TargetKind::User
        || !creator.account_links.iter().any(|link| {
            link.platform_account_id == account.id && link.state == LinkState::Confirmed
        })
    {
        return Err(CreatorOutreachError::ScopeMismatch);
    }
    let handle = creator
        .account_observations
        .iter()
        .find(|observation| observation.platform_account_id == account.id)
        .and_then(|observation| observation.handle.clone());
    let content_id = input.content_id.trim().to_owned();
    if content_id.is_empty() {
        return Err(CreatorOutreachError::InvalidInput);
    }
    let context = CreatorContentOutreachContext {
        creator_id: input.creator_id,
        creator_display_name: creator.creator.display_name,
        operator_account: operator,
        target: CreatorOutreachTarget {
            platform: account.platform,
            target_kind: account.target_kind,
            external_target_id: account.external_id.clone(),
            platform_account_id: account.id.clone(),
            handle,
        },
        content_id,
        reply_to_id: input
            .reply_to_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned),
    };
    transaction
        .commit()
        .map_err(|_| CreatorOutreachError::Storage)?;
    Ok(context)
}
