use rusqlite::{OptionalExtension, TransactionBehavior, params};
use uuid::Uuid;

use crate::{
    AccountRefId, BulkMoveCreatorsToGroupInput, CREATOR_GROUP_BULK_MOVE_LIMIT,
    CREATOR_GROUP_NAME_MAX_CHARS, CREATOR_GROUP_PAGE_LIMITS, CommunityAdministratorRole,
    CommunityAdministratorSource, CommunityReference, CommunityTargetKind, Confidence,
    ContactChannel, CreateCreatorGroupInput, CreateCreatorInput, Creator, CreatorAccountLink,
    CreatorAccountLinkId, CreatorAccountLinkId as LinkId, CreatorContactRouteId,
    CreatorDeleteResult, CreatorGroup, CreatorGroupAssignment, CreatorGroupDestination,
    CreatorGroupError, CreatorGroupId, CreatorGroupListResult, CreatorGroupRef,
    CreatorGroupSummary, CreatorId, CreatorOrigin, CreatorOriginId, CreatorRecord,
    CreatorRelationship, CreatorStage, CreatorSummary, DeleteCreatorGroupInput,
    DeleteCreatorGroupResult, DeleteCreatorInput, DiscoveredContactRoute, DiscoveryId,
    IdentityResolution, LinkState, ListCreatorGroupsInput, ListCreatorsInput, LocatorKind,
    ManualCreatorCreateInput, ManualCreatorCreateResult, MoveCreatorsToGroupResult,
    ObservationSource, ObservationSourceId, Platform, PlatformAccount, PlatformAccountId,
    PlatformAccountObservation, PlatformAccountObservationId, PlatformAccountObservationInput,
    ResolvedTargetId, SetCreatorGroupInput, SourceKind, TargetKind, UpdateCreatorGroupInput,
    UpsertPlatformAccountInput, repository::CreatorRepository,
};

use super::{SqliteRepository, json, now};
mod list;

fn insert_manual_creator_in(
    connection: &rusqlite::Connection,
    input: &ManualCreatorCreateInput,
    check_existing_owner: bool,
) -> Result<ManualCreatorCreateResult, String> {
    let display_name = input.display_name.trim();
    let tags = crate::creators::normalize_creator_tags(input.tags.clone().unwrap_or_default())
        .map_err(|error| error.to_string())?;
    if display_name.is_empty() {
        return Err("creator display_name is required".into());
    }
    let notes = input
        .notes
        .as_deref()
        .map(str::trim)
        .map(|value| {
            if value.is_empty() {
                Err("creator notes cannot be empty when provided".to_string())
            } else {
                Ok(value.to_owned())
            }
        })
        .transpose()?;
    let creator_id = CreatorId::new(Uuid::new_v4().to_string());
    if input.contact_routes.len() > crate::MAX_INITIAL_CONTACT_ROUTES_PER_CREATOR {
        return Err(format!(
            "Creator supports at most {} initial contact routes",
            crate::MAX_INITIAL_CONTACT_ROUTES_PER_CREATOR
        ));
    }
    let source_id = ObservationSourceId::new(Uuid::new_v4().to_string());
    let origin_id = CreatorOriginId::new(Uuid::new_v4().to_string());
    let timestamp = now();
    let transaction = connection;
    transaction
        .execute(
            "INSERT INTO creators
                   (id, display_name, notes, created_at, updated_at, archived_at, tags_json)
                 VALUES (?1, ?2, ?3, ?4, ?4, NULL, ?5)",
            params![
                creator_id.as_str(),
                display_name,
                notes,
                timestamp,
                json(&tags)?
            ],
        )
        .map_err(|error| error.to_string())?;
    transaction
        .execute(
            "INSERT INTO creator_relationships
                   (creator_id, stage, last_touch_at, created_at, updated_at)
                 VALUES (?1, 'prospect', NULL, ?2, ?2)",
            params![creator_id.as_str(), timestamp],
        )
        .map_err(|error| error.to_string())?;
    transaction
        .execute(
            "INSERT INTO observation_sources
                   (id, kind, platform, source_url, external_ref, account_ref_id,
                    metadata_json, observed_at)
                 VALUES (?1, 'manual_entry', NULL, NULL, NULL, NULL, '{}', ?2)",
            params![source_id.as_str(), timestamp],
        )
        .map_err(|error| error.to_string())?;
    transaction
        .execute(
            "INSERT INTO creator_origins
                   (id, kind, creator_id, source_id, discovery_id,
                    source_creator_id, source_contact_route_id, operator_account_ref_id,
                    source_resolved_target_id, administrator_role, community_external_id,
                    community_title, community_handle, community_target_kind, discovered_at)
                 VALUES (?1, 'manual_entry', ?2, ?3, NULL, NULL, NULL, NULL,
                         NULL, NULL, NULL, NULL, NULL, NULL, ?4)",
            params![
                origin_id.as_str(),
                creator_id.as_str(),
                source_id.as_str(),
                timestamp
            ],
        )
        .map_err(|error| error.to_string())?;
    let mut route_keys = std::collections::HashSet::new();
    for route in &input.contact_routes {
        let value = route.value.trim();
        if value.is_empty() {
            return Err("contact route value is required".into());
        }
        let source_url = route
            .source_url
            .as_deref()
            .map(str::trim)
            .map(|value| {
                if value.is_empty() {
                    Err("contact route source_url cannot be empty when provided".to_string())
                } else {
                    Ok(value.to_owned())
                }
            })
            .transpose()?;
        let canonical =
            crate::contact_routes::normalize_discovered_contact_route(&DiscoveredContactRoute {
                channel: route.channel,
                kind: LocatorKind::Handle,
                value: value.to_owned(),
                source: SourceKind::ManualEntry,
                source_url: source_url.clone(),
            })?;
        let key = crate::contact_routes::locator_key(&canonical);
        if !route_keys.insert(key) {
            return Err("duplicate contact route in Creator create request".into());
        }
        if check_existing_owner
            && super::contacts::find_contact_route_owners_in(transaction, &canonical)?
                .into_iter()
                .next()
                .is_some()
        {
            return Err("contact route already belongs to another Creator".into());
        }
        let source = super::contacts::upsert_observation_source_in(
            transaction,
            &crate::ObservationSourceInput {
                kind: SourceKind::ManualEntry,
                platform: match route.channel {
                    ContactChannel::X => Some(Platform::X),
                    ContactChannel::Instagram => Some(Platform::Instagram),
                    ContactChannel::Telegram => Some(Platform::Telegram),
                    ContactChannel::YouTube => Some(Platform::YouTube),
                    ContactChannel::TikTok => Some(Platform::TikTok),
                    _ => None,
                },
                source_url,
                external_ref: None,
                account_ref_id: None,
                metadata: serde_json::json!({}),
            },
        )?;
        let locator = super::contacts::upsert_contact_locator_in(transaction, &canonical)?;
        let created_route = super::contacts::link_creator_contact_route_in(
            transaction,
            &creator_id,
            &locator.id,
            Confidence::Asserted,
        )?;
        super::contacts::append_locator_observation_in(
            transaction,
            &locator.id,
            &source.id,
            Some(&creator_id),
            None,
            value,
        )?;
        transaction
            .execute(
                "INSERT INTO audit_events
                       (id, kind, creator_id, account_ref_id, resolved_target_id,
                        mailbox_ref_id, contact_locator_id, contact_route_id, email_id,
                        payload_json, created_at)
                     VALUES (?1, 'creator_contact_route_added', ?2, NULL, NULL,
                             NULL, ?3, ?4, NULL, ?5, ?6)",
                params![
                    Uuid::new_v4().to_string(),
                    creator_id.as_str(),
                    locator.id.as_str(),
                    created_route.id.as_str(),
                    json(&serde_json::json!({
                        "route_id": created_route.id.as_str(),
                        "source": "manual_entry"
                    }))?,
                    timestamp
                ],
            )
            .map_err(|error| error.to_string())?;
    }
    let contact_routes = super::contacts::load_creator_contact_routes(connection, &creator_id)?;
    Ok(ManualCreatorCreateResult {
        tags,
        creator_id,
        display_name: display_name.to_owned(),
        stage: CreatorStage::Prospect,
        created_at: timestamp,
        contact_routes,
    })
}

