//! Provider-neutral Creator bundle import validation, planning, and mutation materialization.

use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::{
    ConfirmCreatorImportResult, CreatorBundle, CreatorId, CreatorImportAccountKey,
    CreatorImportConflict, CreatorImportConflictKind, CreatorImportImpact,
    CreatorImportOperationId, CreatorImportReceipt, CreatorTransferAccount,
    CreatorTransferContactRoute, CreatorTransferLinkState, CreatorTransferPlatform,
    CreatorTransferTargetKind, PrepareCreatorImportResult,
};

pub(crate) const PREPARE_AGAIN_ACTION: &str =
    "choose the Creator bundle again and prepare a fresh import";

fn unix_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub(crate) struct AccountNaturalKey {
    pub(crate) platform: CreatorTransferPlatform,
    pub(crate) target_kind: CreatorTransferTargetKind,
    pub(crate) external_id: String,
}

impl AccountNaturalKey {
    pub(crate) fn from_account(account: &CreatorTransferAccount) -> Self {
        Self {
            platform: account.platform,
            target_kind: account.target_kind,
            external_id: account.external_id.clone(),
        }
    }

    fn public(&self) -> CreatorImportAccountKey {
        CreatorImportAccountKey {
            platform: self.platform,
            target_kind: self.target_kind,
            external_id: self.external_id.clone(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ImportResolution {
    New,
    PublicIdentity,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct CreatorImportPlan {
    pub(crate) content_sha256: String,
    pub(crate) exact_restore: bool,
    pub(crate) creators: Vec<CreatorImportPlanItem>,
    pub(crate) impact: CreatorImportImpact,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct CreatorImportPlanItem {
    pub(crate) incoming: crate::CreatorTransferCreator,
    pub(crate) target_creator_id: CreatorId,
    pub(crate) resolution: ImportResolution,
    pub(crate) create_creator: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) tags_after: Option<Vec<String>>,
    pub(crate) fill_score: Option<u8>,
    pub(crate) follower_counts_after: Option<BTreeMap<CreatorTransferPlatform, u64>>,
    pub(crate) accounts: Vec<CreatorImportAccountPlan>,
    pub(crate) contacts: Vec<CreatorImportContactPlan>,
    pub(crate) changed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct CreatorImportAccountPlan {
    pub(crate) incoming: CreatorTransferAccount,
    pub(crate) existing_account_id: Option<String>,
    pub(crate) existing_link_state: Option<String>,
    pub(crate) create_link: bool,
    pub(crate) promote_to_confirmed: bool,
    pub(crate) profile_to_write: Option<ProfileFacts>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct CreatorImportContactPlan {
    pub(crate) incoming: CreatorTransferContactRoute,
    pub(crate) target_locator_id: String,
    pub(crate) create_locator: bool,
    pub(crate) create_route: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct ProfileFacts {
    pub(crate) handle: Option<String>,
    pub(crate) display_name: Option<String>,
    pub(crate) profile_url: Option<String>,
    pub(crate) public_data: serde_json::Value,
}

#[derive(Debug, Clone)]
pub(crate) struct PlannedImport {
    pub(crate) plan: Option<CreatorImportPlan>,
    pub(crate) conflicts: Vec<CreatorImportConflict>,
}

#[derive(Debug)]
pub(crate) struct ResolvedCreator<'a> {
    pub(crate) incoming: &'a crate::CreatorTransferCreator,
    pub(crate) target_creator_id: CreatorId,
    pub(crate) resolution: ImportResolution,
}

#[derive(Debug, Clone)]
pub(crate) struct ExistingCreatorFacts {
    pub(crate) tags: Vec<String>,
    pub(crate) score: Option<u8>,
    pub(crate) follower_counts: BTreeMap<CreatorTransferPlatform, u64>,
}

#[derive(Debug, Clone)]
pub(crate) struct ExistingLink {
    pub(crate) creator_id: CreatorId,
    pub(crate) state: String,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub(crate) struct AccountLookupRow {
    platform: CreatorTransferPlatform,
    target_kind: CreatorTransferTargetKind,
    external_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub(crate) struct HandleLookupRow {
    platform: CreatorTransferPlatform,
    target_kind: CreatorTransferTargetKind,
    handle: String,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub(crate) struct ContactNaturalKey {
    pub(crate) channel: String,
    pub(crate) kind: String,
    pub(crate) normalized_value: String,
}

impl ContactNaturalKey {
    fn from_route(route: &CreatorTransferContactRoute) -> Self {
        Self {
            channel: route.channel.as_str().to_owned(),
            kind: route.locator_kind.as_str().to_owned(),
            normalized_value: route.normalized_value.clone(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ValidatedImportArtifact {
    pub(crate) historical_content_sha256: Vec<String>,
    pub(crate) bundle: CreatorBundle,
    pub(crate) canonical_json: String,
    pub(crate) account_lookup_json: String,
    pub(crate) handle_lookup_json: String,
    pub(crate) contact_lookup_json: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct BasisAccountRow {
    pub(crate) key: AccountNaturalKey,
    pub(crate) account_id: String,
    pub(crate) creator_id: Option<String>,
    pub(crate) link_state: Option<String>,
    pub(crate) creator_score: Option<u8>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(crate) tags: Vec<String>,
    pub(crate) follower_counts_json: Option<String>,
    pub(crate) profile_handle: Option<String>,
    pub(crate) profile_display_name: Option<String>,
    pub(crate) profile_url: Option<String>,
    pub(crate) profile_public_json: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub(crate) struct BasisHandleOwnerRow {
    pub(crate) platform: CreatorTransferPlatform,
    pub(crate) target_kind: CreatorTransferTargetKind,
    pub(crate) handle: String,
    pub(crate) creator_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct BasisContactRow {
    pub(crate) key: ContactNaturalKey,
    pub(crate) locator_id: String,
    pub(crate) creator_id: Option<String>,
}

/// Planner-relevant rows loaded by one query-only DEFERRED snapshot. The rows
/// deliberately exclude timestamps and every unrelated Creator fact.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ImportBasis {
    pub(crate) domain_empty: bool,
    pub(crate) accounts: Vec<BasisAccountRow>,
    pub(crate) handle_owners: Vec<BasisHandleOwnerRow>,
    pub(crate) contacts: Vec<BasisContactRow>,
}

#[derive(Debug, Clone, Serialize)]
struct MaterializedAccountAction {
    incoming: CreatorTransferAccount,
    account_id: String,
    create_account: bool,
    link_id: Option<String>,
    promote_to_confirmed: bool,
    profile: Option<ProfileFacts>,
    profile_public_json: Option<String>,
    profile_observation_id: Option<String>,
    profile_source_id: Option<String>,
    account_created_at: u64,
}

#[derive(Debug, Clone, Serialize)]
struct MaterializedContactObservation {
    id: String,
    source_id: String,
    observed_at: u64,
}

#[derive(Debug, Clone, Serialize)]
struct MaterializedContactAction {
    incoming: CreatorTransferContactRoute,
    locator_id: String,
    create_locator: bool,
    route_id: Option<String>,
    observations: Vec<MaterializedContactObservation>,
    created_at: u64,
}

#[derive(Debug, Clone, Serialize)]
struct MaterializedCreatorAction {
    incoming: crate::CreatorTransferCreator,
    creator_id: CreatorId,
    create_creator: bool,
    fill_score: Option<u8>,
    tags_after: Option<Vec<String>>,
    follower_counts_json: Option<String>,
    accounts: Vec<MaterializedAccountAction>,
    contacts: Vec<MaterializedContactAction>,
}

#[derive(Debug, Clone)]
pub(crate) struct MaterializedImport {
    pub(crate) sources_json: String,
    pub(crate) creator_inserts_json: String,
    pub(crate) creator_updates_json: String,
    pub(crate) accounts_json: String,
    pub(crate) links_json: String,
    pub(crate) link_updates_json: String,
    pub(crate) profiles_json: String,
    pub(crate) locators_json: String,
    pub(crate) routes_json: String,
    pub(crate) locator_observations_json: String,
    pub(crate) source_metadata_json: String,
    pub(crate) timestamp: u64,
    #[cfg(test)]
    pub(crate) fail_before_table: Option<&'static str>,
}

impl MaterializedImport {
    /// The table before which a test forces the import transaction to roll
    /// back. Production imports carry no failpoint, so the persistence path
    /// evaluates the same check in every build and never branches on `cfg`.
    pub(crate) fn forced_rollback_table(&self) -> Option<&'static str> {
        #[cfg(test)]
        let forced = self.fail_before_table;
        #[cfg(not(test))]
        let forced = None;
        forced
    }
}

#[derive(Debug, Clone, Serialize)]
struct MaterializedSource {
    id: String,
    kind: crate::SourceKind,
}

#[derive(Debug, Clone)]
#[doc(hidden)]
pub enum PrepareOutcomePayload {
    AlreadyImported {
        content_sha256: String,
        format_version: u16,
        creator_count: u32,
        impact_json: String,
        imported_at: u64,
    },
    ExistingPrepared {
        operation_id: CreatorImportOperationId,
        content_sha256: String,
        impact_json: String,
        expires_at: u64,
    },
    Direct(PrepareCreatorImportResult),
}

#[derive(Debug, Clone)]
pub enum PrepareImportWriteOutcome {
    BasisDrift,
    Finished(PrepareOutcomePayload),
}

#[derive(Debug, Clone)]
pub struct PrepareImportWrite {
    pub(crate) artifact: Arc<ValidatedImportArtifact>,
    pub(crate) basis: ImportBasis,
    pub(crate) planned: PlannedImport,
    pub(crate) operation_id: CreatorImportOperationId,
    pub(crate) plan_fingerprint: Option<String>,
    pub(crate) impact_json: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ImportOperationSnapshot {
    pub(crate) operation_id: CreatorImportOperationId,
    pub(crate) content_sha256: String,
    pub(crate) bundle_json: Option<String>,
    pub(crate) plan_fingerprint: String,
    pub(crate) impact_json: String,
    pub(crate) status: String,
    pub(crate) result_json: Option<String>,
    pub(crate) expires_at: u64,
}

#[derive(Debug, Clone)]
pub(crate) enum ConfirmDecision {
    Stale,
    Apply(Box<MaterializedApply>),
}

#[derive(Debug, Clone)]
pub(crate) struct MaterializedApply {
    pub(crate) materialized: MaterializedImport,
    pub(crate) receipt: CreatorImportReceipt,
}

#[derive(Debug, Clone)]
pub struct ConfirmImportWrite {
    pub(crate) operation: ImportOperationSnapshot,
    pub(crate) artifact: Arc<ValidatedImportArtifact>,
    pub(crate) basis: ImportBasis,
    pub(crate) decision: ConfirmDecision,
    pub(crate) result_json: String,
    pub(crate) stale_result_json: String,
    pub(crate) expired_result_json: String,
    pub(crate) impact_json: String,
    #[cfg(test)]
    pub(crate) timing_sink: Option<Arc<std::sync::Mutex<Option<ImportWriterTiming>>>>,
}

#[cfg(test)]
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct ImportWriterTiming {
    pub(crate) writer_wait: std::time::Duration,
    pub(crate) writer_hold: std::time::Duration,
    pub(crate) transaction_begin: std::time::Duration,
    pub(crate) apply: std::time::Duration,
    pub(crate) commit: std::time::Duration,
}

#[cfg(test)]
impl ConfirmImportWrite {
    pub(crate) fn install_timing_sink(
        &mut self,
    ) -> Arc<std::sync::Mutex<Option<ImportWriterTiming>>> {
        let sink = Arc::new(std::sync::Mutex::new(None));
        self.timing_sink = Some(Arc::clone(&sink));
        sink
    }

    pub(crate) fn install_mutation_failpoint(&mut self, table: &'static str) {
        if let ConfirmDecision::Apply(apply) = &mut self.decision {
            apply.materialized.fail_before_table = Some(table);
        }
    }
}

#[derive(Debug, Clone)]
pub struct ConfirmImportWriteOutcome {
    pub(crate) result_json: String,
    pub(crate) changed: bool,
    pub(crate) publish_change: bool,
}

impl ConfirmImportWriteOutcome {
    pub(crate) const fn should_publish(&self) -> bool {
        self.publish_change
    }

    pub(crate) fn decode(self) -> Result<(ConfirmCreatorImportResult, bool, bool), String> {
        let result = serde_json::from_str(&self.result_json)
            .map_err(|error| format!("stored Creator import result is invalid: {error}"))?;
        Ok((result, self.changed, self.publish_change))
    }
}
pub(crate) fn validated_artifact_from_xlsx(
    bytes: Vec<u8>,
) -> Result<ValidatedImportArtifact, String> {
    validated_artifact_from_bundle(CreatorBundle::from_xlsx_bytes(&bytes)?)
}

pub(crate) fn validated_artifact_from_bundle(
    bundle: CreatorBundle,
) -> Result<ValidatedImportArtifact, String> {
    let canonical_json = String::from_utf8(bundle.to_staged_import_json_bytes()?)
        .map_err(|_| "Creator bundle encoding is not UTF-8".to_string())?;
    let account_lookup = bundle
        .creators
        .iter()
        .flat_map(|creator| &creator.accounts)
        .map(|account| AccountLookupRow {
            platform: account.platform,
            target_kind: account.target_kind,
            external_id: account.external_id.clone(),
        })
        .collect::<BTreeSet<_>>();
    let handle_lookup = bundle
        .creators
        .iter()
        .flat_map(|creator| &creator.accounts)
        .filter(|account| account.link_state != CreatorTransferLinkState::Rejected)
        .filter_map(|account| {
            account
                .latest_profile
                .as_ref()
                .and_then(|profile| profile.handle.as_deref())
                .map(|handle| HandleLookupRow {
                    platform: account.platform,
                    target_kind: account.target_kind,
                    handle: handle.to_ascii_lowercase(),
                })
        })
        .collect::<BTreeSet<_>>();
    let contact_lookup = bundle
        .creators
        .iter()
        .flat_map(|creator| &creator.contact_routes)
        .map(ContactNaturalKey::from_route)
        .collect::<BTreeSet<_>>();
    Ok(ValidatedImportArtifact {
        historical_content_sha256: crate::creator_transfer::historical_import_digests(&bundle)?,
        bundle,
        canonical_json,
        account_lookup_json: serde_json::to_string(&account_lookup)
            .map_err(|error| error.to_string())?,
        handle_lookup_json: serde_json::to_string(&handle_lookup)
            .map_err(|error| error.to_string())?,
        contact_lookup_json: serde_json::to_string(&contact_lookup)
            .map_err(|error| error.to_string())?,
    })
}

pub(crate) fn build_prepare_work(
    artifact: Arc<ValidatedImportArtifact>,
    basis: ImportBasis,
) -> Result<PrepareImportWrite, String> {
    let planned = plan_import(&basis, &artifact.bundle)?;
    let (plan_fingerprint, impact_json) = planned
        .plan
        .as_ref()
        .map(|plan| {
            Ok::<(String, String), String>((
                plan_fingerprint(plan, &basis)?,
                serde_json::to_string(&plan.impact).map_err(|error| error.to_string())?,
            ))
        })
        .transpose()?
        .map_or((None, None), |(fingerprint, impact)| {
            (Some(fingerprint), Some(impact))
        });
    Ok(PrepareImportWrite {
        artifact,
        basis,
        planned,
        operation_id: CreatorImportOperationId::new(Uuid::new_v4().to_string()),
        plan_fingerprint,
        impact_json,
    })
}

pub(crate) fn decode_prepare_outcome(
    outcome: PrepareImportWriteOutcome,
) -> Result<PrepareCreatorImportResult, String> {
    match outcome {
        PrepareImportWriteOutcome::BasisDrift => {
            Err("Creator import basis changed during prepare".into())
        }
        PrepareImportWriteOutcome::Finished(PrepareOutcomePayload::Direct(result)) => Ok(result),
        PrepareImportWriteOutcome::Finished(PrepareOutcomePayload::AlreadyImported {
            content_sha256,
            format_version,
            creator_count,
            impact_json,
            imported_at,
        }) => Ok(PrepareCreatorImportResult::AlreadyImported {
            receipt: CreatorImportReceipt {
                content_sha256,
                format_version,
                creator_count,
                impact: serde_json::from_str(&impact_json)
                    .map_err(|error| format!("stored Creator import impact is invalid: {error}"))?,
                imported_at,
            },
        }),
        PrepareImportWriteOutcome::Finished(PrepareOutcomePayload::ExistingPrepared {
            operation_id,
            content_sha256,
            impact_json,
            expires_at,
        }) => Ok(PrepareCreatorImportResult::ConfirmationRequired {
            operation_id,
            content_sha256,
            impact: serde_json::from_str(&impact_json)
                .map_err(|error| format!("stored Creator import impact is invalid: {error}"))?,
            expires_at,
        }),
    }
}

impl ImportOperationSnapshot {
    pub(crate) fn completed_result(&self) -> Result<Option<ConfirmCreatorImportResult>, String> {
        if self.status != "completed" {
            return Ok(None);
        }
        let json = self
            .result_json
            .as_deref()
            .ok_or_else(|| "completed Creator import has no result".to_string())?;
        serde_json::from_str(json)
            .map(Some)
            .map_err(|error| format!("stored Creator import result is invalid: {error}"))
    }

    pub(crate) fn take_artifact(&mut self) -> Result<ValidatedImportArtifact, String> {
        let json = self
            .bundle_json
            .take()
            .ok_or_else(|| "prepared Creator import has no staged bundle".to_string())?;
        validated_artifact_from_bundle(CreatorBundle::from_staged_import_json_bytes(
            json.as_bytes(),
        )?)
    }
}

pub(crate) fn build_confirm_work(
    operation: ImportOperationSnapshot,
    artifact: Arc<ValidatedImportArtifact>,
    basis: ImportBasis,
) -> Result<ConfirmImportWrite, String> {
    let timestamp = unix_seconds();
    let planned = plan_import(&basis, &artifact.bundle)?;
    let stable = operation.content_sha256 == artifact.bundle.content_sha256
        && planned.conflicts.is_empty()
        && planned.plan.as_ref().is_some_and(|plan| {
            plan_fingerprint(plan, &basis).is_ok_and(|fingerprint| {
                fingerprint == operation.plan_fingerprint
                    && serde_json::to_string(&plan.impact)
                        .is_ok_and(|impact| impact == operation.impact_json)
            })
        });
    let impact_json = serde_json::to_string(
        &planned
            .plan
            .as_ref()
            .map_or_else(CreatorImportImpact::default, |plan| plan.impact.clone()),
    )
    .map_err(|error| error.to_string())?;
    let (decision, result) = if stable {
        let plan = planned
            .plan
            .ok_or_else(|| "Creator import planner returned no plan".to_string())?;
        let materialized = materialize_import(&artifact.bundle, &plan, timestamp)?;
        let receipt = CreatorImportReceipt {
            content_sha256: artifact.bundle.content_sha256.clone(),
            format_version: artifact.bundle.format_version,
            creator_count: artifact.bundle.creator_count,
            impact: plan.impact.clone(),
            imported_at: timestamp,
        };
        let result = result_for_receipt(operation.operation_id.clone(), receipt.clone());
        (
            ConfirmDecision::Apply(Box::new(MaterializedApply {
                materialized,
                receipt,
            })),
            result,
        )
    } else {
        (
            ConfirmDecision::Stale,
            ConfirmCreatorImportResult::Stale {
                operation_id: operation.operation_id.clone(),
                next_action: PREPARE_AGAIN_ACTION.into(),
            },
        )
    };
    let expired = ConfirmCreatorImportResult::Expired {
        operation_id: operation.operation_id.clone(),
        next_action: PREPARE_AGAIN_ACTION.into(),
    };
    let stale = ConfirmCreatorImportResult::Stale {
        operation_id: operation.operation_id.clone(),
        next_action: PREPARE_AGAIN_ACTION.into(),
    };
    Ok(ConfirmImportWrite {
        operation,
        artifact,
        basis,
        decision,
        result_json: serde_json::to_string(&result).map_err(|error| error.to_string())?,
        stale_result_json: serde_json::to_string(&stale).map_err(|error| error.to_string())?,
        expired_result_json: serde_json::to_string(&expired).map_err(|error| error.to_string())?,
        impact_json,
        #[cfg(test)]
        timing_sink: None,
    })
}

fn result_for_receipt(
    operation_id: CreatorImportOperationId,
    receipt: CreatorImportReceipt,
) -> ConfirmCreatorImportResult {
    if receipt.impact.changes_creator_graph() {
        ConfirmCreatorImportResult::Imported {
            operation_id,
            receipt,
        }
    } else {
        ConfirmCreatorImportResult::Unchanged {
            operation_id,
            receipt,
        }
    }
}

#[derive(Debug)]
struct ExistingAccountFacts {
    account_id: String,
    links: Vec<ExistingLink>,
    latest_profile_json: Option<RawProfileFacts>,
}

#[derive(Debug)]
struct RawProfileFacts {
    handle: Option<String>,
    display_name: Option<String>,
    profile_url: Option<String>,
    public_json: String,
}

#[derive(Debug)]
struct ExistingLocatorFacts {
    locator_id: String,
    owners: Vec<CreatorId>,
}

#[derive(Debug)]
struct BasisIndexes {
    accounts: BTreeMap<AccountNaturalKey, ExistingAccountFacts>,
    creators: BTreeMap<String, (Option<u8>, String, Vec<String>)>,
    handle_owners:
        BTreeMap<(CreatorTransferPlatform, CreatorTransferTargetKind, String), Vec<CreatorId>>,
    contacts: BTreeMap<ContactNaturalKey, ExistingLocatorFacts>,
}

impl BasisIndexes {
    fn from_basis(basis: &ImportBasis) -> Self {
        let mut accounts = BTreeMap::<AccountNaturalKey, ExistingAccountFacts>::new();
        let mut creators = BTreeMap::new();
        for row in &basis.accounts {
            let account = accounts
                .entry(row.key.clone())
                .or_insert_with(|| ExistingAccountFacts {
                    account_id: row.account_id.clone(),
                    links: Vec::new(),
                    latest_profile_json: row.profile_public_json.as_ref().map(|public_json| {
                        RawProfileFacts {
                            handle: row.profile_handle.clone(),
                            display_name: row.profile_display_name.clone(),
                            profile_url: row.profile_url.clone(),
                            public_json: public_json.clone(),
                        }
                    }),
                });
            if let (Some(creator_id), Some(state)) = (&row.creator_id, &row.link_state) {
                account.links.push(ExistingLink {
                    creator_id: CreatorId::new(creator_id.clone()),
                    state: state.clone(),
                });
                if let Some(counts) = row.follower_counts_json.as_ref() {
                    creators.entry(creator_id.clone()).or_insert((
                        row.creator_score,
                        counts.clone(),
                        row.tags.clone(),
                    ));
                }
            }
        }
        let mut handle_owners = BTreeMap::<_, Vec<CreatorId>>::new();
        for row in &basis.handle_owners {
            handle_owners
                .entry((row.platform, row.target_kind, row.handle.clone()))
                .or_default()
                .push(CreatorId::new(row.creator_id.clone()));
        }
        let mut contacts = BTreeMap::<ContactNaturalKey, ExistingLocatorFacts>::new();
        for row in &basis.contacts {
            let locator = contacts
                .entry(row.key.clone())
                .or_insert_with(|| ExistingLocatorFacts {
                    locator_id: row.locator_id.clone(),
                    owners: Vec::new(),
                });
            if let Some(creator_id) = row.creator_id.as_ref() {
                locator.owners.push(CreatorId::new(creator_id.clone()));
            }
        }
        Self {
            accounts,
            creators,
            handle_owners,
            contacts,
        }
    }
}

fn plan_import(basis: &ImportBasis, bundle: &CreatorBundle) -> Result<PlannedImport, String> {
    if basis.domain_empty {
        return plan_exact_restore(bundle);
    }
    let indexes = BasisIndexes::from_basis(basis);
    let mut conflicts = Vec::new();
    let mut incoming_account_owners = BTreeMap::<AccountNaturalKey, String>::new();
    for incoming in &bundle.creators {
        for account in &incoming.accounts {
            if account.link_state == CreatorTransferLinkState::Rejected {
                continue;
            }
            let key = AccountNaturalKey::from_account(account);
            if let Some(owner) =
                incoming_account_owners.insert(key.clone(), incoming.record_key.clone())
                && owner != incoming.record_key
            {
                conflicts.push(CreatorImportConflict {
                    kind: CreatorImportConflictKind::IncomingAccountClaimedByMultipleCreators,
                    incoming_creator_key: incoming.record_key.clone(),
                    account: Some(key.public()),
                    local_creator_ids: Vec::new(),
                });
            }
        }
    }
    if !conflicts.is_empty() {
        return Ok(PlannedImport {
            plan: None,
            conflicts,
        });
    }

    let mut resolved = Vec::with_capacity(bundle.creators.len());
    for incoming in &bundle.creators {
        let public_identity_owners = public_account_owners(&indexes, incoming);
        if public_identity_owners.len() > 1 {
            conflicts.push(CreatorImportConflict {
                kind: CreatorImportConflictKind::IncomingCreatorResolvesToMultipleLocalCreators,
                incoming_creator_key: incoming.record_key.clone(),
                account: None,
                local_creator_ids: public_identity_owners
                    .into_iter()
                    .map(CreatorId::new)
                    .collect(),
            });
        } else if let Some(owner) = public_identity_owners.into_iter().next() {
            resolved.push(ResolvedCreator {
                incoming,
                target_creator_id: CreatorId::new(owner),
                resolution: ImportResolution::PublicIdentity,
            });
        } else {
            let possible = possible_handle_matches(&indexes, incoming);
            if !possible.is_empty() {
                conflicts.push(CreatorImportConflict {
                    kind: CreatorImportConflictKind::PossibleHandleMatch,
                    incoming_creator_key: incoming.record_key.clone(),
                    account: None,
                    local_creator_ids: possible,
                });
                continue;
            }
            let possible = possible_contact_matches(&indexes, incoming);
            if !possible.is_empty() {
                conflicts.push(CreatorImportConflict {
                    kind: CreatorImportConflictKind::PossibleContactMatch,
                    incoming_creator_key: incoming.record_key.clone(),
                    account: None,
                    local_creator_ids: possible,
                });
                continue;
            }
            resolved.push(ResolvedCreator {
                incoming,
                target_creator_id: CreatorId::new(incoming.record_key.clone()),
                resolution: ImportResolution::New,
            });
        }
    }

    let mut targets = BTreeMap::<String, String>::new();
    for resolution in &resolved {
        if let Some(first) = targets.insert(
            resolution.target_creator_id.as_str().to_string(),
            resolution.incoming.record_key.clone(),
        ) && first != resolution.incoming.record_key
        {
            conflicts.push(CreatorImportConflict {
                kind: CreatorImportConflictKind::IncomingCreatorResolvesToMultipleLocalCreators,
                incoming_creator_key: resolution.incoming.record_key.clone(),
                account: None,
                local_creator_ids: vec![resolution.target_creator_id.clone()],
            });
        }
    }
    if !conflicts.is_empty() {
        return Ok(PlannedImport {
            plan: None,
            conflicts,
        });
    }

    let mut impact = CreatorImportImpact {
        creator_inputs: bundle.creator_count.into(),
        ..CreatorImportImpact::default()
    };
    let mut items = Vec::with_capacity(resolved.len());
    for resolution in resolved {
        let create_creator = resolution.resolution == ImportResolution::New;
        let existing = if create_creator {
            None
        } else {
            Some(load_creator_facts(&indexes, &resolution.target_creator_id)?)
        };
        match resolution.resolution {
            ImportResolution::New => impact.creators_created += 1,
            ImportResolution::PublicIdentity => impact.creators_merged_by_public_identity += 1,
        }
        let tags_after = if let Some(facts) = existing.as_ref() {
            if resolution.incoming.tags.is_empty() || resolution.incoming.tags == facts.tags {
                None
            } else if facts.tags.is_empty() {
                impact.creator_tags_filled += 1;
                Some(resolution.incoming.tags.clone())
            } else {
                conflicts.push(CreatorImportConflict {
                    kind: CreatorImportConflictKind::CreatorTagsConflict,
                    incoming_creator_key: resolution.incoming.record_key.clone(),
                    account: None,
                    local_creator_ids: vec![resolution.target_creator_id.clone()],
                });
                None
            }
        } else {
            None
        };
        let fill_score = existing
            .as_ref()
            .filter(|facts| facts.score.is_none())
            .and(resolution.incoming.score);
        if fill_score.is_some() {
            impact.creator_scores_filled += 1;
        }
        let follower_counts_after = if let Some(facts) = existing.as_ref() {
            let mut merged = facts.follower_counts.clone();
            let before = merged.len();
            for (platform, count) in &resolution.incoming.follower_counts {
                merged.entry(*platform).or_insert(*count);
            }
            let filled = merged.len().saturating_sub(before);
            if filled > 0 {
                impact.follower_counts_filled += u64::try_from(filled).map_err(|_| {
                    "Creator import follower count impact cannot be represented".to_string()
                })?;
                Some(merged)
            } else {
                None
            }
        } else {
            None
        };
        let mut account_plans = Vec::with_capacity(resolution.incoming.accounts.len());
        for account in &resolution.incoming.accounts {
            if let Some(plan) = plan_account(
                &indexes,
                &resolution.target_creator_id,
                account,
                &mut impact,
                &mut conflicts,
                &resolution.incoming.record_key,
            )? {
                account_plans.push(plan);
            }
        }
        let mut contact_plans = Vec::with_capacity(resolution.incoming.contact_routes.len());
        for contact in &resolution.incoming.contact_routes {
            if let Some(plan) = plan_contact_route(
                &indexes,
                &resolution.target_creator_id,
                contact,
                &mut impact,
                &mut conflicts,
                &resolution.incoming.record_key,
            )? {
                contact_plans.push(plan);
            }
        }
        let changed = create_creator
            || tags_after.is_some()
            || fill_score.is_some()
            || follower_counts_after.is_some()
            || account_plans.iter().any(|account| {
                account.existing_account_id.is_none()
                    || account.create_link
                    || account.promote_to_confirmed
                    || account.profile_to_write.is_some()
            })
            || contact_plans
                .iter()
                .any(|contact| contact.create_locator || contact.create_route);
        if !changed {
            impact.creators_unchanged += 1;
        }
        items.push(CreatorImportPlanItem {
            incoming: resolution.incoming.clone(),
            target_creator_id: resolution.target_creator_id,
            resolution: resolution.resolution,
            create_creator,
            fill_score,
            tags_after,
            follower_counts_after,
            accounts: account_plans,
            contacts: contact_plans,
            changed,
        });
    }
    if !conflicts.is_empty() {
        return Ok(PlannedImport {
            plan: None,
            conflicts,
        });
    }
    Ok(PlannedImport {
        plan: Some(CreatorImportPlan {
            content_sha256: bundle.content_sha256.clone(),
            exact_restore: false,
            creators: items,
            impact,
        }),
        conflicts,
    })
}

fn plan_exact_restore(bundle: &CreatorBundle) -> Result<PlannedImport, String> {
    let creator_inputs = u64::from(bundle.creator_count);
    let platform_accounts_created = bundle
        .creators
        .iter()
        .flat_map(|creator| creator.accounts.iter())
        .map(|account| {
            (
                account.platform,
                account.target_kind,
                account.external_id.as_str(),
            )
        })
        .collect::<BTreeSet<_>>()
        .len();
    let mut impact = CreatorImportImpact {
        creator_inputs,
        creators_created: creator_inputs,
        platform_accounts_created: u64::try_from(platform_accounts_created)
            .map_err(|_| "Creator account restore count cannot be represented")?,
        ..CreatorImportImpact::default()
    };
    for creator in &bundle.creators {
        impact.contact_routes_created += u64::try_from(creator.contact_routes.len())
            .map_err(|_| "Creator contact restore count cannot be represented")?;
        for account in &creator.accounts {
            match account.link_state {
                CreatorTransferLinkState::Claimed => impact.claimed_links_created += 1,
                CreatorTransferLinkState::Confirmed => impact.confirmed_links_created += 1,
                CreatorTransferLinkState::Rejected => impact.rejected_links_created += 1,
            }
            if account.latest_profile.is_some()
                || account.source_kind != crate::SourceKind::InitialPlatformAccount
            {
                impact.profile_observations_created += 1;
            }
        }
    }
    let creators = bundle
        .creators
        .iter()
        .cloned()
        .map(|incoming| CreatorImportPlanItem {
            target_creator_id: CreatorId::new(incoming.record_key.clone()),
            incoming,
            resolution: ImportResolution::New,
            create_creator: true,
            fill_score: None,
            tags_after: None,
            follower_counts_after: None,
            accounts: Vec::new(),
            contacts: Vec::new(),
            changed: true,
        })
        .collect();
    Ok(PlannedImport {
        plan: Some(CreatorImportPlan {
            content_sha256: bundle.content_sha256.clone(),
            exact_restore: true,
            creators,
            impact,
        }),
        conflicts: Vec::new(),
    })
}

fn plan_account(
    indexes: &BasisIndexes,
    target_creator_id: &CreatorId,
    incoming: &CreatorTransferAccount,
    impact: &mut CreatorImportImpact,
    conflicts: &mut Vec<CreatorImportConflict>,
    incoming_creator_key: &str,
) -> Result<Option<CreatorImportAccountPlan>, String> {
    let key = AccountNaturalKey::from_account(incoming);
    let existing = indexes.accounts.get(&key);
    let existing_account_id = existing.map(|account| account.account_id.clone());
    let links = existing.map_or(&[][..], |account| account.links.as_slice());
    if incoming.link_state != CreatorTransferLinkState::Rejected
        && let Some(rejected) = links.iter().find(|link| link.state == "rejected")
    {
        conflicts.push(CreatorImportConflict {
            kind: CreatorImportConflictKind::RejectedLinkWouldBeRevived,
            incoming_creator_key: incoming_creator_key.to_string(),
            account: Some(key.public()),
            local_creator_ids: vec![rejected.creator_id.clone()],
        });
        return Ok(None);
    }
    if incoming.link_state != CreatorTransferLinkState::Rejected {
        let foreign = links
            .iter()
            .filter(|link| link.creator_id != *target_creator_id && link.state != "rejected")
            .map(|link| link.creator_id.clone())
            .collect::<Vec<_>>();
        if !foreign.is_empty() {
            conflicts.push(CreatorImportConflict {
                kind: CreatorImportConflictKind::AccountOwnedByAnotherCreator,
                incoming_creator_key: incoming_creator_key.to_string(),
                account: Some(key.public()),
                local_creator_ids: foreign,
            });
            return Ok(None);
        }
    }
    let same_link = links
        .iter()
        .find(|link| link.creator_id == *target_creator_id);
    if incoming.link_state == CreatorTransferLinkState::Rejected
        && let Some(active) = same_link.filter(|link| link.state != "rejected")
    {
        conflicts.push(CreatorImportConflict {
            kind: CreatorImportConflictKind::IncomingRejectedLinkConflictsWithActiveLocalLink,
            incoming_creator_key: incoming_creator_key.to_string(),
            account: Some(key.public()),
            local_creator_ids: vec![active.creator_id.clone()],
        });
        return Ok(None);
    }
    let create_link = same_link.is_none();
    let promote_to_confirmed = same_link.is_some_and(|link| {
        link.state == "claimed" && incoming.link_state == CreatorTransferLinkState::Confirmed
    });
    if existing_account_id.is_none() {
        impact.platform_accounts_created += 1;
    }
    if create_link {
        match incoming.link_state {
            CreatorTransferLinkState::Claimed => impact.claimed_links_created += 1,
            CreatorTransferLinkState::Confirmed => impact.confirmed_links_created += 1,
            CreatorTransferLinkState::Rejected => impact.rejected_links_created += 1,
        }
    }
    if promote_to_confirmed {
        impact.links_promoted_to_confirmed += 1;
    }
    let current_profile = existing
        .and_then(|account| account.latest_profile_json.as_ref())
        .map(|profile| {
            Ok::<ProfileFacts, String>(ProfileFacts {
                handle: profile.handle.clone(),
                display_name: profile.display_name.clone(),
                profile_url: profile.profile_url.clone(),
                public_data: serde_json::from_str(&profile.public_json).map_err(|error| {
                    format!("stored Creator profile facts are invalid: {error}")
                })?,
            })
        })
        .transpose()?;
    let profile_to_write = merge_profile(
        incoming.platform,
        current_profile.as_ref(),
        incoming
            .latest_profile
            .as_ref()
            .map(|profile| ProfileFacts::from_transfer(incoming.platform, profile)),
    )
    .or_else(|| {
        (existing_account_id.is_none()
            && incoming.source_kind != crate::SourceKind::InitialPlatformAccount)
            .then(|| ProfileFacts {
                handle: None,
                display_name: None,
                profile_url: None,
                public_data: serde_json::json!({}),
            })
    });
    if profile_to_write.is_some() {
        impact.profile_observations_created += 1;
    }
    Ok(Some(CreatorImportAccountPlan {
        incoming: incoming.clone(),
        existing_account_id,
        existing_link_state: same_link.map(|link| link.state.clone()),
        create_link,
        promote_to_confirmed,
        profile_to_write,
    }))
}

fn plan_contact_route(
    indexes: &BasisIndexes,
    target_creator_id: &CreatorId,
    incoming: &CreatorTransferContactRoute,
    impact: &mut CreatorImportImpact,
    conflicts: &mut Vec<CreatorImportConflict>,
    incoming_creator_key: &str,
) -> Result<Option<CreatorImportContactPlan>, String> {
    let existing = indexes
        .contacts
        .get(&ContactNaturalKey::from_route(incoming));
    let existing_locator = existing.map(|locator| locator.locator_id.clone());
    let target_locator_id = existing_locator.clone().unwrap_or_default();
    let owners = existing.map_or(&[][..], |locator| locator.owners.as_slice());
    let foreign = owners
        .iter()
        .filter(|owner| *owner != target_creator_id)
        .cloned()
        .collect::<Vec<_>>();
    if !foreign.is_empty() {
        conflicts.push(CreatorImportConflict {
            kind: CreatorImportConflictKind::ContactRouteOwnedByAnotherCreator,
            incoming_creator_key: incoming_creator_key.to_string(),
            account: None,
            local_creator_ids: foreign,
        });
        return Ok(None);
    }
    if owners.iter().any(|owner| owner == target_creator_id) {
        return Ok(None);
    }
    impact.contact_routes_created += 1;
    Ok(Some(CreatorImportContactPlan {
        incoming: incoming.clone(),
        target_locator_id,
        create_locator: existing_locator.is_none(),
        create_route: true,
    }))
}

fn public_account_owners(
    indexes: &BasisIndexes,
    incoming: &crate::CreatorTransferCreator,
) -> BTreeSet<String> {
    let mut owners = BTreeSet::new();
    for account in &incoming.accounts {
        if account.link_state == CreatorTransferLinkState::Rejected {
            continue;
        }
        let key = AccountNaturalKey::from_account(account);
        if let Some(account) = indexes.accounts.get(&key) {
            for owner in &account.links {
                if owner.state != "rejected" {
                    owners.insert(owner.creator_id.as_str().to_string());
                }
            }
        }
    }
    owners
}

fn possible_contact_matches(
    indexes: &BasisIndexes,
    incoming: &crate::CreatorTransferCreator,
) -> Vec<CreatorId> {
    let mut matches = BTreeSet::new();
    for route in &incoming.contact_routes {
        if let Some(locator) = indexes.contacts.get(&ContactNaturalKey::from_route(route)) {
            matches.extend(locator.owners.iter().map(|owner| owner.as_str().to_owned()));
        }
    }
    matches.into_iter().map(CreatorId::new).collect()
}

fn possible_handle_matches(
    indexes: &BasisIndexes,
    incoming: &crate::CreatorTransferCreator,
) -> Vec<CreatorId> {
    let mut matches = BTreeSet::<String>::new();
    for account in &incoming.accounts {
        if account.link_state == CreatorTransferLinkState::Rejected {
            continue;
        }
        let Some(handle) = account
            .latest_profile
            .as_ref()
            .and_then(|profile| profile.handle.as_deref())
        else {
            continue;
        };
        if let Some(owners) = indexes.handle_owners.get(&(
            account.platform,
            account.target_kind,
            handle.to_ascii_lowercase(),
        )) {
            matches.extend(owners.iter().map(|owner| owner.as_str().to_owned()));
        }
    }
    matches.into_iter().map(CreatorId::new).collect()
}

fn load_creator_facts(
    indexes: &BasisIndexes,
    creator_id: &CreatorId,
) -> Result<ExistingCreatorFacts, String> {
    let (score, counts, tags) = indexes
        .creators
        .get(creator_id.as_str())
        .ok_or_else(|| format!("resolved Creator {} is missing", creator_id.as_str()))?;
    Ok(ExistingCreatorFacts {
        tags: tags.clone(),
        score: *score,
        follower_counts: serde_json::from_str::<BTreeMap<crate::Platform, u64>>(counts)
            .map_err(|error| format!("stored follower counts are invalid: {error}"))?
            .into_iter()
            .map(|(platform, count)| (transfer_platform(platform), count))
            .collect(),
    })
}

fn merge_profile(
    platform: crate::CreatorTransferPlatform,
    current: Option<&ProfileFacts>,
    incoming: Option<ProfileFacts>,
) -> Option<ProfileFacts> {
    let incoming = incoming?;
    if incoming.handle.is_none()
        && incoming.display_name.is_none()
        && incoming.profile_url.is_none()
        && incoming.public_data == serde_json::json!({})
    {
        return None;
    }
    let public_data = match current {
        Some(current) => {
            merge_transfer_avatar(platform, &current.public_data, &incoming.public_data)
        }
        None => incoming.public_data.clone(),
    };
    let merged = ProfileFacts {
        handle: current
            .and_then(|value| value.handle.clone())
            .or_else(|| incoming.handle.clone()),
        display_name: current
            .and_then(|value| value.display_name.clone())
            .or_else(|| incoming.display_name.clone()),
        profile_url: current
            .and_then(|value| value.profile_url.clone())
            .or_else(|| incoming.profile_url.clone()),
        public_data,
    };
    (current != Some(&merged)).then_some(merged)
}

impl ProfileFacts {
    fn from_transfer(
        platform: crate::CreatorTransferPlatform,
        profile: &crate::CreatorTransferProfile,
    ) -> Self {
        Self {
            handle: profile.handle.clone(),
            display_name: profile.display_name.clone(),
            profile_url: profile.profile_url.clone(),
            public_data: transfer_profile_public_data(platform, profile.avatar_url.as_deref()),
        }
    }
}

fn transfer_avatar_key(platform: crate::CreatorTransferPlatform) -> &'static str {
    match platform {
        crate::CreatorTransferPlatform::X => "profile_image_url",
        crate::CreatorTransferPlatform::Instagram => "profile_pic_url",
        crate::CreatorTransferPlatform::Telegram => "photo_url",
        crate::CreatorTransferPlatform::YouTube => "thumbnail_url",
        crate::CreatorTransferPlatform::TikTok => "profile_image",
    }
}

fn transfer_profile_public_data(
    platform: crate::CreatorTransferPlatform,
    avatar_url: Option<&str>,
) -> serde_json::Value {
    avatar_url.map_or_else(
        || serde_json::json!({}),
        |avatar_url| {
            let mut object = serde_json::Map::new();
            object.insert(
                transfer_avatar_key(platform).to_string(),
                serde_json::Value::String(avatar_url.to_string()),
            );
            serde_json::Value::Object(object)
        },
    )
}

fn merge_transfer_avatar(
    platform: crate::CreatorTransferPlatform,
    current: &serde_json::Value,
    incoming: &serde_json::Value,
) -> serde_json::Value {
    if current
        .get(transfer_avatar_key(platform))
        .and_then(serde_json::Value::as_str)
        .is_some()
    {
        return current.clone();
    }
    let Some(avatar_url) = incoming
        .get(transfer_avatar_key(platform))
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned)
    else {
        return current.clone();
    };
    let mut merged = current.as_object().cloned().unwrap_or_default();
    merged.insert(
        transfer_avatar_key(platform).to_string(),
        serde_json::Value::String(avatar_url),
    );
    serde_json::Value::Object(merged)
}

fn plan_fingerprint(plan: &CreatorImportPlan, basis: &ImportBasis) -> Result<String, String> {
    #[derive(Serialize)]
    struct SemanticFingerprint<'a> {
        plan: &'a CreatorImportPlan,
        basis: &'a ImportBasis,
    }
    let encoded = serde_json::to_string(&SemanticFingerprint { plan, basis })
        .map_err(|error| error.to_string())?;
    let bytes = encoded
        .replace(",\"tags\":[]", "")
        .replace(",\"creator_tags_filled\":0", "")
        .into_bytes();
    Ok(format!("sha256:{:x}", Sha256::digest(bytes)))
}

fn imported_order_created_at(imported_at: u64, max_save_order: u32, save_order: u32) -> u64 {
    imported_at.saturating_sub(u64::from(max_save_order.saturating_sub(save_order)))
}

fn materialize_import(
    bundle: &CreatorBundle,
    plan: &CreatorImportPlan,
    timestamp: u64,
) -> Result<MaterializedImport, String> {
    let max_capture_order = plan
        .creators
        .iter()
        .flat_map(|item| &item.incoming.accounts)
        .map(|account| account.capture_order)
        .max()
        .unwrap_or(0);
    let mut account_ids = BTreeMap::<AccountNaturalKey, String>::new();
    let mut created_accounts = BTreeSet::<AccountNaturalKey>::new();
    let mut exact_profiles = BTreeSet::<AccountNaturalKey>::new();
    let mut locator_ids = BTreeMap::<ContactNaturalKey, String>::new();
    let mut source_ids = BTreeMap::<crate::SourceKind, String>::new();
    if plan.creators.iter().any(|c| c.incoming.notes.is_some()) {
        source_ids.insert(crate::SourceKind::CreatorImport, Uuid::new_v4().to_string());
    }
    let mut creators = Vec::with_capacity(plan.creators.len());
    for item in &plan.creators {
        let creator_id = if item.create_creator {
            CreatorId::new(Uuid::new_v4().to_string())
        } else {
            item.target_creator_id.clone()
        };
        let follower_counts_json = if item.create_creator {
            Some(
                serde_json::to_string(&item.incoming.follower_counts)
                    .map_err(|error| error.to_string())?,
            )
        } else {
            item.follower_counts_after
                .as_ref()
                .map(serde_json::to_string)
                .transpose()
                .map_err(|error| error.to_string())?
        };
        let max_save_order = item
            .incoming
            .contact_routes
            .iter()
            .map(|route| route.save_order)
            .max()
            .unwrap_or(0);
        let account_plans = if plan.exact_restore {
            item.incoming
                .accounts
                .iter()
                .map(|incoming| CreatorImportAccountPlan {
                    incoming: incoming.clone(),
                    existing_account_id: None,
                    existing_link_state: None,
                    create_link: true,
                    promote_to_confirmed: false,
                    profile_to_write: (incoming.latest_profile.is_some()
                        || incoming.source_kind != crate::SourceKind::InitialPlatformAccount)
                        .then(|| {
                            incoming.latest_profile.as_ref().map_or_else(
                                || ProfileFacts {
                                    handle: None,
                                    display_name: None,
                                    profile_url: None,
                                    public_data: serde_json::json!({}),
                                },
                                |profile| ProfileFacts::from_transfer(incoming.platform, profile),
                            )
                        }),
                })
                .collect::<Vec<_>>()
        } else {
            item.accounts.clone()
        };
        let mut accounts = Vec::with_capacity(account_plans.len());
        for account_plan in account_plans {
            let key = AccountNaturalKey::from_account(&account_plan.incoming);
            let account_created_at = if plan.exact_restore {
                imported_order_created_at(
                    timestamp,
                    max_capture_order,
                    account_plan.incoming.capture_order,
                )
            } else {
                timestamp
            };
            let account_id = account_plan.existing_account_id.clone().unwrap_or_else(|| {
                account_ids
                    .entry(key.clone())
                    .or_insert_with(|| Uuid::new_v4().to_string())
                    .clone()
            });
            account_ids.entry(key.clone()).or_insert(account_id.clone());
            let create_account =
                account_plan.existing_account_id.is_none() && created_accounts.insert(key.clone());
            let profile = if plan.exact_restore && !exact_profiles.insert(key.clone()) {
                None
            } else {
                account_plan.profile_to_write
            };
            let profile_source_id = profile.as_ref().map(|_| {
                source_ids
                    .entry(account_plan.incoming.source_kind)
                    .or_insert_with(|| Uuid::new_v4().to_string())
                    .clone()
            });
            let profile_public_json = profile
                .as_ref()
                .map(|facts| serde_json::to_string(&facts.public_data))
                .transpose()
                .map_err(|error| error.to_string())?;
            accounts.push(MaterializedAccountAction {
                incoming: account_plan.incoming,
                account_id,
                create_account,
                link_id: account_plan.create_link.then(|| Uuid::new_v4().to_string()),
                promote_to_confirmed: account_plan.promote_to_confirmed,
                profile_observation_id: profile.as_ref().map(|_| Uuid::new_v4().to_string()),
                profile_source_id,
                profile,
                profile_public_json,
                account_created_at,
            });
        }
        let contact_plans = if plan.exact_restore {
            item.incoming
                .contact_routes
                .iter()
                .map(|incoming| CreatorImportContactPlan {
                    incoming: incoming.clone(),
                    target_locator_id: String::new(),
                    create_locator: true,
                    create_route: true,
                })
                .collect::<Vec<_>>()
        } else {
            item.contacts.clone()
        };
        let mut contacts = Vec::with_capacity(contact_plans.len());
        for contact_plan in contact_plans {
            let key = ContactNaturalKey::from_route(&contact_plan.incoming);
            let created_at = imported_order_created_at(
                timestamp,
                max_save_order,
                contact_plan.incoming.save_order,
            );
            let locator_id = if contact_plan.create_locator {
                locator_ids
                    .entry(key)
                    .or_insert_with(|| Uuid::new_v4().to_string())
                    .clone()
            } else {
                contact_plan.target_locator_id.clone()
            };
            let observations = contact_plan
                .incoming
                .source_kinds
                .iter()
                .enumerate()
                .map(|(index, kind)| {
                    Ok(MaterializedContactObservation {
                        id: Uuid::new_v4().to_string(),
                        source_id: source_ids
                            .entry(*kind)
                            .or_insert_with(|| Uuid::new_v4().to_string())
                            .clone(),
                        observed_at: timestamp.saturating_sub(u64::try_from(index).map_err(
                            |_| "contact source order cannot be represented".to_string(),
                        )?),
                    })
                })
                .collect::<Result<Vec<_>, String>>()?;
            contacts.push(MaterializedContactAction {
                observations,
                incoming: contact_plan.incoming,
                locator_id,
                create_locator: contact_plan.create_locator,
                route_id: contact_plan
                    .create_route
                    .then(|| Uuid::new_v4().to_string()),
                created_at,
            });
        }
        creators.push(MaterializedCreatorAction {
            incoming: item.incoming.clone(),
            creator_id,
            create_creator: item.create_creator,
            fill_score: item.fill_score,
            tags_after: item.tags_after.clone(),
            follower_counts_json,
            accounts,
            contacts,
        });
    }
    let sources = source_ids
        .into_iter()
        .map(|(kind, id)| MaterializedSource { id, kind })
        .collect::<Vec<_>>();
    let creator_inserts = creators
        .iter()
        .filter(|creator| creator.create_creator)
        .map(|creator| {
            serde_json::json!({
                "id": creator.creator_id,
                "display_name": creator.incoming.display_name,
                "notes": creator.incoming.notes,
                "tags_json": serde_json::json!(creator.incoming.tags).to_string(),
                "stage": creator.incoming.relationship.stage,
                "archived_at": (creator.incoming.relationship.stage == crate::CreatorTransferStage::Archived).then_some(timestamp),
                "score": creator.incoming.score,
                "follower_counts_json": creator.follower_counts_json,
                "pinned_at": creator.incoming.pinned.then_some(timestamp),
            })
        })
        .collect::<Vec<_>>();
    let creator_updates = creators
        .iter()
        .filter(|creator| {
            !creator.create_creator
                && (creator.fill_score.is_some() || creator.follower_counts_json.is_some() || creator.tags_after.is_some())
        })
        .map(|creator| {
            serde_json::json!({
                "id": creator.creator_id,
                "fill_score": creator.fill_score,
                "tags_json": creator.tags_after.as_ref().map(|tags| serde_json::json!(tags).to_string()),
                "follower_counts_json": creator.follower_counts_json,
            })
        })
        .collect::<Vec<_>>();
    let accounts = creators
        .iter()
        .flat_map(|creator| &creator.accounts)
        .filter(|account| account.create_account)
        .map(|account| {
            serde_json::json!({
                "id": account.account_id,
                "platform": account.incoming.platform,
                "target_kind": account.incoming.target_kind,
                "external_id": account.incoming.external_id,
                "capture_order": account.incoming.capture_order,
                "created_at": account.account_created_at,
            })
        })
        .collect::<Vec<_>>();
    let links = creators
        .iter()
        .flat_map(|creator| {
            creator.accounts.iter().filter_map(|account| {
                account.link_id.as_ref().map(|link_id| {
                    serde_json::json!({
                        "id": link_id,
                        "creator_id": creator.creator_id,
                        "account_id": account.account_id,
                        "state": account.incoming.link_state,
                        "confidence": account.incoming.confidence,
                    })
                })
            })
        })
        .collect::<Vec<_>>();
    let link_updates = creators
        .iter()
        .flat_map(|creator| {
            creator
                .accounts
                .iter()
                .filter(|account| account.promote_to_confirmed)
                .map(|account| {
                    serde_json::json!({
                        "creator_id": creator.creator_id,
                        "account_id": account.account_id,
                        "confidence": account.incoming.confidence,
                    })
                })
        })
        .collect::<Vec<_>>();
    let profiles = creators
        .iter()
        .flat_map(|creator| &creator.accounts)
        .filter_map(|account| {
            Some(serde_json::json!({
                "id": account.profile_observation_id.as_ref()?,
                "account_id": account.account_id,
                "source_id": account.profile_source_id.as_ref()?,
                "handle": account.profile.as_ref()?.handle,
                "display_name": account.profile.as_ref()?.display_name,
                "profile_url": account.profile.as_ref()?.profile_url,
                "public_json": account.profile_public_json.as_ref()?,
            }))
        })
        .collect::<Vec<_>>();
    let locators = creators
        .iter()
        .flat_map(|creator| &creator.contacts)
        .filter(|contact| contact.create_locator)
        .map(|contact| {
            serde_json::json!({
                "id": contact.locator_id,
                "channel": contact.incoming.channel,
                "kind": contact.incoming.locator_kind,
                "normalized_value": contact.incoming.normalized_value,
                "display_value": contact.incoming.display_value,
                "created_at": contact.created_at,
            })
        })
        .collect::<Vec<_>>();
    let routes = creators
        .iter()
        .flat_map(|creator| {
            creator.contacts.iter().filter_map(|contact| {
                contact.route_id.as_ref().map(|route_id| {
                    serde_json::json!({
                        "id": route_id,
                        "creator_id": creator.creator_id,
                        "locator_id": contact.locator_id,
                        "confidence": contact.incoming.confidence,
                        "created_at": contact.created_at,
                    })
                })
            })
        })
        .collect::<Vec<_>>();
    let locator_observations = creators
        .iter()
        .flat_map(|creator| {
            creator.contacts.iter().flat_map(|contact| {
                contact.observations.iter().map(|observation| {
                    serde_json::json!({
                        "id": observation.id,
                        "locator_id": contact.locator_id,
                        "source_id": observation.source_id,
                        "creator_id": creator.creator_id,
                        "observed_value": contact.incoming.display_value,
                        "observed_at": observation.observed_at,
                    })
                })
            })
        })
        .collect::<Vec<_>>();
    Ok(MaterializedImport {
        sources_json: serde_json::to_string(&sources).map_err(|error| error.to_string())?,
        creator_inserts_json: serde_json::to_string(&creator_inserts)
            .map_err(|error| error.to_string())?,
        creator_updates_json: serde_json::to_string(&creator_updates)
            .map_err(|error| error.to_string())?,
        accounts_json: serde_json::to_string(&accounts).map_err(|error| error.to_string())?,
        links_json: serde_json::to_string(&links).map_err(|error| error.to_string())?,
        link_updates_json: serde_json::to_string(&link_updates)
            .map_err(|error| error.to_string())?,
        profiles_json: serde_json::to_string(&profiles).map_err(|error| error.to_string())?,
        locators_json: serde_json::to_string(&locators).map_err(|error| error.to_string())?,
        routes_json: serde_json::to_string(&routes).map_err(|error| error.to_string())?,
        locator_observations_json: serde_json::to_string(&locator_observations)
            .map_err(|error| error.to_string())?,
        source_metadata_json: serde_json::json!({
            "format": bundle.format,
            "format_version": bundle.format_version,
            "source_notes": creators.iter().filter_map(|c| c.incoming.notes.as_ref().map(|n| (c.creator_id.as_str().to_owned(), n.clone()))).collect::<BTreeMap<_,_>>()
        })
        .to_string(),
        timestamp,
        #[cfg(test)]
        fail_before_table: None,
    })
}

fn transfer_platform(platform: crate::Platform) -> CreatorTransferPlatform {
    match platform {
        crate::Platform::X => CreatorTransferPlatform::X,
        crate::Platform::Instagram => CreatorTransferPlatform::Instagram,
        crate::Platform::Telegram => CreatorTransferPlatform::Telegram,
        crate::Platform::YouTube => CreatorTransferPlatform::YouTube,
        crate::Platform::TikTok => CreatorTransferPlatform::TikTok,
    }
}