impl CreatorRepository for SqliteRepository {
    fn outreach_route_context(
        &self,
        creator_id: &crate::CreatorId,
        route_id: &crate::CreatorContactRouteId,
        target_id: &crate::ResolvedTargetId,
        operator_id: &crate::AccountRefId,
    ) -> Result<crate::CreatorRouteOutreachContext, crate::CreatorOutreachError> {
        super::outreach_actions::context::route_context(
            self,
            creator_id,
            route_id,
            target_id,
            operator_id,
        )
    }
    fn outreach_conversation_context(
        &self,
        conversation_id: &crate::ConversationId,
    ) -> Result<crate::CreatorRouteOutreachContext, crate::CreatorOutreachError> {
        super::outreach_actions::context::conversation_context(self, conversation_id)
    }
    fn outreach_content_context(
        &self,
        input: crate::PrepareContentReplyInput,
    ) -> Result<crate::CreatorContentOutreachContext, crate::CreatorOutreachError> {
        super::outreach_actions::context::content_context(self, input)
    }
    fn export_creator_bundle(&self) -> Result<crate::CreatorBundle, String> {
        super::creator_transfer::export_creator_bundle(self)
    }

    fn export_creator_workbook(&self) -> Result<crate::CreatorWorkbookExport, String> {
        super::creator_transfer::export_creator_workbook(self)
    }

    fn export_creator_workbook_snapshot(
        &self,
    ) -> Result<Vec<(crate::CreatorTransferCreator, crate::CreatorWorkbookDisplay)>, String> {
        super::creator_transfer::export_creator_workbook_snapshot(self)
    }

    fn prepare_creator_import(
        &self,
        bundle: crate::CreatorBundle,
    ) -> Result<crate::PrepareCreatorImportResult, String> {
        super::creator_import::prepare(self, bundle)
    }

    fn load_creator_import_basis(
        &self,
        artifact: &crate::repository::ValidatedImportArtifact,
    ) -> Result<crate::repository::ImportBasis, String> {
        super::creator_import::load_basis(self, artifact)
    }

    fn write_creator_import_prepare(
        &self,
        work: crate::repository::PrepareImportWrite,
    ) -> Result<crate::repository::PrepareImportWriteOutcome, String> {
        super::creator_import::write_prepare(self, work)
    }

    fn load_creator_import_operation(
        &self,
        input: &crate::ConfirmCreatorImportInput,
    ) -> Result<crate::repository::ImportOperationSnapshot, String> {
        super::creator_import::load_operation(self, input)
    }

    fn write_creator_import_confirm(
        &self,
        work: crate::repository::ConfirmImportWrite,
    ) -> Result<crate::repository::ConfirmImportWriteOutcome, String> {
        super::creator_import::write_confirm(self, work)
    }

    fn confirm_creator_import(
        &self,
        input: crate::ConfirmCreatorImportInput,
    ) -> Result<(crate::ConfirmCreatorImportResult, bool), String> {
        super::creator_import::confirm(self, input)
    }

    fn cancel_creator_import(
        &self,
        input: crate::ConfirmCreatorImportInput,
    ) -> Result<crate::ConfirmCreatorImportResult, String> {
        super::creator_import::cancel(self, input)
    }

    fn create_creator(&self, input: CreateCreatorInput) -> Result<CreatorRecord, String> {
        let tags = crate::creators::normalize_creator_tags(input.tags.clone().unwrap_or_default())
            .map_err(|error| error.to_string())?;
        let display_name = input.display_name.trim();
        if display_name.is_empty() {
            return Err("creator display_name is required".into());
        }
        let id = CreatorId::new(Uuid::new_v4().to_string());
        let timestamp = now();
        let mut connection = self.lock()?;
        let transaction = connection
            .transaction()
            .map_err(|error| error.to_string())?;
        transaction
            .execute(
                "INSERT INTO creators
                   (id, display_name, notes, created_at, updated_at, archived_at, tags_json)
                 VALUES (?1, ?2, ?3, ?4, ?4, NULL, ?5)",
                params![
                    id.as_str(),
                    display_name,
                    input.notes,
                    timestamp,
                    json(&tags)?
                ],
            )
            .map_err(|error| error.to_string())?;
        transaction
            .execute(
                "INSERT INTO creator_relationships
                   (creator_id, stage, last_touch_at, created_at, updated_at)
                 VALUES (?1, 'prospect', NULL, ?2, ?2)",
                params![id.as_str(), timestamp],
            )
            .map_err(|error| error.to_string())?;
        transaction.commit().map_err(|error| error.to_string())?;
        load_creator_record(&connection, &id)
    }

    fn create_manual_creator(
        &self,
        input: ManualCreatorCreateInput,
    ) -> Result<ManualCreatorCreateResult, String> {
        let mut connection = self.lock()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| error.to_string())?;
        let result = insert_manual_creator_in(&transaction, &input, true)?;
        transaction.commit().map_err(|error| error.to_string())?;
        Ok(result)
    }

    fn create_manual_creator_batch(
        &self,
        inputs: Vec<ManualCreatorCreateInput>,
    ) -> Result<Vec<ManualCreatorCreateResult>, String> {
        if inputs.is_empty() {
            return Ok(Vec::new());
        }
        let mut connection = self.lock()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| error.to_string())?;
        let mut results = Vec::with_capacity(inputs.len());
        for input in &inputs {
            // The service performs the set-based deterministic preflight. The
            // unique locator index and trigger below remain the final race
            // guard while this transaction is writing.
            results.push(insert_manual_creator_in(&transaction, input, false)?);
        }
        transaction.commit().map_err(|error| error.to_string())?;
        Ok(results)
    }

    fn get_creator(&self, creator_id: &CreatorId) -> Result<CreatorRecord, String> {
        let mut connection = self.read_connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Deferred)
            .map_err(|error| error.to_string())?;
        let creator = load_creator_record(&transaction, creator_id)?;
        transaction.commit().map_err(|error| error.to_string())?;
        Ok(creator)
    }

    fn list_creators(&self, input: ListCreatorsInput) -> Result<crate::CreatorListPage, String> {
        list::list_creators(self, input)
    }

    fn list_creator_groups(
        &self,
        input: ListCreatorGroupsInput,
    ) -> Result<CreatorGroupListResult, CreatorGroupError> {
        let limit = CREATOR_GROUP_PAGE_LIMITS.effective(input.limit) as usize;
        let query = input
            .query
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned);
        let context = format!(
            "creator_group_list|query={}|archived={}",
            query.as_deref().unwrap_or(""),
            input.include_archived
        );
        let position = input
            .cursor
            .as_deref()
            .map(|cursor| super::decode_keyset_position(cursor, &context, 1))
            .transpose()
            .map_err(|_| CreatorGroupError::InvalidInput)?;
        let (position_order, position_id) = match position {
            Some((numbers, id)) => (Some(numbers[0]), Some(id)),
            None => (None, None),
        };
        let mut connection = self
            .read_connection()
            .map_err(|_| CreatorGroupError::Internal)?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Deferred)
            .map_err(|_| CreatorGroupError::Internal)?;
        let mut statement = transaction
            .prepare(
                "SELECT creator_group.id, creator_group.name, creator_group.revision,
                        creator_group.position, creator_group.created_at,
                        creator_group.updated_at,
                        (SELECT COUNT(*) FROM creators creator
                          WHERE creator.group_id = creator_group.id
                            AND (?2 OR creator.archived_at IS NULL)) AS creator_count
                 FROM creator_groups creator_group
                 WHERE (?1 IS NULL OR instr(lower(creator_group.name), lower(?1)) > 0)
                   AND (?3 IS NULL OR creator_group.position > ?3
                        OR (creator_group.position = ?3 AND creator_group.id > ?4))
                 ORDER BY creator_group.position ASC, creator_group.id ASC
                 LIMIT ?5",
            )
            .map_err(|_| CreatorGroupError::Internal)?;
        let rows = statement
            .query_map(
                params![
                    query,
                    input.include_archived,
                    position_order,
                    position_id,
                    (limit + 1) as i64
                ],
                |row| {
                    Ok(CreatorGroupSummary {
                        group: CreatorGroup {
                            id: CreatorGroupId::new(row.get::<_, String>(0)?),
                            name: row.get(1)?,
                            revision: row.get(2)?,
                            position: row.get(3)?,
                            created_at: row.get(4)?,
                            updated_at: row.get(5)?,
                        },
                        creator_count: row.get(6)?,
                    })
                },
            )
            .map_err(|_| CreatorGroupError::Internal)?
            .map(|row| row.map_err(|_| CreatorGroupError::Internal))
            .collect::<Result<Vec<_>, _>>()?;
        drop(statement);
        let page = super::keyset_page(rows, limit, &context, |summary| {
            (vec![summary.group.position], summary.group.id.as_str())
        });
        let uncategorized_count = transaction
            .query_row(
                "SELECT COUNT(*) FROM creators
                 WHERE group_id IS NULL AND (?1 OR archived_at IS NULL)",
                [input.include_archived],
                |row| row.get::<_, u64>(0),
            )
            .map_err(|_| CreatorGroupError::Internal)?;
        transaction
            .commit()
            .map_err(|_| CreatorGroupError::Internal)?;
        Ok(CreatorGroupListResult {
            items: page.items,
            uncategorized_count,
            next_cursor: page.next_cursor,
        })
    }

    fn create_creator_group(
        &self,
        input: CreateCreatorGroupInput,
    ) -> Result<CreatorGroup, CreatorGroupError> {
        let name = normalize_creator_group_name(&input.name)?;
        let id = CreatorGroupId::new(Uuid::new_v4().to_string());
        let timestamp = now();
        let mut connection = self.lock().map_err(|_| CreatorGroupError::Internal)?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|_| CreatorGroupError::Internal)?;
        ensure_group_name_available(&transaction, &name, None)?;
        let position = transaction
            .query_row(
                "SELECT COALESCE(MAX(position), -1) + 1 FROM creator_groups",
                [],
                |row| row.get::<_, u64>(0),
            )
            .map_err(|_| CreatorGroupError::Internal)?;
        transaction
            .execute(
                "INSERT INTO creator_groups
                   (id, name, revision, position, created_at, updated_at)
                 VALUES (?1, ?2, 1, ?3, ?4, ?4)",
                params![id.as_str(), name, position, timestamp],
            )
            .map_err(|_| CreatorGroupError::Internal)?;
        let group = load_creator_group(&transaction, &id)?;
        transaction
            .commit()
            .map_err(|_| CreatorGroupError::Internal)?;
        Ok(group)
    }

    fn update_creator_group(
        &self,
        input: UpdateCreatorGroupInput,
    ) -> Result<(CreatorGroup, bool), CreatorGroupError> {
        let name = normalize_creator_group_name(&input.name)?;
        let timestamp = now();
        let mut connection = self.lock().map_err(|_| CreatorGroupError::Internal)?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|_| CreatorGroupError::Internal)?;
        ensure_group_revision(&transaction, &input.group_id, input.expected_revision)?;
        let current = load_creator_group(&transaction, &input.group_id)?;
        if current.name == name {
            transaction
                .commit()
                .map_err(|_| CreatorGroupError::Internal)?;
            return Ok((current, false));
        }
        ensure_group_name_available(&transaction, &name, Some(&input.group_id))?;
        transaction
            .execute(
                "UPDATE creator_groups
                 SET name = ?2, revision = revision + 1, updated_at = ?3
                 WHERE id = ?1 AND revision = ?4",
                params![
                    input.group_id.as_str(),
                    name,
                    timestamp,
                    input.expected_revision
                ],
            )
            .map_err(|_| CreatorGroupError::Internal)?;
        let group = load_creator_group(&transaction, &input.group_id)?;
        transaction
            .commit()
            .map_err(|_| CreatorGroupError::Internal)?;
        Ok((group, true))
    }

    fn delete_creator_group(
        &self,
        input: DeleteCreatorGroupInput,
    ) -> Result<DeleteCreatorGroupResult, CreatorGroupError> {
        let mut connection = self.lock().map_err(|_| CreatorGroupError::Internal)?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|_| CreatorGroupError::Internal)?;
        ensure_group_revision(&transaction, &input.group_id, input.expected_revision)?;
        let uncategorized_creator_count = transaction
            .query_row(
                "SELECT COUNT(*) FROM creators WHERE group_id = ?1",
                [input.group_id.as_str()],
                |row| row.get::<_, u64>(0),
            )
            .map_err(|_| CreatorGroupError::Internal)?;
        transaction
            .execute(
                "DELETE FROM creator_groups WHERE id = ?1 AND revision = ?2",
                params![input.group_id.as_str(), input.expected_revision],
            )
            .map_err(|_| CreatorGroupError::Internal)?;
        transaction
            .commit()
            .map_err(|_| CreatorGroupError::Internal)?;
        Ok(DeleteCreatorGroupResult {
            group_id: input.group_id,
            uncategorized_creator_count,
        })
    }

    fn set_creator_group(
        &self,
        input: SetCreatorGroupInput,
    ) -> Result<MoveCreatorsToGroupResult, CreatorGroupError> {
        move_creators_to_group(self, vec![input.creator_id], input.destination)
    }

    fn bulk_move_creators_to_group(
        &self,
        input: BulkMoveCreatorsToGroupInput,
    ) -> Result<MoveCreatorsToGroupResult, CreatorGroupError> {
        move_creators_to_group(self, input.creator_ids, input.destination)
    }

    fn set_creator_pinned(&self, creator_id: &CreatorId, pinned: bool) -> Result<(), String> {
        let timestamp = now();
        let connection = self.lock()?;
        // 置顶是工作台排序偏好,不触碰 updated_at:列表的主排序键不动。
        let changed = connection
            .execute(
                "UPDATE creators SET pinned_at = CASE WHEN ?2 THEN ?3 ELSE NULL END
                 WHERE id = ?1",
                params![creator_id.as_str(), pinned, timestamp],
            )
            .map_err(|error| error.to_string())?;
        if changed == 0 {
            return Err("creator not found".into());
        }
        Ok(())
    }

    fn set_creator_stage(
        &self,
        creator_id: &CreatorId,
        stage: CreatorStage,
    ) -> Result<CreatorRelationship, String> {
        let timestamp = now();
        let connection = self.lock()?;
        let changed = connection
            .execute(
                "UPDATE creator_relationships SET stage = ?2, updated_at = ?3
                 WHERE creator_id = ?1",
                params![creator_id.as_str(), stage.as_str(), timestamp],
            )
            .map_err(|error| error.to_string())?;
        if changed == 0 {
            return Err("creator not found".into());
        }
        load_creator_relationship(&connection, creator_id)
    }

    fn update_creator(&self, input: crate::UpdateCreatorInput) -> Result<CreatorRecord, String> {
        let creator_id = input.creator_id.clone();
        let timestamp = now();
        let mut connection = self.lock()?;
        let transaction = connection
            .transaction()
            .map_err(|error| error.to_string())?;
        if let Some(display_name) = input.display_name.as_deref() {
            transaction
                .execute(
                    "UPDATE creators SET display_name = ?2, updated_at = ?3 WHERE id = ?1",
                    params![creator_id.as_str(), display_name, timestamp],
                )
                .map_err(|error| error.to_string())?;
        }
        if let Some(notes) = input.notes.as_deref() {
            transaction
                .execute(
                    "UPDATE creators SET notes = ?2, updated_at = ?3 WHERE id = ?1",
                    params![creator_id.as_str(), notes, timestamp],
                )
                .map_err(|error| error.to_string())?;
        }
        apply_creator_tags_in(&transaction, &creator_id, input.tags, timestamp)?;
        apply_creator_quality_in(
            &transaction,
            &creator_id,
            &crate::CreatorQualityInput {
                score: input.score,
                assessment: input.assessment.clone(),
                follower_counts: input.follower_counts.clone().unwrap_or_default(),
            },
            timestamp,
        )?;
        let record = load_creator_record(&transaction, &creator_id)?;
        transaction.commit().map_err(|error| error.to_string())?;
        Ok(record)
    }

    fn delete_creator(&self, input: DeleteCreatorInput) -> Result<CreatorDeleteResult, String> {
        let creator_id = input.creator_id;
        let mut connection = self.lock()?;
        let transaction = connection
            .transaction()
            .map_err(|error| error.to_string())?;
        let exists: bool = transaction
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM creators WHERE id = ?1)",
                [creator_id.as_str()],
                |row| row.get(0),
            )
            .map_err(|error| error.to_string())?;
        if !exists {
            return Err("creator not found".into());
        }
        if super::outreach_actions::guards::creator_pending(&transaction, &creator_id)? {
            return Err(
                "Creator has an unfinished outreach action; reconcile it before deletion".into(),
            );
        }
        let run = |sql: &str| -> Result<u64, String> {
            transaction
                .execute(sql, [creator_id.as_str()])
                .map(|changed| changed as u64)
                .map_err(|error| error.to_string())
        };
        // Route-linked children first (their FKs RESTRICT the route rows),
        // then references that survive with cleared attribution, then the
        // creator-owned rows, then the creator itself.
        run(
            "DELETE FROM conversation_contact_routes WHERE contact_route_id IN
               (SELECT id FROM creator_contact_routes WHERE creator_id = ?1)",
        )?;
        run(
            "DELETE FROM creator_contact_route_dependencies WHERE parent_contact_route_id IN
               (SELECT id FROM creator_contact_routes WHERE creator_id = ?1)
             OR child_contact_route_id IN
               (SELECT id FROM creator_contact_routes WHERE creator_id = ?1)",
        )?;
        let cleared = run(
            "UPDATE email_drafts SET creator_id = NULL, contact_route_id = NULL
             WHERE creator_id = ?1 OR contact_route_id IN
               (SELECT id FROM creator_contact_routes WHERE creator_id = ?1)",
        )? + run(
            "UPDATE email_threads SET creator_id = NULL, contact_route_id = NULL
             WHERE creator_id = ?1 OR contact_route_id IN
               (SELECT id FROM creator_contact_routes WHERE creator_id = ?1)",
        )? + run(
            "UPDATE audit_events SET creator_id = NULL, contact_route_id = NULL
             WHERE creator_id = ?1 OR contact_route_id IN
               (SELECT id FROM creator_contact_routes WHERE creator_id = ?1)",
        )? + run("UPDATE idempotency_records SET contact_route_id = NULL
             WHERE contact_route_id IN
               (SELECT id FROM creator_contact_routes WHERE creator_id = ?1)")?
            + run("UPDATE creator_origins
             SET source_creator_id = NULL, source_contact_route_id = NULL
             WHERE source_creator_id = ?1 OR source_contact_route_id IN
               (SELECT id FROM creator_contact_routes WHERE creator_id = ?1)")?
            + run("UPDATE locator_observations SET creator_id = NULL
             WHERE creator_id = ?1 AND platform_account_id IS NOT NULL")?;
        run("UPDATE touchpoints SET contact_route_id = NULL
             WHERE contact_route_id IN
               (SELECT id FROM creator_contact_routes WHERE creator_id = ?1)")?;
        run("DELETE FROM locator_observations WHERE creator_id = ?1")?;
        run("DELETE FROM identity_conflicts WHERE creator_id = ?1")?;
        run("DELETE FROM creator_contact_route_delete_operations WHERE creator_id = ?1")?;
        let removed_contact_routes =
            run("DELETE FROM creator_contact_routes WHERE creator_id = ?1")?;
        let removed_touchpoints = run("DELETE FROM touchpoints WHERE creator_id = ?1")?;
        let removed_origins = run("DELETE FROM creator_origins WHERE creator_id = ?1")?;
        let removed_account_links = run("DELETE FROM creator_account_links WHERE creator_id = ?1")?;
        run("DELETE FROM creator_relationships WHERE creator_id = ?1")?;
        run("DELETE FROM creators WHERE id = ?1")?;
        transaction.commit().map_err(|error| error.to_string())?;
        Ok(CreatorDeleteResult {
            creator_id,
            removed_contact_routes,
            removed_touchpoints,
            removed_origins,
            removed_account_links,
            cleared_references: cleared,
        })
    }

    fn archive_creator(&self, creator_id: &CreatorId) -> Result<Creator, String> {
        let timestamp = now();
        let mut connection = self.lock()?;
        let transaction = connection
            .transaction()
            .map_err(|error| error.to_string())?;
        let changed = transaction
            .execute(
                "UPDATE creators SET archived_at = ?2, updated_at = ?2 WHERE id = ?1",
                params![creator_id.as_str(), timestamp],
            )
            .map_err(|error| error.to_string())?;
        if changed == 0 {
            return Err("creator not found".into());
        }
        transaction
            .execute(
                "UPDATE creator_relationships SET stage = 'archived', updated_at = ?2
                 WHERE creator_id = ?1",
                params![creator_id.as_str(), timestamp],
            )
            .map_err(|error| error.to_string())?;
        transaction.commit().map_err(|error| error.to_string())?;
        load_creator(&connection, creator_id)
    }

    fn upsert_platform_account(
        &self,
        input: UpsertPlatformAccountInput,
    ) -> Result<PlatformAccount, String> {
        let external_id = input.external_id.trim();
        if external_id.is_empty() {
            return Err("platform account external_id is required".into());
        }
        let connection = self.lock()?;
        let timestamp = now();
        let existing = connection
            .query_row(
                "SELECT id, created_at FROM platform_accounts
                 WHERE platform = ?1 AND target_kind = ?2 AND external_id = ?3",
                params![
                    input.platform.as_str(),
                    input.target_kind.as_str(),
                    external_id
                ],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, u64>(1)?)),
            )
            .optional()
            .map_err(|error| error.to_string())?;
        let (id, created_at) = existing
            .map(|(id, created_at)| (PlatformAccountId::new(id), created_at))
            .unwrap_or_else(|| {
                (
                    PlatformAccountId::new(Uuid::new_v4().to_string()),
                    timestamp,
                )
            });
        connection
            .execute(
                "INSERT INTO platform_accounts
                   (id, platform, target_kind, external_id, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                 ON CONFLICT(platform, target_kind, external_id) DO UPDATE SET
                   updated_at = excluded.updated_at",
                params![
                    id.as_str(),
                    input.platform.as_str(),
                    input.target_kind.as_str(),
                    external_id,
                    created_at,
                    timestamp
                ],
            )
            .map_err(|error| error.to_string())?;
        Ok(PlatformAccount {
            id,
            platform: input.platform,
            target_kind: input.target_kind,
            external_id: external_id.to_owned(),
            created_at,
            updated_at: timestamp,
        })
    }

    fn link_creator_account(
        &self,
        creator_id: &CreatorId,
        platform_account_id: &PlatformAccountId,
        state: LinkState,
        confidence: Confidence,
    ) -> Result<CreatorAccountLink, String> {
        let connection = self.lock()?;
        let timestamp = now();
        let existing = connection
            .query_row(
                "SELECT id, created_at FROM creator_account_links
                 WHERE creator_id = ?1 AND platform_account_id = ?2",
                params![creator_id.as_str(), platform_account_id.as_str()],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, u64>(1)?)),
            )
            .optional()
            .map_err(|error| error.to_string())?;
        let (id, created_at) = existing
            .map(|(id, created_at)| (LinkId::new(id), created_at))
            .unwrap_or_else(|| {
                (
                    CreatorAccountLinkId::new(Uuid::new_v4().to_string()),
                    timestamp,
                )
            });
        connection
            .execute(
                "INSERT INTO creator_account_links
                   (id, creator_id, platform_account_id, state, confidence, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                 ON CONFLICT(creator_id, platform_account_id) DO UPDATE SET
                   state = excluded.state, confidence = excluded.confidence,
                   updated_at = excluded.updated_at",
                params![
                    id.as_str(),
                    creator_id.as_str(),
                    platform_account_id.as_str(),
                    state.as_str(),
                    confidence.as_str(),
                    created_at,
                    timestamp
                ],
            )
            .map_err(|error| error.to_string())?;
        Ok(CreatorAccountLink {
            id,
            creator_id: creator_id.clone(),
            platform_account_id: platform_account_id.clone(),
            state,
            confidence,
            created_at,
            updated_at: timestamp,
        })
    }

    fn append_platform_account_observation(
        &self,
        input: PlatformAccountObservationInput,
    ) -> Result<PlatformAccountObservation, String> {
        let id = PlatformAccountObservationId::new(Uuid::new_v4().to_string());
        let timestamp = now();
        let connection = self.lock()?;
        connection
            .execute(
                "INSERT INTO platform_account_observations
                   (id, platform_account_id, source_id, handle, display_name, profile_url,
                    public_json, observed_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    id.as_str(),
                    input.platform_account_id.as_str(),
                    input.source_id.as_str(),
                    input.handle,
                    input.display_name,
                    input.profile_url,
                    json(&input.public_data)?,
                    timestamp
                ],
            )
            .map_err(|error| error.to_string())?;
        Ok(PlatformAccountObservation {
            id,
            platform_account_id: input.platform_account_id,
            source_id: input.source_id,
            handle: input.handle,
            display_name: input.display_name,
            profile_url: input.profile_url,
            public_data: input.public_data,
            observed_at: timestamp,
        })
    }

    fn resolve_creator_identity(
        &self,
        candidate: &crate::PlatformAccountCandidate,
    ) -> Result<IdentityResolution, String> {
        let mut connection = self.read_connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Deferred)
            .map_err(|error| error.to_string())?;
        let resolution = super::identity::resolve_creator_identity(&transaction, candidate)?;
        transaction.commit().map_err(|error| error.to_string())?;
        Ok(resolution)
    }

    fn prepare_creator_save_batch(
        &self,
        inputs: Vec<crate::CanonicalCreatorSaveInput>,
        intent: crate::CreatorSaveIntent,
    ) -> Result<crate::PreparedCreatorSaveBatch, crate::CreatorSavePreparationError> {
        super::creator_save::prepare_entries(self, inputs, intent)
    }

    fn execute_creator_save_batch(
        &self,
        prepared: crate::SelectedPreparedCreatorSaveBatch,
        execution_key: String,
    ) -> Result<(crate::CreatorSaveBatchResult, bool), String> {
        super::creator_save::execute_prepared(self, prepared, execution_key)
    }
}

fn normalize_creator_group_name(name: &str) -> Result<String, CreatorGroupError> {
    let name = name.trim();
    if name.is_empty() || name.chars().count() > CREATOR_GROUP_NAME_MAX_CHARS {
        return Err(CreatorGroupError::InvalidInput);
    }
    Ok(name.to_owned())
}

fn load_creator_group(
    connection: &rusqlite::Connection,
    group_id: &CreatorGroupId,
) -> Result<CreatorGroup, CreatorGroupError> {
    connection
        .query_row(
            "SELECT id, name, revision, position, created_at, updated_at
             FROM creator_groups WHERE id = ?1",
            [group_id.as_str()],
            |row| {
                Ok(CreatorGroup {
                    id: CreatorGroupId::new(row.get::<_, String>(0)?),
                    name: row.get(1)?,
                    revision: row.get(2)?,
                    position: row.get(3)?,
                    created_at: row.get(4)?,
                    updated_at: row.get(5)?,
                })
            },
        )
        .optional()
        .map_err(|_| CreatorGroupError::Internal)?
        .ok_or(CreatorGroupError::GroupNotFound)
}

fn ensure_group_revision(
    connection: &rusqlite::Connection,
    group_id: &CreatorGroupId,
    expected_revision: u64,
) -> Result<(), CreatorGroupError> {
    let revision = connection
        .query_row(
            "SELECT revision FROM creator_groups WHERE id = ?1",
            [group_id.as_str()],
            |row| row.get::<_, u64>(0),
        )
        .optional()
        .map_err(|_| CreatorGroupError::Internal)?
        .ok_or(CreatorGroupError::GroupNotFound)?;
    if revision != expected_revision {
        return Err(CreatorGroupError::RevisionConflict);
    }
    Ok(())
}

fn ensure_group_name_available(
    connection: &rusqlite::Connection,
    name: &str,
    excluding: Option<&CreatorGroupId>,
) -> Result<(), CreatorGroupError> {
    let exists = connection
        .query_row(
            "SELECT EXISTS(
               SELECT 1 FROM creator_groups
               WHERE name = ?1 COLLATE NOCASE
                 AND (?2 IS NULL OR id != ?2)
             )",
            params![name, excluding.map(CreatorGroupId::as_str)],
            |row| row.get::<_, bool>(0),
        )
        .map_err(|_| CreatorGroupError::Internal)?;
    if exists {
        return Err(CreatorGroupError::DuplicateName);
    }
    Ok(())
}

fn move_creators_to_group(
    repository: &SqliteRepository,
    creator_ids: Vec<CreatorId>,
    destination: CreatorGroupDestination,
) -> Result<MoveCreatorsToGroupResult, CreatorGroupError> {
    if creator_ids.is_empty() || creator_ids.len() > CREATOR_GROUP_BULK_MOVE_LIMIT {
        return Err(CreatorGroupError::InvalidInput);
    }
    let mut seen = std::collections::HashSet::with_capacity(creator_ids.len());
    if creator_ids.iter().any(|creator_id| {
        creator_id.as_str().trim().is_empty() || !seen.insert(creator_id.as_str())
    }) {
        return Err(CreatorGroupError::InvalidInput);
    }
    let mut connection = repository.lock().map_err(|_| CreatorGroupError::Internal)?;
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|_| CreatorGroupError::Internal)?;
    let destination_group = match &destination {
        CreatorGroupDestination::Uncategorized => None,
        CreatorGroupDestination::Group { group_id } => {
            Some(load_creator_group(&transaction, group_id)?)
        }
    };
    let destination_id = destination_group
        .as_ref()
        .map(|group| group.id.as_str().to_owned());
    let destination_ref = destination_group.as_ref().map(|group| CreatorGroupRef {
        id: group.id.clone(),
        name: group.name.clone(),
    });
    let mut moved_count = 0_u64;
    let mut unchanged_count = 0_u64;
    let mut assignments = Vec::with_capacity(creator_ids.len());
    for creator_id in creator_ids {
        let current_group_id = transaction
            .query_row(
                "SELECT group_id FROM creators WHERE id = ?1",
                [creator_id.as_str()],
                |row| row.get::<_, Option<String>>(0),
            )
            .optional()
            .map_err(|_| CreatorGroupError::Internal)?
            .ok_or(CreatorGroupError::CreatorNotFound)?;
        if current_group_id == destination_id {
            unchanged_count += 1;
        } else {
            transaction
                .execute(
                    "UPDATE creators SET group_id = ?2 WHERE id = ?1",
                    params![creator_id.as_str(), destination_id],
                )
                .map_err(|_| CreatorGroupError::Internal)?;
            moved_count += 1;
        }
        assignments.push(CreatorGroupAssignment {
            creator_id,
            group: destination_ref.clone(),
        });
    }
    transaction
        .commit()
        .map_err(|_| CreatorGroupError::Internal)?;
    Ok(MoveCreatorsToGroupResult {
        assignments,
        moved_count,
        unchanged_count,
    })
}

pub(super) fn load_creator_record(
    connection: &rusqlite::Connection,
    creator_id: &CreatorId,
) -> Result<CreatorRecord, String> {
    let creator = load_creator(connection, creator_id)?;
    let group = load_creator_group_ref(connection, creator_id)?;
    let relationship = load_creator_relationship(connection, creator_id)?;
    let platform_accounts = load_platform_accounts(connection, creator_id)?;
    let account_links = load_account_links(connection, creator_id)?;
    let account_observations = load_account_observations(connection, creator_id)?;
    let observation_sources = load_observation_sources(connection, creator_id)?;
    let origins = load_origins(connection, creator_id)?;
    let contact_routes = super::contacts::load_creator_contact_routes(connection, creator_id)?;
    Ok(CreatorRecord {
        creator,
        group,
        relationship,
        platform_accounts,
        account_links,
        account_observations,
        observation_sources,
        origins,
        contact_routes,
    })
}

fn load_creator_group_ref(
    connection: &rusqlite::Connection,
    creator_id: &CreatorId,
) -> Result<Option<CreatorGroupRef>, String> {
    connection
        .query_row(
            "SELECT creator_group.id, creator_group.name
             FROM creators creator
             LEFT JOIN creator_groups creator_group
               ON creator_group.id = creator.group_id
             WHERE creator.id = ?1",
            [creator_id.as_str()],
            |row| {
                Ok(row
                    .get::<_, Option<String>>(0)?
                    .zip(row.get::<_, Option<String>>(1)?)
                    .map(|(id, name)| CreatorGroupRef {
                        id: CreatorGroupId::new(id),
                        name,
                    }))
            },
        )
        .map_err(|error| error.to_string())
}

fn load_creator(
    connection: &rusqlite::Connection,
    creator_id: &CreatorId,
) -> Result<Creator, String> {
    connection
        .query_row(
            "SELECT id, display_name, notes, score, assessment, follower_counts_json,
                    created_at, updated_at, archived_at, pinned_at, tags_json
             FROM creators WHERE id = ?1",
            [creator_id.as_str()],
            |row| {
                let follower_counts: String = row.get(5)?;
                Ok(Creator {
                    id: CreatorId::new(row.get::<_, String>(0)?),
                    display_name: row.get(1)?,
                    tags: serde_json::from_str(&row.get::<_, String>(10)?)
                        .map_err(|error| to_sql_conversion_error(error.to_string()))?,
                    notes: row.get(2)?,
                    score: row.get(3)?,
                    assessment: row.get(4)?,
                    follower_counts: parse_follower_counts(&follower_counts)
                        .map_err(to_sql_conversion_error)?,
                    created_at: row.get(6)?,
                    updated_at: row.get(7)?,
                    archived_at: row.get(8)?,
                    pinned_at: row.get(9)?,
                })
            },
        )
        .optional()
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "creator not found".into())
}

pub(super) fn parse_follower_counts(
    raw: &str,
) -> Result<std::collections::BTreeMap<Platform, u64>, String> {
    serde_json::from_str(raw)
        .map_err(|error| format!("stored follower counts are invalid: {error}"))
}

/// Applies agent-quality facts to one Creator row inside the caller's
/// transaction: provided fields overwrite and `follower_counts` merges
/// per platform.
pub(super) fn apply_creator_quality_in(
    connection: &rusqlite::Connection,
    creator_id: &CreatorId,
    quality: &crate::CreatorQualityInput,
    timestamp: u64,
) -> Result<(), String> {
    if quality.is_empty() {
        return Ok(());
    }
    let (current_score, current_assessment, current_counts_raw): (
        Option<u8>,
        Option<String>,
        String,
    ) = connection
        .query_row(
            "SELECT score, assessment, follower_counts_json FROM creators WHERE id = ?1",
            [creator_id.as_str()],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "creator not found".to_string())?;
    let score = quality.score.or(current_score);
    let assessment = quality.assessment.clone().or(current_assessment);
    let mut follower_counts = parse_follower_counts(&current_counts_raw)?;
    for (platform, count) in &quality.follower_counts {
        follower_counts.insert(*platform, *count);
    }
    let follower_counts_json =
        serde_json::to_string(&follower_counts).map_err(|error| error.to_string())?;
    connection
        .execute(
            "UPDATE creators
             SET score = ?2, assessment = ?3, follower_counts_json = ?4, updated_at = ?5
             WHERE id = ?1",
            params![
                creator_id.as_str(),
                score,
                assessment,
                follower_counts_json,
                timestamp
            ],
        )
        .map_err(|error| error.to_string())?;
    Ok(())
}

/// The last time we reached out. Three sources, because the evidence does not
/// live in one table:
///   1. `touchpoints` — every send Reacher itself committed (five writers).
///   2. `emails` — outbound mail older than those touchpoint writers.
///   3. `messages` — outbound DMs that arrived by conversation sync, which
///      writes messages without a touchpoint, so source 1 alone misses them.
///
/// Outbound only: an inbound reply is a different fact, and the neighbouring
/// `last_contact_at` projection is outbound-only too — two adjacent fields that
/// look like the same thing and answer differently is worse than one narrow one.
/// `messages.direction` carries no CHECK constraint; `'outbound'` is the
/// writer-side convention.
///
/// The value can move backwards: purging a contact route deletes the
/// touchpoints that reference it.
fn last_outbound_touch_at(
    connection: &rusqlite::Connection,
    creator_id: &str,
) -> Result<Option<u64>, String> {
    connection
        .prepare_cached(&format!("SELECT {}", list::last_touch_expression("?1")))
        .map_err(|error| error.to_string())?
        .query_row([creator_id], |row| row.get::<_, Option<u64>>(0))
        .map_err(|error| error.to_string())
}

fn load_creator_relationship(
    connection: &rusqlite::Connection,
    creator_id: &CreatorId,
) -> Result<CreatorRelationship, String> {
    let mut relationship = connection
        .query_row(
            "SELECT creator_id, stage, created_at, updated_at
             FROM creator_relationships WHERE creator_id = ?1",
            [creator_id.as_str()],
            |row| {
                let stage: String = row.get(1)?;
                Ok(CreatorRelationship {
                    creator_id: CreatorId::new(row.get::<_, String>(0)?),
                    stage: CreatorStage::parse(&stage).map_err(to_sql_conversion_error)?,
                    last_touch_at: None,
                    created_at: row.get(2)?,
                    updated_at: row.get(3)?,
                })
            },
        )
        .map_err(|error| error.to_string())?;
    relationship.last_touch_at = last_outbound_touch_at(connection, creator_id.as_str())?;
    Ok(relationship)
}

fn load_platform_accounts(
    connection: &rusqlite::Connection,
    creator_id: &CreatorId,
) -> Result<Vec<PlatformAccount>, String> {
    let mut statement = connection
        .prepare(
            "SELECT pa.id, pa.platform, pa.target_kind, pa.external_id,
                    pa.created_at, pa.updated_at
             FROM platform_accounts pa
             JOIN creator_account_links link ON link.platform_account_id = pa.id
             WHERE link.creator_id = ?1 AND link.state != 'rejected'
             ORDER BY pa.created_at ASC",
        )
        .map_err(|error| error.to_string())?;
    statement
        .query_map([creator_id.as_str()], |row| {
            let platform: String = row.get(1)?;
            let target_kind: String = row.get(2)?;
            Ok(PlatformAccount {
                id: PlatformAccountId::new(row.get::<_, String>(0)?),
                platform: Platform::parse(&platform).map_err(to_sql_conversion_error)?,
                target_kind: TargetKind::parse(&target_kind).map_err(to_sql_conversion_error)?,
                external_id: row.get(3)?,
                created_at: row.get(4)?,
                updated_at: row.get(5)?,
            })
        })
        .map_err(|error| error.to_string())?
        .map(|row| row.map_err(|error| error.to_string()))
        .collect()
}

fn load_account_links(
    connection: &rusqlite::Connection,
    creator_id: &CreatorId,
) -> Result<Vec<CreatorAccountLink>, String> {
    let mut statement = connection
        .prepare(
            "SELECT id, creator_id, platform_account_id, state, confidence,
                    created_at, updated_at
             FROM creator_account_links WHERE creator_id = ?1 ORDER BY created_at ASC",
        )
        .map_err(|error| error.to_string())?;
    statement
        .query_map([creator_id.as_str()], |row| {
            let state: String = row.get(3)?;
            let confidence: String = row.get(4)?;
            Ok(CreatorAccountLink {
                id: CreatorAccountLinkId::new(row.get::<_, String>(0)?),
                creator_id: CreatorId::new(row.get::<_, String>(1)?),
                platform_account_id: PlatformAccountId::new(row.get::<_, String>(2)?),
                state: LinkState::parse(&state).map_err(to_sql_conversion_error)?,
                confidence: Confidence::parse(&confidence).map_err(to_sql_conversion_error)?,
                created_at: row.get(5)?,
                updated_at: row.get(6)?,
            })
        })
        .map_err(|error| error.to_string())?
        .map(|row| row.map_err(|error| error.to_string()))
        .collect()
}

fn load_account_observations(
    connection: &rusqlite::Connection,
    creator_id: &CreatorId,
) -> Result<Vec<PlatformAccountObservation>, String> {
    let mut statement = connection
        .prepare(
            "SELECT observation.id, observation.platform_account_id, observation.source_id,
                    observation.handle, observation.display_name, observation.profile_url,
                    observation.public_json, observation.observed_at
             FROM platform_account_observations observation
             JOIN creator_account_links link
               ON link.platform_account_id = observation.platform_account_id
             WHERE link.creator_id = ?1 ORDER BY observation.observed_at DESC, observation.rowid DESC",
        )
        .map_err(|error| error.to_string())?;
    statement
        .query_map([creator_id.as_str()], |row| {
            let public_json: String = row.get(6)?;
            Ok(PlatformAccountObservation {
                id: PlatformAccountObservationId::new(row.get::<_, String>(0)?),
                platform_account_id: PlatformAccountId::new(row.get::<_, String>(1)?),
                source_id: ObservationSourceId::new(row.get::<_, String>(2)?),
                handle: row.get(3)?,
                display_name: row.get(4)?,
                profile_url: row.get(5)?,
                public_data: serde_json::from_str(&public_json)
                    .map_err(|error| to_sql_conversion_error(error.to_string()))?,
                observed_at: row.get(7)?,
            })
        })
        .map_err(|error| error.to_string())?
        .map(|row| row.map_err(|error| error.to_string()))
        .collect()
}

fn load_observation_sources(
    connection: &rusqlite::Connection,
    creator_id: &CreatorId,
) -> Result<Vec<ObservationSource>, String> {
    let mut statement = connection
        .prepare(LOAD_OBSERVATION_SOURCES_SQL)
        .map_err(|error| error.to_string())?;
    statement
        .query_map(
            rusqlite::named_params! { ":creator_id": creator_id.as_str() },
            |row| {
                let kind: String = row.get(1)?;
                let platform = row
                    .get::<_, Option<String>>(2)?
                    .map(|value| Platform::parse(&value).map_err(to_sql_conversion_error))
                    .transpose()?;
                let metadata: String = row.get(6)?;
                Ok(ObservationSource {
                    id: ObservationSourceId::new(row.get::<_, String>(0)?),
                    kind: SourceKind::parse(&kind).map_err(to_sql_conversion_error)?,
                    platform,
                    source_url: row.get(3)?,
                    external_ref: row.get(4)?,
                    account_ref_id: row
                        .get::<_, Option<String>>(5)?
                        .map(crate::AccountRefId::new),
                    metadata: serde_json::from_str(&metadata)
                        .map_err(|error| to_sql_conversion_error(error.to_string()))?,
                    observed_at: row.get(7)?,
                })
            },
        )
        .map_err(|error| error.to_string())?
        .map(|row| row.map_err(|error| error.to_string()))
        .collect()
}

pub(super) const LOAD_OBSERVATION_SOURCES_SQL: &str =
    include_str!("queries/load_observation_sources_q5.sql");

fn load_origins(
    connection: &rusqlite::Connection,
    creator_id: &CreatorId,
) -> Result<Vec<CreatorOrigin>, String> {
    let mut statement = connection
        .prepare(
            "SELECT id, kind, creator_id, source_id, discovery_id, source_creator_id,
                    source_contact_route_id, operator_account_ref_id, source_resolved_target_id,
                    administrator_role, community_external_id, community_title, community_handle,
                    community_target_kind, import_content_sha256, imported_creator_id,
                    discovered_at
             FROM creator_origins
             WHERE creator_id = ?1 AND source_id IS NOT NULL
             ORDER BY discovered_at DESC",
        )
        .map_err(|error| error.to_string())?;
    statement
        .query_map([creator_id.as_str()], |row| {
            let id = CreatorOriginId::new(row.get::<_, String>(0)?);
            let kind: String = row.get(1)?;
            let creator_id = CreatorId::new(row.get::<_, String>(2)?);
            let source_id = ObservationSourceId::new(row.get::<_, String>(3)?);
            let discovery_id = row.get::<_, Option<String>>(4)?.map(DiscoveryId::new);
            let required = |value: Option<String>, field: &str| {
                value.ok_or_else(|| {
                    to_sql_conversion_error(format!(
                        "creator origin {kind} is missing required field {field}"
                    ))
                })
            };
            match kind.as_str() {
                "platform_search" => Ok(CreatorOrigin::PlatformSearch {
                    id,
                    discovery_id: discovery_id.ok_or_else(|| {
                        to_sql_conversion_error(
                            "creator origin platform_search is missing required field discovery_id"
                                .into(),
                        )
                    })?,
                    account_ref_id: AccountRefId::new(required(
                        row.get(7)?,
                        "operator_account_ref_id",
                    )?),
                    creator_id,
                    source_id,
                    discovered_at: row.get(16)?,
                }),
                "community_administrator" => Ok(CreatorOrigin::CommunityAdministrator {
                    id,
                    discovery_id: discovery_id.ok_or_else(|| {
                        to_sql_conversion_error(
                            "creator origin community_administrator is missing required field discovery_id"
                                .into(),
                        )
                    })?,
                    creator_id,
                    source_id,
                    role: CommunityAdministratorRole::parse(&required(
                        row.get(9)?,
                        "administrator_role",
                    )?)
                    .map_err(to_sql_conversion_error)?,
                    community: CommunityReference {
                        external_id: required(row.get(10)?, "community_external_id")?,
                        title: required(row.get(11)?, "community_title")?,
                        handle: row.get(12)?,
                        target_kind: CommunityTargetKind::parse(&required(
                            row.get(13)?,
                            "community_target_kind",
                        )?)
                        .map_err(to_sql_conversion_error)?,
                    },
                    // The operator-account source has no source Creator; the
                    // contact-route source always names one.
                    source: match row.get::<_, Option<String>>(5)? {
                        None => CommunityAdministratorSource::OperatorAccount {
                            operator_account_ref_id: AccountRefId::new(required(
                                row.get(7)?,
                                "operator_account_ref_id",
                            )?),
                        },
                        Some(source_creator_id) => {
                            CommunityAdministratorSource::CreatorContactRoute {
                                source_creator_id: CreatorId::new(source_creator_id),
                                source_contact_route_id: row
                                    .get::<_, Option<String>>(6)?
                                    .map(CreatorContactRouteId::new),
                                operator_account_ref_id: AccountRefId::new(required(
                                    row.get(7)?,
                                    "operator_account_ref_id",
                                )?),
                                source_resolved_target_id: ResolvedTargetId::new(required(
                                    row.get(8)?,
                                    "source_resolved_target_id",
                                )?),
                            }
                        }
                    },
                    discovered_at: row.get(16)?,
                }),
                "manual_entry" => Ok(CreatorOrigin::ManualEntry {
                    id,
                    creator_id,
                    source_id,
                    created_at: row.get(16)?,
                }),
                "creator_import" => Ok(CreatorOrigin::CreatorImport {
                    id,
                    creator_id,
                    source_id,
                    content_sha256: required(row.get(14)?, "import_content_sha256")?,
                    imported_creator_id: CreatorId::new(required(
                        row.get(15)?,
                        "imported_creator_id",
                    )?),
                    imported_at: row.get(16)?,
                }),
                _ => Err(to_sql_conversion_error(format!(
                    "unknown creator origin kind: {kind}"
                ))),
            }
        })
        .map_err(|error| error.to_string())?
        .map(|row| row.map_err(|error| error.to_string()))
        .collect()
}

/// Fill the identity presentation facts of one list row: the first-captured
/// (earliest, non-rejected) account's platform and latest handle, the
/// creator-wide avatar (resolved across accounts by `creator_avatar_url`,
/// not necessarily that same account's), plus the distinct platform and
/// contact-channel sets.
fn enrich_summary_identity(
    connection: &rusqlite::Connection,
    summary: &mut CreatorSummary,
) -> Result<(), String> {
    let creator_id = summary.id.as_str();
    let platforms = connection
        .prepare_cached(
            "SELECT account.platform, MIN(account.created_at) AS first_captured
             FROM platform_accounts account
             JOIN creator_account_links link ON link.platform_account_id = account.id
             WHERE link.creator_id = ?1 AND link.state != 'rejected'
             GROUP BY account.platform
             ORDER BY first_captured ASC, MIN(account.rowid) ASC",
        )
        .map_err(|error| error.to_string())?
        .query_map([creator_id], |row| row.get::<_, String>(0))
        .map_err(|error| error.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;
    summary.platforms = platforms
        .iter()
        .map(|platform| crate::Platform::parse(platform))
        .collect::<Result<Vec<_>, _>>()?;

    let primary = connection
        .prepare_cached(
            "SELECT account.platform, observation.handle
             FROM platform_accounts account
             JOIN creator_account_links link ON link.platform_account_id = account.id
             LEFT JOIN platform_account_observations observation
               ON observation.id = (
                 SELECT latest.id FROM platform_account_observations latest
                 WHERE latest.platform_account_id = account.id
                 ORDER BY latest.observed_at DESC, latest.rowid DESC LIMIT 1)
             WHERE link.creator_id = ?1 AND link.state != 'rejected'
             ORDER BY account.created_at ASC, account.rowid ASC
             LIMIT 1",
        )
        .map_err(|error| error.to_string())?
        .query_row([creator_id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?))
        })
        .optional()
        .map_err(|error| error.to_string())?;
    if let Some((platform, handle)) = primary {
        let platform = crate::Platform::parse(&platform)?;
        summary.primary_platform = Some(platform);
        summary.primary_handle = handle
            .map(|handle| handle.trim().to_string())
            .filter(|handle| !handle.is_empty());
    }
    // 头像走 creator 级解析:首个收录账号优先,它没有就顺延,免得
    // 一个只有 Telegram 观察的主账号把整个人显示成无脸。
    summary.avatar_url = creator_avatar_url(connection, creator_id)?;

    let channels = connection
        .prepare_cached(
            "SELECT locator.channel, MIN(route.created_at) AS first_saved
             FROM creator_contact_routes route
             JOIN contact_locators locator ON locator.id = route.locator_id
             WHERE route.creator_id = ?1
             GROUP BY locator.channel
             ORDER BY first_saved ASC, MIN(route.rowid) ASC",
        )
        .map_err(|error| error.to_string())?
        .query_map([creator_id], |row| row.get::<_, String>(0))
        .map_err(|error| error.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;
    summary.channels = channels
        .iter()
        .map(|channel| crate::ContactChannel::parse(channel))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(())
}

/// 一个 Creator 对外展示的头像:按账号首次收录顺序取第一个有头像的
/// 观察。同一个人在各处应当是同一张脸,首个收录账号优先;它没有头像时
/// (例如 Telegram 账号常常没有)顺延到下一个有的。
pub(super) fn creator_avatar_url(
    connection: &rusqlite::Connection,
    creator_id: &str,
) -> Result<Option<String>, String> {
    let mut statement = connection
        .prepare_cached(
            "SELECT account.platform, observation.public_json
             FROM platform_accounts account
             JOIN creator_account_links link ON link.platform_account_id = account.id
             LEFT JOIN platform_account_observations observation
               ON observation.id = (
                 SELECT latest.id FROM platform_account_observations latest
                 WHERE latest.platform_account_id = account.id
                 ORDER BY latest.observed_at DESC, latest.rowid DESC LIMIT 1)
             WHERE link.creator_id = ?1 AND link.state != 'rejected'
             ORDER BY account.created_at ASC, account.rowid ASC",
        )
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map([creator_id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?))
        })
        .map_err(|error| error.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;
    for (platform, public_json) in rows {
        let platform = crate::Platform::parse(&platform)?;
        let avatar = public_json
            .and_then(|json| serde_json::from_str::<serde_json::Value>(&json).ok())
            .and_then(|public_data| avatar_url_from_public_data(platform, &public_data));
        if avatar.is_some() {
            return Ok(avatar);
        }
    }
    Ok(None)
}

/// Canonical observations name the avatar directly. The platform-specific
/// keys below remain readers of historical v36 observations only.
pub(super) fn avatar_url_from_public_data(
    platform: crate::Platform,
    public_data: &serde_json::Value,
) -> Option<String> {
    if let Some(avatar) = public_data
        .get("avatar_url")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        return Some(avatar.to_owned());
    }
    let keys: &[&str] = match platform {
        crate::Platform::X => &["profile_image_url"],
        crate::Platform::Instagram => &["profile_pic_url", "profile_pic_url_hd"],
        crate::Platform::Telegram => &["photo_url", "profile_photo_url"],
        crate::Platform::YouTube => &["thumbnail_url"],
        crate::Platform::TikTok => &["profile_image"],
    };
    keys.iter().find_map(|key| {
        public_data
            .get(key)
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
    })
}

pub(super) fn to_sql_conversion_error(error: String) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(
        0,
        rusqlite::types::Type::Text,
        Box::new(std::io::Error::other(error)),
    )
}

pub(super) fn apply_creator_tags_in(
    connection: &rusqlite::Connection,
    creator_id: &CreatorId,
    tags: Option<Vec<String>>,
    timestamp: u64,
) -> Result<(), String> {
    if let Some(tags) = tags {
        let tags =
            crate::creators::normalize_creator_tags(tags).map_err(|error| error.to_string())?;
        connection
            .execute(
                "UPDATE creators SET tags_json = ?2, updated_at = ?3 WHERE id = ?1",
                params![creator_id.as_str(), json(&tags)?, timestamp],
            )
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}
